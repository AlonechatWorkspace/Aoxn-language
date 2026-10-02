//! `aoxn audit` — check the lockfile against a vulnerability advisory DB.
//!
//! The advisory source is configured globally in `$AOXN_HOME/config.json`:
//! ```json
//! { "advisories": { "git": "https://github.com/you/aoxn-advisories" } }
//! ```
//! (or `"path": "/dir"` for an air-gapped copy). When nothing is configured,
//! audit falls back to the `advisories/` directory of the default registry —
//! which is how a *curated* registry carries advisories alongside the trust
//! index (`aoxn trust bootstrap` wires all of it up from one URL).
//!
//! Every `*.json` file in the source holds an array of advisories:
//! ```json
//! [{
//!   "id": "AOXN-2026-0001",
//!   "package": "http",
//!   "vulnerable": "<1.4.1",
//!   "patched": "1.4.1",
//!   "severity": "high",
//!   "url": "https://example.com/advisory/1"
//! }]
//! ```
//! Exit code 1 when any locked package matches an advisory (CI-friendly).
//! `--audit-level` narrows that to a severity floor, and `--fix` moves the
//! manifest constraint up to the patched version when the existing
//! constraint already admits it.

use std::path::PathBuf;

use serde::Deserialize;

use crate::context::Ctx;
use crate::errors::PkgError;
use crate::manifest::{DependencySpec, Manifest};

