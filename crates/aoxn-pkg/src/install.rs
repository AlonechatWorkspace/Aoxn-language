//! Install: resolve → lock → materialize `aox_modules/`.
//!
//! Materialization rules (what makes `import { x } from "pkg"` work with a
//! **zero-change compiler**):
//! - registry deps: the tarball is extracted to `<pkg>/aox_modules/<name>/`;
//!   if the package's entry file is not a root-level `index.ax`, a shim
//!   `index.ax` (`import * from "./<entry>"`) is generated — the loader
//!   merges names transitively, so the shim is fully transparent.
//! - path deps (workspace members): `<pkg>/aox_modules/<name>/` contains
//!   only a shim `index.ax` whose import path points at the member's real
//!   entry file — sources of truth stay in place, edits are live.
//! - everything under `aox_modules/` is managed: stale entries are pruned
//!   on every install.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::context::Ctx;
use crate::errors::PkgError;
use crate::lockfile::{Lockfile, LockedPackage};
use crate::manifest::{DependencySpec, Manifest};
use crate::resolve::{resolve_deps, Resolution};
use crate::tarball;
use crate::ui::Ui;

pub struct InstallOpts {
    pub force_resolve: bool,
    /// CI mode: never resolve or touch the lockfile; fail when it does not
    /// exactly satisfy the manifests
    pub frozen: bool,
    pub dry_run: bool,
}

pub struct InstallReport {
    pub resolved: usize,
    pub installed: Vec<String>,
    pub removed: Vec<String>,
    pub reused_cache: usize,
}

/// Compute the manifest requirements fingerprint: `name -> "req@registry"`.
/// The lockfile stores it; when it matches, install skips re-resolution
/// entirely (fast path, works offline).
fn requirements_fingerprint(packages: &[(PathBuf, Manifest)]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (dir, m) in packages {
        for (dep, spec) in &m.dependencies {
            if let DependencySpec::Registry { req, registry } = spec {
                let url = registry.clone().unwrap_or_default();
                out.insert(dep.clone(), format!("{req}@{url}@{}", dir.display()));
            }
        }
    }
    out
}

