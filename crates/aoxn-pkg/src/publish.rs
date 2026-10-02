//! `aoxn publish` / `aoxn yank` / `aoxn build` — the release side.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tarball::manifest_hash;
use crate::cache::sha256_file;
use crate::context::Ctx;
use crate::errors::PkgError;
use crate::manifest::Manifest;
use crate::registry::{PublishOutcome, PublishRequest};
use crate::tarball;

pub fn run(allow_dirty: bool, dry_run: bool) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let mut ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let manifest = ctx.manifest.clone();
    let version = manifest.version.clone();

    // ---- preflight
    let Some(entry) = manifest.entry_file(&ctx.project) else {
        return Err(PkgError::Manifest(format!(
            "`{}` declares no entry: set \"main\" in aoxn.json or add index.ax/main.ax",
            manifest.name
        )));
    };
    if !ctx.project.join(&entry).exists() {
        return Err(PkgError::Manifest(format!(
            "entry file `{}` does not exist",
            entry.display()
        )));
    }

    check_dirty(&ctx, allow_dirty)?;
    changelog_check(&ctx, &version, &ui);

    // ---- pack
    let tmp = std::env::temp_dir().join(format!(
        "aoxn-publish-{}-{}",
        manifest.name, std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let tgz = tmp.join(format!("{}.tar.gz", manifest.name));
    let step = ui.step(&format!("packing {}@{}", manifest.name, version));
    // integrity = manifest hash over the tree (immune to re-packing),
    // computed BEFORE packing (the tarball itself is excluded from the walk)
    let checksum = manifest_hash(&ctx.project)?;
    let files = tarball::pack(&ctx.project, &manifest.name, &tgz)?;
    // transport digest: sha256 of the tarball bytes as they will be served.
    // A mirror or HTTP frontend can then reject a corrupted transfer before
    // it ever reaches a cache; `checksum` stays the content anchor.
    let tarball_sha256 = sha256_file(&tgz)?;
    step.done_ok_msg(&format!("{files} files, manifest {checksum}"));

    // ---- registry handshake
    let url = ctx.registry_url(None)?;
    ui.info(&format!("publishing to {url}"));

    let mut deps: BTreeMap<String, String> = BTreeMap::new();
    for (name, spec) in manifest.dependencies.iter().chain(&manifest.dev_dependencies) {
        if let crate::manifest::DependencySpec::Registry { req, .. } = spec {
            deps.insert(name.clone(), req.clone());
        }
    }
    // Record the compiler this package was published with as its floor, so
    // install can refuse the package on an older compiler instead of failing
    // somewhere deep in a compile.
    let req = PublishRequest {
        name: manifest.name.clone(),
        version: version.clone(),
        tarball: tgz.clone(),
        checksum: checksum.clone(),
        tarball_sha256: Some(tarball_sha256),
        dependencies: deps,
        aoxn: Some(env!("CARGO_PKG_VERSION").to_string()),
    };
    let reg = ctx.registry_for(&url)?;
    let outcome = reg.publish(&req, dry_run)?;
    let _ = std::fs::remove_dir_all(&tmp);

    match outcome {
        PublishOutcome::Published => {
            if dry_run {
                ui.success(&format!(
                    "dry run: {name}@{version} would be published (checksum {checksum})",
                    name = manifest.name
                ));
            } else {
                ui.success(&format!(
                    "published {name}@{version} (tag pkg/{name}/v{version})",
                    name = manifest.name
                ));
            }
        }
        PublishOutcome::AlreadyPublished => {
            ui.success(&format!(
                "{name}@{version} was already published with the same checksum — nothing to do (idempotent retry)",
                name = manifest.name
            ));
        }
    }
    Ok(0)
}

/// Warn (or fail) when the package lives in a dirty git worktree.
fn check_dirty(ctx: &Ctx, allow_dirty: bool) -> Result<(), PkgError> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&ctx.project)
        .output();
    let Ok(out) = out else {
        return Ok(()); // not a git repo (or no git): nothing to check
    };
    if !out.status.success() {
        return Ok(());
    }
    let dirty = !String::from_utf8_lossy(&out.stdout).trim().is_empty();
    if dirty {
        if allow_dirty {
            crate::ui::current().warn("git worktree has uncommitted changes (--allow-dirty)");
        } else {
            return Err(PkgError::other(
                "git worktree has uncommitted changes; commit them or pass --allow-dirty",
            ));
        }
    }
    Ok(())
}

/// Advisory check: warn when CHANGELOG.md exists but doesn't mention the
/// version being published (silent when there is no changelog).
fn changelog_check(ctx: &Ctx, version: &str, ui: &crate::ui::Ui) {
    let cl = ctx.project.join("CHANGELOG.md");
    let Ok(text) = std::fs::read_to_string(&cl) else {
        return;
    };
    if !text.contains(version) {
        ui.warn(&format!(
            "CHANGELOG.md has no entry for {version} (recommended before publishing)"
        ));
    }
}

