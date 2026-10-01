//! `aoxn audit` — check the lockfile against a vulnerability advisory DB.
//!
//! The advisory source is configured globally in `$AOXN_HOME/config.json`:
//! ```json
//! { "advisories": { "git": "https://github.com/you/aoxn-advisories" } }
//! ```
//! (or `"path": "/dir"` for an air-gapped copy). Every `*.json` file in the
//! source holds an array of advisories:
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

use std::path::PathBuf;

use serde::Deserialize;

use crate::context::Ctx;
use crate::errors::PkgError;

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

fn advisory_dir(ctx: &Ctx) -> Result<PathBuf, PkgError> {
    let Some(src) = &ctx.config.advisories else {
        return Err(PkgError::Config(
            "no advisory database configured. Add to $AOXN_HOME/config.json:\n  \
             { \"advisories\": { \"git\": \"https://github.com/you/aoxn-advisories\" } }"
                .into(),
        ));
    };
    if let Some(path) = &src.path {
        return Ok(PathBuf::from(path));
    }
    if let Some(git_url) = &src.git {
        let dir = ctx.cache.registries().join(format!(
            "advisories-{}",
            crate::cache::sha256_hex(git_url.as_bytes())[..12].to_string()
        ));
        if !dir.join(".git").exists() {
            std::fs::create_dir_all(ctx.cache.registries())?;
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
        return Ok(dir);
    }
    Err(PkgError::Config("advisories source needs `git` or `path`".into()))
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

pub fn run() -> Result<i32, PkgError> {
    let ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(crate::lockfile::Lockfile::empty);
    let dir = advisory_dir(&ctx)?;
    let advisories = load_advisories(&dir)?;

    let mut hits: Vec<(&crate::lockfile::LockedPackage, &Advisory)> = Vec::new();
    for pkg in &lock.packages {
        let Ok(ver) = semver::Version::parse(&pkg.version) else {
            continue;
        };
        for adv in &advisories {
            if adv.package != pkg.name {
                continue;
            }
            if let Ok(req) = semver::VersionReq::parse(&adv.vulnerable) {
                if req.matches(&ver) {
                    hits.push((pkg, adv));
                }
            }
        }
    }

    let ui = crate::ui::current();
    if hits.is_empty() {
        ui.success(&format!(
            "audit clean: {} packages checked against {} advisories",
            lock.packages.len(),
            advisories.len()
        ));
        return Ok(0);
    }
    ui.out(&format!(
        "{} vulnerable package{} found:",
        hits.len(),
        if hits.len() == 1 { "" } else { "s" }
    ));
    for (pkg, adv) in &hits {
        ui.out(&format!(
            "  {}@{} — {} ({}){}{}",
            pkg.name,
            pkg.version,
            adv.id,
            adv.severity.as_deref().unwrap_or("unknown"),
            adv.patched
                .as_ref()
                .map(|p| format!(" patched in {p};"))
                .unwrap_or_default(),
            adv.url
                .as_ref()
                .map(|u| format!(" {u}"))
                .unwrap_or_default(),
        ));
    }
    ui.out("update the listed packages (`aoxn update --latest <name>`) or pin safe versions.");
    // details already printed; exit non-zero for CI
    Ok(1)
}