pub fn install(ctx: &mut Ctx, ui: &Ui, opts: InstallOpts) -> Result<InstallReport, PkgError> {
    let packages = ctx.all_package_manifests()?;
    let old_lock = ctx.load_lockfile()?.unwrap_or_else(Lockfile::empty);

    // ---- 1. decide whether resolution is needed
    let reqs = requirements_fingerprint(&packages);
    let lock_matches_manifest = old_lock.requirements == reqs;
    let needs_resolve = opts.force_resolve || old_lock.packages.is_empty() || !lock_matches_manifest;
    if !needs_resolve {
        // in-sync fast path: belt-and-braces — every manifest constraint
        // must be satisfied by the locked versions
        if let Err(why) = old_lock.satisfies_manifests(&reqs) {
            if opts.frozen {
                return Err(PkgError::Lockfile(format!(
                    "--frozen: lockfile does not satisfy the manifests: {why}"
                )));
            }
            // stale-but-recoverable: fall through to a re-resolve
            return install(
                ctx,
                ui,
                InstallOpts {
                    force_resolve: true,
                    frozen: false,
                    dry_run: opts.dry_run,
                },
            );
        }
    }
    if opts.frozen && needs_resolve {
        return Err(PkgError::Lockfile(format!(
            "--frozen: {} is out of date with the manifests (run `aoxn install` once to update it,              or commit a fresh aoxn.lock in CI)",
            ctx.lockfile_path().display()
        )));
    }

    let resolution: Resolution;
    let mut lock = old_lock.clone();
    let mut newly_added: Vec<String> = Vec::new();
    let path_deps = ctx.path_dependencies()?;
    if needs_resolve {
        let roots = ctx.roots()?;
        let default_registry = if roots.is_empty() {
            String::new()
        } else {
            ctx.registry_url(None)?
        };
        let locked = old_lock.locked_versions();
        let prev: HashSet<String> = locked.keys().cloned().collect();
        let res = resolve_deps(roots, ctx, &default_registry, &locked)?;

        // typosquat guard: newly added packages compared against registry names
        let added = crate::resolve::added_packages(&res, &prev);
        newly_added = added.clone();
        typosquat_check(ctx, ui, &added)?;

        // yank / deprecation warnings for everything selected
        for (name, sel) in &res.packages {
            if sel.yanked {
                ui.warn(&format!(
                    "{name}@{} is yanked in the registry but pinned in your lockfile; plan a migration (aoxn update --latest {name})",
                    sel.version
                ));
            }
            if let Some(msg) = &sel.deprecated {
                ui.warn(&format!(
                    "{name}@{} is deprecated{}",
                    sel.version,
                    if msg.is_empty() { String::new() } else { format!(": {msg}") }
                ));
            }
        }

        // ---- 2. write the new lockfile
        let mut new_lock = Lockfile::empty();
        new_lock.requirements = reqs.clone();
        for (name, sel) in &res.packages {
            new_lock.insert(LockedPackage {
                name: name.clone(),
                version: sel.version.clone(),
                registry: sel.registry.clone(),
                checksum: sel.checksum.clone(),
                dependencies: sel.dependencies.keys().map(|d| d.clone()).collect(),
            });
        }
        // path dependencies are part of the lock too, marked `path+<abs>`
        // with an empty checksum (their source of truth is the local tree)
        for (name, target) in &path_deps {
            if new_lock.get(name).is_some() {
                continue;
            }
            let m = Manifest::load(&Manifest::path_for(target))?;
            new_lock.insert(LockedPackage {
                name: name.to_string(),
                version: m.version,
                registry: format!("path+{}", target.display()),
                checksum: String::new(),
                dependencies: m
                    .dependencies
                    .keys()
                    .map(|d| d.clone())
                    .collect(),
            });
        }
        report_lock_diff(ui, &old_lock, &new_lock);
        lock = new_lock;
        resolution = res;
    } else {
        // lockfile is authoritative — no resolution, no network
        ui.info(&format!(
            "using locked versions from {} ({} packages)",
            ctx.lockfile_path().display(),
            lock.packages.len()
        ));
        // build a pseudo-resolution from the lock for materialization
        let mut pkgs = BTreeMap::new();
        for lp in &lock.packages {
            pkgs.insert(
                lp.name.clone(),
                crate::resolve::Selected {
                    version: lp.version.clone(),
                    registry: lp.registry.clone(),
                    checksum: lp.checksum.clone(),
                    dependencies: lp
                        .dependencies
                        .iter()
                        .filter_map(|d| {
                            d.split_once('@').map(|(n, v)| (n.to_string(), v.to_string()))
                        })
                        .collect(),
                    deprecated: None,
                    yanked: false,
                },
            );
        }
        resolution = Resolution { packages: pkgs };
    }

    if opts.dry_run {
        return Ok(InstallReport {
            resolved: lock.packages.len(),
            installed: newly_added,
            removed: diff_removed(&old_lock, &lock),
            reused_cache: 0,
        });
    }

    // ---- 3. prefetch tarballs (grouped per registry, verified against the
    //         lockfile checksums)
    let mut reused = 0usize;
    let mut by_registry: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for (name, sel) in &resolution.packages {
        by_registry
            .entry(sel.registry.clone())
            .or_default()
            .push((name.clone(), sel.version.clone(), sel.checksum.clone()));
    }
    let offline = ctx.offline;
    let cache = ctx.cache.clone();
    for (url, wanted) in &by_registry {
        let reg = ctx.registry_for(url)?;
        for (name, version, checksum) in wanted {
            let path = cache.tar_path(checksum);
            if path.exists() {
                reused += 1;
                continue;
            }
            let step = ui.step(&format!("fetching {name}@{version}"));
            match reg.fetch_tarball(&cache, name, version, checksum, offline) {
                Ok(_) => step.done_ok(""),
                Err(e) => {
                    step.done_fail("");
                    return Err(e);
                }
            }
        }
    }

    // ---- 4. materialize aox_modules for every package dir
    let mut removed_total: Vec<String> = Vec::new();
    for (dir, manifest) in &packages {
        let (installed, removed) = materialize(
            dir,
            manifest,
            &resolution,
            &lock,
            &path_deps,
            ui,
        )?;
        removed_total.extend(removed);
        let _ = installed;
    }

    // ---- 5. persist the lockfile
    if needs_resolve {
        lock.save(&ctx.lockfile_path())?;
        ui.info(&format!(
            "wrote {} ({} packages)",
            ctx.lockfile_path().display(),
            lock.packages.len()
        ));
    }

    Ok(InstallReport {
        resolved: lock.packages.len(),
        installed: newly_added,
        removed: removed_total,
        reused_cache: reused,
    })
}