// ---------------------------------------------------------------------------
// yank

pub fn yank(name: &str, version: &str, undo: bool, dry_run: bool) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let mut ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let url = ctx.registry_url(None)?;
    let reg = ctx.registry_for(&url)?;
    if !dry_run {
        reg.set_yank(name, version, !undo)?;
    }
    if undo {
        ui.success(&format!("un-yanked {name}@{version}"));
    } else {
        ui.success(&format!(
            "yanked {name}@{version} (fresh resolution refuses it; existing lockfiles keep working)"
        ));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// build (workspace-aware)

/// Build all packages of the project (workspace members in dependency
/// order, or the single current package). Delegates each member to the
/// compiler (`aoxn build <entry>`), which has its own content-hash cache,
/// so unchanged members are incremental no-ops.
pub fn build() -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let mut pkgs = ctx.all_package_manifests()?;

    // topological order over path dependencies among the members
    pkgs.sort_by(|a, b| a.0.cmp(&b.0));
    let order = topo_sort(&pkgs)?;

    let self_exe = std::env::current_exe()?;
    let mut failed = false;
    for dir in &order {
        let m = Manifest::load(&Manifest::path_for(dir))?;
        let Some(entry) = m.entry_file(dir) else {
            return Err(PkgError::Manifest(format!(
                "`{}` has no entry file (set \"main\" or add index.ax)",
                m.name
            )));
        };
        let entry_path: PathBuf = dir.join(&entry);
        let out_dir = dir.join("target");
        std::fs::create_dir_all(&out_dir)?;
        let exe = out_dir.join(&m.name);
        let step = ui.step(&format!("build {} ({})", m.name, entry.display()));
        let status = Command::new(&self_exe)
            .arg("build")
            .arg(&entry_path)
            .arg("-o")
            .arg(&exe)
            .status()
            .map_err(|e| PkgError::other(format!("cannot spawn aoxn compiler: {e}")))?;
        if status.success() {
            step.done_ok("");
        } else {
            step.done_fail("");
            failed = true;
            break;
        }
    }
    if failed {
        Err(PkgError::other("build failed"))
    } else {
        ui.success(&format!("built {} package(s)", order.len()));
        Ok(0)
    }
}

/// Topological order of package dirs by path-dependency membership.
fn topo_sort(pkgs: &[(PathBuf, Manifest)]) -> Result<Vec<PathBuf>, PkgError> {
    let dir_of: BTreeMap<String, PathBuf> = pkgs
        .iter()
        .map(|(d, m)| (m.name.clone(), d.clone()))
        .collect();
    let mut deps_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, m) in pkgs {
        let mut deps = Vec::new();
        for spec in m.dependencies.values() {
            if let crate::manifest::DependencySpec::Path { path } = spec {
                // resolve to a member name if it points at one
                if let Ok(Some((dep_name, _))) = path_target_name(&dir_of, path) {
                    deps.push(dep_name);
                }
            }
        }
        deps_of.insert(m.name.clone(), deps);
    }
    let mut order: Vec<PathBuf> = Vec::new();
    let mut done: std::collections::HashSet<String> = Default::default();
    let mut visiting: std::collections::HashSet<String> = Default::default();
    fn visit(
        name: &str,
        deps_of: &BTreeMap<String, Vec<String>>,
        dir_of: &BTreeMap<String, PathBuf>,
        done: &mut std::collections::HashSet<String>,
        visiting: &mut std::collections::HashSet<String>,
        order: &mut Vec<PathBuf>,
    ) -> Result<(), PkgError> {
        if done.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name.to_string()) {
            return Err(PkgError::other(format!(
                "dependency cycle between workspace members at `{name}`"
            )));
        }
        if let Some(deps) = deps_of.get(name) {
            for d in deps.clone() {
                visit(&d, deps_of, dir_of, done, visiting, order)?;
            }
        }
        visiting.remove(name);
        done.insert(name.to_string());
        if let Some(dir) = dir_of.get(name) {
            order.push(dir.clone());
        }
        Ok(())
    }
    for (name, _) in &deps_of {
        visit(name, &deps_of, &dir_of, &mut done, &mut visiting, &mut order)?;
    }
    Ok(order)
}

fn path_target_name(
    dir_of: &BTreeMap<String, PathBuf>,
    path: &Path,
) -> std::io::Result<Option<(String, PathBuf)>> {
    for (name, dir) in dir_of {
        let candidate = dir.join(path);
        if candidate.canonicalize().ok() == dir.canonicalize().ok() {
            return Ok(Some((name.clone(), dir.clone())));
        }
    }
    Ok(None)
}
