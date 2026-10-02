//! Project lifecycle commands: init / add / remove / install / update.

use std::collections::BTreeMap;

use crate::context::{Ctx, DepScope};
use crate::errors::PkgError;
use crate::install::{install, InstallOpts, DEFAULT_JOBS};
use crate::manifest::{DependencySpec, Manifest, RegistryDecl};
use crate::ui::Ui;

fn ui() -> Ui {
    crate::ui::current()
}

// ---------------------------------------------------------------------------
// init

pub fn init(registry: Option<String>, name: Option<String>) -> Result<i32, PkgError> {
    let ui = ui();
    let dir = std::env::current_dir().map_err(PkgError::Io)?;
    let manifest_path = Manifest::path_for(&dir);
    if manifest_path.exists() {
        return Err(PkgError::Manifest(format!(
            "{} already exists",
            manifest_path.display()
        )));
    }
    let pkg_name = name.unwrap_or_else(|| {
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "my-package".into());
        sanitize_name(&dir_name)
    });
    let main = "main.ax";
    let manifest = Manifest {
        name: pkg_name.clone(),
        version: "0.1.0".into(),
        main: Some(main.into()),
        types: None,
        exports: BTreeMap::new(),
        description: None,
        dependencies: BTreeMap::new(),
        dev_dependencies: BTreeMap::new(),
        overrides: BTreeMap::new(),
        workspace: None,
        registries: registry
            .map(|url| {
                BTreeMap::from([(
                    "default".to_string(),
                    RegistryDecl { url, kind: None },
                )])
            })
            .unwrap_or_default(),
    };
    manifest.validate()?;
    if !dir.join(main).exists() {
        std::fs::write(
            dir.join(main),
            "def main():\n    print(\"hello from aoxn\\n\")\n\nmain()\n",
        )?;
    }
    manifest.save(&manifest_path)?;
    ui.success(&format!("created {pkg_name} 0.1.0 ({})", manifest_path.display()));
    ui.info("next steps:");
    ui.info("  aoxn add <package>     # add a dependency");
    ui.info("  aoxn run main.ax       # run it (via the compiler)");
    Ok(0)
}

fn sanitize_name(dir_name: &str) -> String {
    let cleaned: String = dir_name
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
                c
            } else if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "my-package".into()
    } else {
        trimmed
    }
}

// ---------------------------------------------------------------------------
// add

pub fn add(
    pkgs: &[String],
    dev: bool,
    registry: Option<&str>,
    dry_run: bool,
    offline: bool,
) -> Result<i32, PkgError> {
    if pkgs.is_empty() {
        return Err(PkgError::Manifest(
            "nothing to add: pass packages like `aoxn add http@^2`".into(),
        ));
    }
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();
    let mut parsed: Vec<(String, Option<String>)> = Vec::new();
    for spec in pkgs {
        let (name, req) = match spec.split_once('@') {
            Some((n, r)) => (n.to_string(), Some(r.to_string())),
            None => (spec.clone(), None),
        };
        crate::manifest::validate_name(&name)?;
        parsed.push((name, req));
    }

    // A dependency may live on a named registry. The explicit --registry
    // wins; otherwise keep whatever the manifest already recorded — dropping
    // it silently rebound packages to the default registry, which made
    // `aoxn add` unable to ever target a second source.
    let registry_for = |ctx: &Ctx, name: &str| -> Option<String> {
        if registry.is_some() {
            return registry.map(String::from);
        }
        for table in [&ctx.manifest.dependencies, &ctx.manifest.dev_dependencies] {
            if let Some(DependencySpec::Registry { registry: Some(r), .. }) = table.get(name) {
                return Some(r.clone());
            }
        }
        None
    };

    // discover latest versions for unpinned adds (also warms the registry)
    let mut resolved_reqs: Vec<(String, String)> = Vec::new();
    for (name, req) in &parsed {
        match req {
            Some(r) => resolved_reqs.push((name.clone(), r.clone())),
            None => {
                let url = ctx.registry_url(registry_for(&ctx, name).as_deref())?;
                let idx = {
                    let reg = ctx.registry_for(&url)?;
                    reg.index(name)?
                };
                let Some((latest, entry)) = idx.latest(false) else {
                    return Err(PkgError::Registry(format!(
                        "`{name}` has no installable (non-yanked) version"
                    )));
                };
                if let Some(msg) = &entry.deprecated {
                    ui.warn(&format!("{name}@{latest} is deprecated: {msg}"));
                }
                if let Some(tier) = entry_tier(&mut ctx, &url, name) {
                    if !tier.reviewed() {
                        ui.warn(&format!(
                            "{name} has no reviewed trust record in this registry (tier: {})",
                            tier.as_str()
                        ));
                    }
                }
                let skipped = idx.versions.keys().filter(|k| k.contains('-')).count();
                if skipped > 0 {
                    ui.info(&format!(
                        "adding {name}@{latest} (newest; {skipped} pre-release version(s) are not considered by `^` ranges — pin one explicitly with {name}@<version> if wanted)"
                    ));
                } else {
                    ui.info(&format!("adding {name}@{latest} (newest)"));
                }
                resolved_reqs.push((name.clone(), format!("^{latest}")));
            }
        }
    }

    for (name, req) in &resolved_reqs {
        if req.parse::<semver::VersionReq>().is_err() {
            return Err(PkgError::Manifest(format!(
                "invalid requirement `{req}` for `{name}`"
            )));
        }
        let reg = registry_for(&ctx, name);
        let spec = DependencySpec::Registry { req: req.clone(), registry: reg };
        // adding a dev dependency that is already a prod one updates the prod
        // entry instead of silently shadowing it
        let target = if dev && !ctx.manifest.dependencies.contains_key(name) {
            &mut ctx.manifest.dev_dependencies
        } else {
            &mut ctx.manifest.dependencies
        };
        target.insert(name.clone(), spec);
    }
    if !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    let report = install(&mut ctx, &ui, InstallOpts { force_resolve: true, dry_run, ..Default::default() })?;
    if !dry_run {
        let table = if dev { "devDependencies" } else { "dependencies" };
        ui.success(&format!(
            "added {} to {table} ({} packages resolved)",
            pkgs.join(", "),
            report.resolved
        ));
    }
    Ok(0)
}