/// Materialize one package dir's `aox_modules/`. Returns (installed, removed).
fn materialize(
    dir: &Path,
    manifest: &Manifest,
    resolution: &Resolution,
    lock: &Lockfile,
    path_deps: &[(String, PathBuf)],
    ui: &Ui,
) -> Result<(Vec<String>, Vec<String>), PkgError> {
    let modules_dir = dir.join("aox_modules");
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let mut installed = Vec::new();

    // registry deps of this manifest
    for (dep, spec) in &manifest.dependencies {
        if let DependencySpec::Registry { .. } = spec {
            keep.insert(dep.clone());
            if !resolution.packages.contains_key(dep) {
                if let Some(lp) = lock.get(dep) {
                    // resolution skipped this dep (shouldn't happen), but the
                    // lock knows it — install from the lock anyway
                    install_registry_dep(&modules_dir, dep, &lp.version, &lp.checksum)?;
                    installed.push(dep.clone());
                }
                continue;
            }
            let sel = &resolution.packages[dep];
            install_registry_dep(&modules_dir, dep, &sel.version, &sel.checksum)?;
            installed.push(dep.clone());
        }
    }

    // path deps (workspace members / local packages)
    for (name, target) in path_deps {
        // only materialize for dirs that actually depend on it
        let depends = manifest
            .dependencies
            .iter()
            .any(|(d, s)| matches!(s, DependencySpec::Path { .. }) && d == name)
            || manifest.workspace.is_some();
        if !depends {
            continue;
        }
        keep.insert(name.clone());
        let target_manifest = Manifest::load(&Manifest::path_for(target))?;
        let Some(entry) = target_manifest.entry_file(target) else {
            return Err(PkgError::Manifest(format!(
                "path dependency `{name}` ({}): no entry file and no `main` in its manifest",
                target.display()
            )));
        };
        let shim_dir = modules_dir.join(name);
        std::fs::create_dir_all(&shim_dir)?;
        let rel = relative_path(&shim_dir, &target.join(&entry));
        std::fs::write(shim_dir.join("index.ax"), format!("import * from \"{rel}\"\n"))?;
        installed.push(name.clone());
    }

    // prune managed-but-stale entries
    let mut removed = Vec::new();
    if modules_dir.exists() {
        for e in std::fs::read_dir(&modules_dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            if !keep.contains(&name) {
                let p = e.path();
                if p.is_dir() {
                    std::fs::remove_dir_all(&p)?;
                } else {
                    std::fs::remove_file(&p)?;
                }
                removed.push(name);
            }
        }
    }
    let _ = ui;
    Ok((installed, removed))
}

/// Extract a locked registry dep into `modules_dir/<name>` (verify checksum
/// first — the cache entry is content-addressed, but verify anyway: cheap
/// and it pins the supply chain to the lockfile).
fn install_registry_dep(
    modules_dir: &Path,
    name: &str,
    version: &str,
    checksum: &str,
) -> Result<(), PkgError> {
    let cache = crate::cache::Cache::global();
    let tarball = cache.tar_path(checksum);
    if !tarball.exists() {
        return Err(PkgError::Offline(format!(
            "{name}@{version} is not cached yet and resolution was skipped; \
             run `aoxn install` online once"
        )));
    }
    let dest = modules_dir.join(name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::create_dir_all(modules_dir)?;
    tarball::unpack(&tarball, &dest, name)?;
    // integrity: the extracted tree must hash to the lockfile checksum.
    // This is the supply-chain anchor — what you compile is what the
    // lockfile describes, regardless of how the tarball was packed.
    let actual = match crate::tarball::manifest_hash(&dest) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(e);
        }
    };
    if actual != checksum {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(PkgError::Integrity {
            name: name.to_string(),
            version: version.to_string(),
        });
    }
    write_shim_if_needed(&dest)?;
    Ok(())
}