#[derive(Debug, Clone, Deserialize)]
pub struct Advisory {
    pub id: String,
    pub package: String,
    /// semver requirement matched against the locked version
    pub vulnerable: String,
    #[serde(default)]
    pub patched: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Normalize one advisory JSON object: accept the native simple format
/// (fields `id`/`package`/`vulnerable`) **or** an OSV entry
/// (`id`/`affected[].package.name` + SEMVER `ranges[].events`
/// introduced/fixed), so an OSV.dev export works with zero conversion.
fn parse_advisory(v: &serde_json::Value) -> Option<Advisory> {
    let id = v.get("id")?.as_str()?.to_string();
    if let (Some(pkg), Some(vuln)) = (
        v.get("package").and_then(|p| p.as_str()),
        v.get("vulnerable").and_then(|r| r.as_str()),
    ) {
        return Some(Advisory {
            id,
            package: pkg.to_string(),
            vulnerable: vuln.to_string(),
            patched: v.get("patched").and_then(|p| p.as_str()).map(String::from),
            severity: v.get("severity").and_then(|s| s.as_str()).map(String::from),
            url: v.get("url").and_then(|u| u.as_str()).map(String::from),
        });
    }
    // OSV shape
    let affected = v.get("affected")?.as_array()?;
    for a in affected {
        let Some(name) = a.pointer("/package/name").and_then(|n| n.as_str()) else {
            continue;
        };
        let Some(ranges) = a.get("ranges").and_then(|r| r.as_array()) else {
            continue;
        };
        for r in ranges {
            if r.get("type").and_then(|t| t.as_str()) != Some("SEMVER") {
                continue;
            }
            let mut introduced = String::from("*");
            let mut fixed: Option<String> = None;
            if let Some(events) = r.get("events").and_then(|e| e.as_array()) {
                for ev in events {
                    if let Some(i) = ev.get("introduced").and_then(|i| i.as_str()) {
                        if i != "0" {
                            introduced = format!(">={i}");
                        }
                    }
                    if let Some(f) = ev.get("fixed").and_then(|f| f.as_str()) {
                        fixed = Some(f.to_string());
                    }
                }
            }
            let vulnerable = match &fixed {
                Some(f) => format!("{introduced},<{f}"),
                None => introduced,
            };
            return Some(Advisory {
                id: id.clone(),
                package: name.to_string(),
                vulnerable,
                patched: fixed,
                severity: None,
                url: Some(format!("https://osv.dev/vulnerability/{id}")),
            });
        }
    }
    None
}

const NO_ADVISORIES: &str = "no advisory database available. Add one to \
     $AOXN_HOME/config.json:\n  \
     { \"advisories\": { \"git\": \"https://github.com/you/aoxn-advisories\" } }\n\
     or point one command at a curated registry, which carries its own:\n  \
     aoxn trust bootstrap https://github.com/AlonechatWorkspace/Aoxn-trusted-third-party-package";

/// Shallow-clone (and best-effort refresh) an advisory repository into the
/// cache, keyed by its URL.
fn clone_advisories(cache: &crate::cache::Cache, git_url: &str) -> Result<PathBuf, PkgError> {
    let dir = cache.registries().join(format!(
        "advisories-{}",
        &crate::cache::sha256_hex(git_url.as_bytes())[..12]
    ));
    if !dir.join(".git").exists() {
        std::fs::create_dir_all(cache.registries())?;
        let out = std::process::Command::new("git")
            .args(["clone", "--depth", "1", git_url])
            .arg(&dir)
            .output()
            .map_err(|e| PkgError::Registry(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(PkgError::Registry(format!(
                "cloning advisories failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
    } else {
        // best-effort refresh
        let _ = std::process::Command::new("git")
            .args(["fetch", "--depth", "1", "origin"])
            .current_dir(&dir)
            .output();
        let _ = std::process::Command::new("git")
            .args(["reset", "--hard", "FETCH_HEAD"])
            .current_dir(&dir)
            .output();
    }
    Ok(dir)
}

/// The default registry's own `advisories/`, when it ships one and the
/// backend has a local copy of the tree to read.
fn registry_advisory_dir(ctx: &mut Ctx) -> Result<Option<PathBuf>, PkgError> {
    let Ok(url) = ctx.registry_url(None) else {
        return Ok(None);
    };
    let Ok(reg) = ctx.registry_for(&url) else {
        return Ok(None);
    };
    match reg.advisories_dir() {
        Ok(d) => Ok(d),
        // a git registry that cannot be fetched offline is not fatal here:
        // report "no advisories" and let the caller say so
        Err(_) => Ok(None),
    }
}

/// Where the advisory database lives, in order of preference:
///
/// 1. an explicitly configured local `path`;
/// 2. an explicitly configured `git` repository;
/// 3. the default registry's own `advisories/` — which is what makes a
///    curated registry work from a single URL.
///
/// If a configured `git` source is unreachable (a mirror is down, or the
/// configured URL turns out to be a directory rather than a repository), we
/// fall back to (3) rather than failing: audit should report "no advisories"
/// or find the ones next door, not die on a stale config entry.
fn resolve_advisory_dir(ctx: &mut Ctx) -> Result<PathBuf, PkgError> {
    if let Some(src) = ctx.config.advisories.clone() {
        if let Some(path) = src.path {
            return Ok(PathBuf::from(path));
        }
        if let Some(git_url) = src.git {
            if let Ok(dir) = clone_advisories(&ctx.cache, &git_url) {
                return Ok(dir);
            }
            if let Some(dir) = registry_advisory_dir(ctx)? {
                return Ok(dir);
            }
        }
    }
    match registry_advisory_dir(ctx)? {
        Some(dir) => Ok(dir),
        None => Err(PkgError::Config(NO_ADVISORIES.into())),
    }
}

fn load_advisories(dir: &PathBuf) -> Result<Vec<Advisory>, PkgError> {
    let mut out = Vec::new();
    fn walk(dir: &PathBuf, out: &mut Vec<Advisory>) -> Result<(), PkgError> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.extension().map(|x| x == "json").unwrap_or(false) {
                let text = std::fs::read_to_string(&p)?;
                let value: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
                    PkgError::Config(format!("invalid advisory file {}: {err}", p.display()))
                })?;
                match value {
                    serde_json::Value::Array(list) => {
                        for v in &list {
                            if let Some(a) = parse_advisory(v) {
                                out.push(a);
                            }
                        }
                    }
                    // a single OSV object per file is also common
                    serde_json::Value::Object(_) => {
                        if let Some(a) = parse_advisory(&value) {
                            out.push(a);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    walk(dir, &mut out)?;
    Ok(out)
}

/// Severity ranking for `--audit-level`. Unknown/absent severity sorts
/// lowest so a `--audit-level medium` run does not silently fail on an
/// advisory that simply never recorded one.
fn severity_rank(s: Option<&str>) -> u8 {
    match s.unwrap_or("").to_ascii_lowercase().as_str() {
        "critical" => 4,
        "high" => 3,
        "medium" | "moderate" => 2,
        "low" => 1,
        _ => 0,
    }
}

pub fn level_from_str(s: &str) -> Result<u8, PkgError> {
    severity_rank(Some(s))
        .checked_sub(1)
        .filter(|_| severity_rank(Some(s)) > 0)
        .ok_or_else(|| {
            PkgError::Config(format!(
                "unknown --audit-level `{s}` (expected low, medium, high or critical)"
            ))
        })
        .map(|_| severity_rank(Some(s)))
}

struct Hit {
    name: String,
    version: String,
    advisory: Advisory,
}

pub fn run(level: Option<&str>, json: bool, fix: bool, offline: bool) -> Result<i32, PkgError> {
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(crate::lockfile::Lockfile::empty);
    let dir = resolve_advisory_dir(&mut ctx)?;
    let advisories = load_advisories(&dir)?;
    let floor = match level {
        Some(l) => level_from_str(l)?,
        None => 0,
    };

    let mut hits: Vec<Hit> = Vec::new();
    for pkg in &lock.packages {
        let Ok(ver) = semver::Version::parse(&pkg.version) else {
            continue;
        };
        for adv in &advisories {
            if adv.package != pkg.name {
                continue;
            }
            if severity_rank(adv.severity.as_deref()) < floor {
                continue;
            }
            if let Ok(req) = semver::VersionReq::parse(&adv.vulnerable) {
                if req.matches(&ver) {
                    hits.push(Hit {
                        name: pkg.name.clone(),
                        version: pkg.version.clone(),
                        advisory: adv.clone(),
                    });
                }
            }
        }
    }
    hits.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.advisory.id.cmp(&b.advisory.id))
    });
    hits.dedup_by(|a, b| a.name == b.name && a.advisory.id == b.advisory.id);

    let ui = crate::ui::current();
    let mut fixed = 0usize;
    let mut unfixable: Vec<String> = Vec::new();
    if fix && !hits.is_empty() {
        for hit in &hits {
            let Some(patched) = hit.advisory.patched.as_deref() else {
                unfixable.push(format!(
                    "{}: advisory {} names no patched version",
                    hit.name, hit.advisory.id
                ));
                continue;
            };
            match bump_constraint(&mut ctx, &hit.name, patched) {
                Ok(FixOutcome::Bumped) => fixed += 1,
                Ok(FixOutcome::AlreadySafe) => {}
                Ok(FixOutcome::OutOfRange) => unfixable.push(format!(
                    "{}: `{patched}` is outside the manifest's current constraint — \
                     widen it deliberately (`aoxn add {}@^{patched}`) if you accept the change",
                    hit.name, hit.name
                )),
                Ok(FixOutcome::NotDeclared) => {}
                Err(e) => unfixable.push(format!("{}: {e}", hit.name)),
            }
        }
    }

    if json {
        ui.out_json(&serde_json::json!({
            "advisories": advisories.len(),
            "packages_checked": lock.packages.len(),
            "vulnerabilities": hits
                .iter()
                .map(|h| serde_json::json!({
                    "package": h.name,
                    "version": h.version,
                    "id": h.advisory.id,
                    "severity": h.advisory.severity,
                    "patched": h.advisory.patched,
                    "url": h.advisory.url,
                }))
                .collect::<Vec<_>>(),
            "fixed": fixed,
            "unfixable": unfixable,
        }));
    } else if hits.is_empty() {
        ui.success(&format!(
            "audit clean: {} packages checked against {} advisories",
            lock.packages.len(),
            advisories.len()
        ));
    } else {
        ui.out(&format!(
            "{} vulnerable package{} found:",
            hits.len(),
            if hits.len() == 1 { "" } else { "s" }
        ));
        for h in &hits {
            ui.out(&format!(
                "  {}@{} — {} ({}){}{}",
                h.name,
                h.version,
                h.advisory.id,
                h.advisory.severity.as_deref().unwrap_or("unknown"),
                h.advisory
                    .patched
                    .as_ref()
                    .map(|p| format!(" patched in {p};"))
                    .unwrap_or_default(),
                h.advisory.url.as_ref().map(|u| format!(" {u}")).unwrap_or_default(),
            ));
        }
        if fixed > 0 {
            ui.success(&format!(
                "fixed {fixed} package(s) in the manifest; run `aoxn install` to apply"
            ));
        }
        for u in &unfixable {
            ui.warn(u);
        }
        if fixed == 0 && unfixable.is_empty() {
            ui.out("update the listed packages (`aoxn update --latest <name>`) or pin safe versions.");
        }
    }
    // non-zero for CI
    Ok(if hits.is_empty() { 0 } else { 1 })
}

enum FixOutcome {
    /// the manifest constraint was raised
    Bumped,
    /// the constraint already admitted the patched version
    AlreadySafe,
    /// the patched version is outside a constraint someone wrote on purpose
    OutOfRange,
    /// the package is only transitive — no manifest line to edit
    NotDeclared,
}

/// Raise the manifest constraint for `name` to `^patched`.
///
/// `--fix` deliberately refuses to *widen* a range: it only tightens a
/// constraint that already admits the patched version, and says so when the
/// fix would require a decision the user should make themselves. Transitive
/// packages have no manifest line and are reported as such.
fn bump_constraint(ctx: &mut Ctx, name: &str, patched: &str) -> Result<FixOutcome, PkgError> {
    let Ok(patched_v) = semver::Version::parse(patched) else {
        return Err(PkgError::Manifest(format!(
            "advisory names a non-semver patched version `{patched}`"
        )));
    };
    let new_req = format!("^{patched}");
    let packages = ctx.all_package_manifests()?;
    let mut outcome = FixOutcome::NotDeclared;

    for (dir, _) in &packages {
        let mut manifest = match Manifest::load(&Manifest::path_for(dir)) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mut touched = false;
        for table in [&mut manifest.dependencies, &mut manifest.dev_dependencies] {
            let Some(DependencySpec::Registry { req, registry }) = table.get(name).cloned() else {
                continue;
            };
            let Ok(existing) = semver::VersionReq::parse(&req) else {
                continue;
            };
            if !existing.matches(&patched_v) {
                outcome = FixOutcome::OutOfRange;
                continue;
            }
            if req == new_req {
                outcome = FixOutcome::AlreadySafe;
                continue;
            }
            table.insert(
                name.to_string(),
                DependencySpec::Registry { req: new_req.clone(), registry },
            );
            touched = true;
            outcome = FixOutcome::Bumped;
        }
        if touched {
            manifest.save(&Manifest::path_for(dir))?;
        }
    }
    Ok(outcome)
}