/// The registry's recorded review tier for a package, when the registry
/// ships a trust index at all.
fn entry_tier(ctx: &mut Ctx, url: &str, name: &str) -> Option<crate::trust::Tier> {
    let trust = crate::trust::index_of(ctx, url).ok()??;
    Some(trust.tier_of(name))
}

// ---------------------------------------------------------------------------
// remove / uninstall

pub fn remove(pkgs: &[String], dev: bool, dry_run: bool, offline: bool) -> Result<i32, PkgError> {
    if pkgs.is_empty() {
        return Err(PkgError::Manifest(
            "nothing to remove: pass package names".into(),
        ));
    }
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();
    let mut removed = Vec::new();
    for name in pkgs {
        // With an explicit table, honor it. Without one, take the name out of
        // both — a package listed as both prod and dev is only really gone
        // when both entries are.
        let hit = if dev {
            ctx.manifest.dev_dependencies.remove(name).is_some()
        } else if ctx.manifest.dependencies.remove(name).is_some() {
            true
        } else {
            let from_dev = ctx.manifest.dev_dependencies.remove(name).is_some();
            if !dev {
                ui.info(&format!("`{name}` was a devDependency; removed it there too"));
            }
            from_dev
        };
        if hit {
            removed.push(name.clone());
        } else {
            ui.warn(&format!("{name} is not a dependency of {}", ctx.manifest.name));
        }
    }
    if !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    install(&mut ctx, &ui, InstallOpts { dry_run, ..Default::default() })?;
    if !dry_run {
        ui.success(&format!("removed {}", removed.join(", ")));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// install

/// Resolve `--prod` / `--dev-only` into a scope. clap marks the two flags as
/// conflicting, so only one can be set.
pub fn scope_from_flags(prod: bool, dev_only: bool) -> DepScope {
    if prod {
        DepScope::Prod
    } else if dev_only {
        DepScope::DevOnly
    } else {
        DepScope::All
    }
}

/// Download concurrency: explicit flag, else `AOXN_JOBS`, else the default.
/// `--jobs 1` forces the old serial behavior.
pub fn jobs_from_flag(jobs: Option<usize>) -> usize {
    if let Some(n) = jobs {
        return n.max(1);
    }
    std::env::var("AOXN_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_JOBS)
}

pub fn install_cmd(
    force: bool,
    frozen: bool,
    scope: DepScope,
    jobs: usize,
    dry_run: bool,
    offline: bool,
    json: bool,
) -> Result<i32, PkgError> {
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();
    let report = install(
        &mut ctx,
        &ui,
        InstallOpts {
            force_resolve: force,
            frozen,
            dry_run,
            scope,
            jobs,
        },
    )?;
    if json {
        ui.out_json(&report.to_json());
        return Ok(0);
    }
    if dry_run {
        ui.out(&format!("would ensure {} packages installed", report.resolved));
        for p in &report.installed {
            ui.out(&format!("  + {p}"));
        }
        for p in &report.removed {
            ui.out(&format!("  - {p}"));
        }
        for p in &report.skipped_dev {
            ui.out(&format!("  - {p} (dev-only, excluded from a production install)"));
        }
    } else {
        let mut line = format!(
            "{} packages ready ({} from cache, {} downloaded)",
            report.resolved, report.reused_cache, report.downloaded
        );
        if !report.skipped_dev.is_empty() {
            line.push_str(&format!(
                ", {} dev-only left out",
                report.skipped_dev.len()
            ));
        }
        ui.success(&line);
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// update

pub fn update(
    pkgs: &[String],
    latest: bool,
    dev: bool,
    dry_run: bool,
    offline: bool,
    jobs: usize,
) -> Result<i32, PkgError> {
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();

    if pkgs.is_empty() {
        if latest {
            return Err(PkgError::Manifest(
                "`update --latest` needs explicit package names (it rewrites the manifest)"
                    .into(),
            ));
        }
        // refresh everything within the existing constraints
        let report = install(
            &mut ctx,
            &ui,
            InstallOpts { force_resolve: true, dry_run, jobs, ..Default::default() },
        )?;
        if !dry_run {
            ui.success(&format!("dependencies up to date ({} packages)", report.resolved));
        }
        return Ok(0);
    }

    let mut changed_manifest = false;
    for name in pkgs {
        // Read the spec first and drop the borrow — resolving the registry
        // URL and reading the index both need `&mut ctx`.
        let table: &BTreeMap<String, DependencySpec> = if dev {
            &ctx.manifest.dev_dependencies
        } else {
            &ctx.manifest.dependencies
        };
        let Some(spec) = table.get(name).cloned() else {
            return Err(PkgError::Manifest(if dev {
                format!("`{name}` is not a devDependency of {}", ctx.manifest.name)
            } else if ctx.manifest.dev_dependencies.contains_key(name) {
                format!(
                    "`{name}` is a devDependency of {} — use `aoxn update -D {name}`",
                    ctx.manifest.name
                )
            } else {
                format!("`{name}` is not a dependency of {}", ctx.manifest.name)
            }));
        };
        let url = match &spec {
            DependencySpec::Registry { registry, .. } => ctx.registry_url(registry.as_deref())?,
            DependencySpec::Path { .. } => {
                ui.warn(&format!("{name} is a path dependency; nothing to update"));
                continue;
            }
        };
        let idx = {
            let reg = ctx.registry_for(&url)?;
            reg.index(name)?
        };
        let Some((newest, entry)) = idx.latest(false) else {
            return Err(PkgError::Registry(format!("{name} has no installable version")));
        };
        if let Some(msg) = &entry.deprecated {
            ui.warn(&format!("{name}@{newest} is deprecated: {msg}"));
        }
        let (current_req, registry) = match &spec {
            DependencySpec::Registry { req, registry } => (req.clone(), registry.clone()),
            _ => unreachable!(),
        };
        let req = if latest { format!("^{newest}") } else { current_req };
        let current_locked = ctx
            .load_lockfile()?
            .and_then(|l| l.get(name).map(|p| p.version.clone()))
            .unwrap_or_default();
        let newest_req_ok = semver::VersionReq::parse(&req)
            .map(|r| r.matches(&semver::Version::parse(&newest).unwrap()))
            .unwrap_or(false);
        if !latest && newest_req_ok && current_locked == newest {
            ui.info(&format!("{name}: already at {newest} (newest within `{req}`)"));
            continue;
        }
        ui.info(&format!(
            "{name}: {} -> {newest}{}",
            if current_locked.is_empty() { "?" } else { &current_locked },
            if latest { " (constraint bumped)" } else { "" }
        ));
        // keep the registry binding the dependency already had
        let table = if dev {
            &mut ctx.manifest.dev_dependencies
        } else {
            &mut ctx.manifest.dependencies
        };
        table.insert(name.clone(), DependencySpec::Registry { req, registry });
        changed_manifest = true;
    }
    if changed_manifest && !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    let report = install(
        &mut ctx,
        &ui,
        InstallOpts { force_resolve: true, dry_run, jobs, ..Default::default() },
    )?;
    if !dry_run {
        ui.success(&format!("updated ({} packages)", report.resolved));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_flags_pick_the_right_slice() {
        assert_eq!(scope_from_flags(false, false), DepScope::All);
        assert_eq!(scope_from_flags(true, false), DepScope::Prod);
        assert_eq!(scope_from_flags(false, true), DepScope::DevOnly);
    }

    #[test]
    fn an_explicit_job_count_always_wins() {
        assert_eq!(jobs_from_flag(Some(3)), 3);
        // 0 would mean "no workers"; clamp to one rather than divide by zero
        assert_eq!(jobs_from_flag(Some(0)), 1);
    }

    #[test]
    fn without_a_flag_the_default_is_used() {
        assert_eq!(jobs_from_flag(None), DEFAULT_JOBS);
    }
}