/// If the package's entry is not a root `index.ax`, generate one so the
/// compiler's `aox_modules/<name>` → `index.ax` probe reaches it.
fn write_shim_if_needed(pkg_dir: &Path) -> Result<(), PkgError> {
    let manifest = Manifest::load(&Manifest::path_for(pkg_dir))?;
    if let Some(entry) = manifest.entry_file(pkg_dir) {
        if entry == Path::new("index.ax") {
            return Ok(());
        }
        let rel = format!("/{}", entry.to_string_lossy().replace('\\', "/"));
        std::fs::write(
            pkg_dir.join("index.ax"),
            format!("import * from \".{rel}\"\n"),
        )?;
    }
    Ok(())
}

/// Relative import path from `from_dir` to `to_file`, POSIX style.
fn relative_path(from_dir: &Path, to_file: &Path) -> String {
    let from_abs = from_dir
        .canonicalize()
        .unwrap_or_else(|_| from_dir.to_path_buf());
    let to_abs = to_file
        .canonicalize()
        .unwrap_or_else(|_| to_file.to_path_buf());
    let from_parts: Vec<_> = from_abs.components().collect();
    let to_parts: Vec<_> = to_abs.components().collect();
    let mut common = 0;
    while common < from_parts.len() && common < to_parts.len() && from_parts[common] == to_parts[common]
    {
        common += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..from_parts.len() {
        parts.push("..".into());
    }
    for c in &to_parts[common..] {
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

fn diff_removed(old: &Lockfile, new: &Lockfile) -> Vec<String> {
    old.packages
        .iter()
        .filter(|p| new.get(&p.name).is_none())
        .map(|p| format!("{}@{}", p.name, p.version))
        .collect()
}

fn report_lock_diff(ui: &Ui, old: &Lockfile, new: &Lockfile) {
    for p in &new.packages {
        match old.get(&p.name) {
            None => ui.info(&format!("+ {}@{} ({})", p.name, p.version, p.registry)),
            Some(prev) if prev.version != p.version => {
                ui.info(&format!("~ {}: {} -> {}", p.name, prev.version, p.version))
            }
            _ => {}
        }
    }
    for name in diff_removed(old, new) {
        ui.info(&format!("- {name}"));
    }
}

/// Typosquat guard: warn when a newly added package name is confusingly
/// similar to an existing registry package.
fn typosquat_check(ctx: &mut Ctx, ui: &Ui, added: &[String]) -> Result<(), PkgError> {
    if added.is_empty() {
        return Ok(());
    }
    // best effort: no registry configured → skip silently
    let Ok(url) = ctx.registry_url(None) else {
        return Ok(());
    };
    let names = match ctx.registry_for(&url) {
        Ok(r) => r.all_names().unwrap_or_default(),
        Err(_) => return Ok(()),
    };
    for a in added {
        for existing in &names {
            if existing == a {
                continue;
            }
            if edit_distance(a, existing) <= typo_threshold(a) {
                ui.warn(&format!(
                    "`{a}` looks similar to existing package `{existing}` — \
                     double-check the spelling (typosquat protection)"
                ));
            }
        }
    }
    Ok(())
}

/// Similarity threshold scaled by name length: short names are noisy at
/// distance 2 (`ax` vs `ao`), so only names of 8+ chars admit it.
fn typo_threshold(name: &str) -> usize {
    if name.chars().count() >= 8 {
        2
    } else {
        1
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_basics() {
        assert_eq!(edit_distance("http", "http"), 0);
        assert_eq!(edit_distance("http", "htp"), 1);
        assert_eq!(edit_distance("htpps", "http"), 2);
        assert!(edit_distance("totally-different", "http") > 2);
    }

    #[test]
    fn relative_paths() {
        let tmp = std::env::temp_dir().join(format!("aoxn-rel-{}", std::process::id()));
        let from = tmp.join("app").join("aox_modules").join("lib");
        let to = tmp.join("packages").join("lib").join("src").join("main.ax");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&to, "").unwrap();
        let rel = relative_path(&from, &to);
        assert!(rel.starts_with("../"), "got {rel}");
        assert!(rel.ends_with("main.ax"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
