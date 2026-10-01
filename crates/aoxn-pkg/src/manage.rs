//! Project lifecycle commands: init / add / remove / install / update.

use std::collections::BTreeMap;

use crate::context::Ctx;
use crate::errors::PkgError;
use crate::install::{install, InstallOpts};
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
        description: None,
        dependencies: BTreeMap::new(),
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

pub fn add(pkgs: &[String], dry_run: bool, offline: bool) -> Result<i32, PkgError> {
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

    // discover latest versions for unpinned adds (also warms the registry)
    let mut resolved_reqs: Vec<(String, String)> = Vec::new();
    for (name, req) in &parsed {
        match req {
            Some(r) => resolved_reqs.push((name.clone(), r.clone())),
            None => {
                let url = ctx.registry_url(None)?;
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
                let skipped = idx.versions.keys().filter(|k| k.contains('-')).count();
                if skipped > 0 {
                    ui.info(&format!(
                        "adding {name}@{latest} (newest; {skipped} pre-release version(s) are not                          considered by `^` ranges — pin one explicitly with {name}@<version> if wanted)"
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
        ctx.manifest
            .dependencies
            .insert(name.clone(), DependencySpec::Registry { req: req.clone(), registry: None });
    }
    if !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    let report = install(
        &mut ctx,
        &ui,
        InstallOpts {
            force_resolve: true,
            frozen: false,
            dry_run,
        },
    )?;
    if !dry_run {
        ui.success(&format!(
            "added {} ({} packages resolved)",
            pkgs.join(", "),
            report.resolved
        ));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// remove / uninstall

pub fn remove(pkgs: &[String], dry_run: bool, offline: bool) -> Result<i32, PkgError> {
    if pkgs.is_empty() {
        return Err(PkgError::Manifest(
            "nothing to remove: pass package names".into(),
        ));
    }
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();
    let mut removed = Vec::new();
    for name in pkgs {
        if ctx.manifest.dependencies.remove(name).is_none() {
            ui.warn(&format!("{name} is not a dependency of {}", ctx.manifest.name));
        } else {
            removed.push(name.clone());
        }
    }
    if !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    install(
        &mut ctx,
        &ui,
        InstallOpts {
            force_resolve: false,
            frozen: false,
            dry_run,
        },
    )?;
    if !dry_run {
        ui.success(&format!("removed {}", removed.join(", ")));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// install

pub fn install_cmd(force: bool, frozen: bool, dry_run: bool, offline: bool) -> Result<i32, PkgError> {
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let ui = ui();
    let report = install(
        &mut ctx,
        &ui,
        InstallOpts {
            force_resolve: force,
            frozen,
            dry_run,
        },
    )?;
    if dry_run {
        ui.out(&format!("would ensure {} packages installed", report.resolved));
        for p in &report.installed {
            ui.out(&format!("  + {p}"));
        }
        for p in &report.removed {
            ui.out(&format!("  - {p}"));
        }
    } else {
        ui.success(&format!(
            "{} packages ready ({} from cache)",
            report.resolved, report.reused_cache
        ));
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// update

pub fn update(pkgs: &[String], latest: bool, dry_run: bool, offline: bool) -> Result<i32, PkgError> {
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
            InstallOpts {
                force_resolve: true,
                frozen: false,
                dry_run,
            },
        )?;
        if !dry_run {
            ui.success(&format!("dependencies up to date ({} packages)", report.resolved));
        }
        return Ok(0);
    }

    let mut changed_manifest = false;
    for name in pkgs {
        let spec = ctx.manifest.dependencies.get(name).cloned();
        let Some(spec) = spec else {
            return Err(PkgError::Manifest(format!(
                "`{name}` is not a dependency of {}",
                ctx.manifest.name
            )));
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
        let req = if latest {
            format!("^{newest}")
        } else {
            match spec {
                DependencySpec::Registry { req, .. } => req.clone(),
                _ => unreachable!(),
            }
        };
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
        ctx.manifest
            .dependencies
            .insert(name.clone(), DependencySpec::Registry { req, registry: None });
        changed_manifest = true;
    }
    if changed_manifest && !dry_run {
        ctx.manifest.save(&Manifest::path_for(&ctx.project))?;
    }
    let report = install(
        &mut ctx,
        &ui,
        InstallOpts {
            force_resolve: true,
            frozen: false,
            dry_run,
        },
    )?;
    if !dry_run {
        ui.success(&format!("updated ({} packages)", report.resolved));
    }
    Ok(0)
}
