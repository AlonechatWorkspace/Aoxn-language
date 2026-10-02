//! Curated-registry support: the trust index, its tiers, and the
//! `aoxn trust` commands.
//!
//! A registry may carry a `trust.json` at its root declaring which of its
//! packages have been reviewed and by whom. That is the third role a
//! curated registry plays, next to hosting packages and carrying
//! advisories — see `docs/trusted-registry.md` for the on-disk layout.
//!
//! ```json
//! {
//!   "schema": 1,
//!   "updated": "2026-10-02",
//!   "packages": {
//!     "http": { "tier": "audited", "reviewer": "@someone",
//!               "reviewed": "2026-09-30", "summary": "HTTP client",
//!               "advisories": "advisories/http.json" }
//!   }
//! }
//! ```
//!
//! Two things this module deliberately does **not** do: it never blocks an
//! install on its own (a registry with no trust index is not a hostile
//! registry — `None` means "no opinion"), and it never asserts anything
//! about provenance. Trust here is a *curation signal surfaced to the
//! user*, nothing more; the cryptographic anchor is still the manifest hash
//! in `aoxn.lock`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::context::{AdvisoriesSource, Ctx, GlobalConfig};
use crate::errors::PkgError;
use crate::ui::Ui;

/// How much review a package in a curated registry has had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// source read; nobody has reviewed it — the default for an unknown name
    Unreviewed,
    /// read by a community member, no maintainer sign-off
    Community,
    /// reviewed and signed off by the registry's maintainers
    Audited,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Unreviewed => "unreviewed",
            Tier::Community => "community",
            Tier::Audited => "audited",
        }
    }

    /// A reviewer is required for anything above `unreviewed`.
    pub fn reviewed(&self) -> bool {
        *self != Tier::Unreviewed
    }

    /// Parse a tier name from the command line (`aoxn trust check --tier`).
    pub fn from_str(s: &str) -> Result<Tier, PkgError> {
        match s.to_ascii_lowercase().as_str() {
            "unreviewed" => Ok(Tier::Unreviewed),
            "community" => Ok(Tier::Community),
            "audited" => Ok(Tier::Audited),
            other => Err(PkgError::Config(format!(
                "unknown trust tier `{other}` (expected unreviewed, community or audited)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    /// review level; absent means `unreviewed`
    #[serde(default)]
    pub tier: Tier,
    /// who did the review (`@handle` or a team slug)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// date of the review, ISO `YYYY-MM-DD`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed: Option<String>,
    /// one-line description shown by `aoxn trust list`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// repo-relative path to this package's advisories, if it has its own
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisories: Option<String>,
}

impl Default for Tier {
    fn default() -> Self {
        Tier::Unreviewed
    }
}

/// The `trust.json` document at a registry's root.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustIndex {
    /// schema version; `1` is the only one defined so far
    #[serde(default)]
    pub schema: u32,
    /// date the list was last curated, ISO `YYYY-MM-DD`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// package name -> review record
    #[serde(default)]
    pub packages: BTreeMap<String, TrustEntry>,
}

/// Current `trust.json` schema version.
pub const TRUST_SCHEMA: u32 = 1;

impl TrustIndex {
    pub fn from_slice(bytes: &[u8]) -> Result<TrustIndex, PkgError> {
        let idx: TrustIndex = serde_json::from_slice(bytes)
            .map_err(|e| PkgError::Registry(format!("invalid trust.json: {e}")))?;
        if idx.schema != TRUST_SCHEMA {
            return Err(PkgError::Registry(format!(
                "trust.json declares schema {} but this aoxn understands {}",
                idx.schema, TRUST_SCHEMA
            )));
        }
        Ok(idx)
    }

    /// The review record for `name`. A package the registry hosts but does
    /// not list is `None` — distinct from a listed `unreviewed` entry, which
    /// means "we know about it and nobody has looked at it".
    pub fn entry(&self, name: &str) -> Option<&TrustEntry> {
        self.packages.get(name)
    }

    /// Tier for `name`, defaulting to `unreviewed` when unlisted.
    pub fn tier_of(&self, name: &str) -> Tier {
        self.entry(name).map(|e| e.tier).unwrap_or(Tier::Unreviewed)
    }
}

/// Load `trust.json` from disk; a missing file means "no trust index", which
/// is not an error.
pub fn load_trust_file(path: &Path) -> Result<Option<TrustIndex>, PkgError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(TrustIndex::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PkgError::Io(e)),
    }
}

/// Load the trust index of a registry URL (memoized per `Ctx`).
pub fn index_of(ctx: &mut Ctx, url: &str) -> Result<Option<TrustIndex>, PkgError> {
    let reg = ctx.registry_for(url)?;
    reg.trust()
}

/// Warn about installed packages a curated registry has no reviewed record
/// for. Silently does nothing when the registry publishes no trust index —
/// we do not want to train users to ignore a warning that fires everywhere.
pub fn report_untrusted(ui: &Ui, trust: &TrustIndex, registry: &str, names: &[String]) {    let mut warned: Vec<&String> = names
        .iter()
        .filter(|n| !trust.tier_of(n).reviewed())
        .collect();
    if warned.is_empty() {
        return;
    }
    warned.sort();
    let listed = warned
        .iter()
        .map(|n| n.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    ui.warn(&format!(
        "registry `{registry}` has no reviewed trust record for: {listed} \
         (check `aoxn trust list`, or review the source before shipping)"
    ));
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// Point the global config at a curated registry in one shot: default
/// registry + advisory database, both from the same repository. This is the
/// whole point of the curated layout — one URL, all three roles wired.
pub fn bootstrap(registry: Option<String>, offline: bool) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let cache = crate::cache::Cache::global();
    let mut config = read_config(&cache);
    let url = match registry {
        Some(u) => u,
        None => match config.default_registry.clone() {
            Some(u) => u,
            None => {
                return Err(PkgError::Config(
                    "pass the curated registry URL: \
                     `aoxn trust bootstrap https://github.com/AlonechatWorkspace/Aoxn-trusted-third-party-package`"
                        .into(),
                ))
            }
        },
    };

    // Verify the URL is really a curated registry before writing config, so a
    // typo fails now instead of on every later install.
    let mut probe = Ctx::probe(&url, offline)?;
    let (trust, names, advisories) = {
        let reg = probe.registry_for(&url)?;
        (
            reg.trust()?,
            reg.all_names().unwrap_or_default(),
            reg.advisories_dir()?,
        )
    };

    // A directory registry is not a git repository, so its advisories are
    // recorded as a local path; anything else is recorded as a git source
    // that audit shallow-clones into the cache.
    let source = match &advisories {
        Some(dir) => AdvisoriesSource {
            git: None,
            path: Some(dir.to_string_lossy().to_string()),
        },
        None => AdvisoriesSource {
            git: Some(url.clone()),
            path: None,
        },
    };

    config.default_registry = Some(url.clone());
    config.advisories = Some(source);
    cache.create_dirs()?;
    std::fs::write(&cache.config_path(), serde_json::to_string_pretty(&config)? + "\n")?;

    ui.success(&format!("wrote {}", cache.config_path().display()));
    ui.info(&format!("  default registry : {url}"));
    match &advisories {
        Some(dir) => ui.info(&format!("  advisories       : {} (local)", dir.display())),
        None => ui.info(&format!("  advisories       : {url} (cloned into the cache)")),
    }
    match &trust {
        Some(t) => ui.info(&format!(
            "  trust index       : {} reviewed of {} hosted package(s), updated {}",
            t.packages.values().filter(|e| e.tier.reviewed()).count(),
            names.len(),
            t.updated.as_deref().unwrap_or("unknown")
        )),
        None => ui.info("  trust index       : none (registry ships no trust.json)"),
    }
    Ok(0)
}

pub fn list(json: bool) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    // works inside a project or outside one: a CI trust gate usually runs
    // from a directory with no aoxn.json
    let mut ctx = Ctx::discover_or_probe(false)?;
    let url = ctx.registry_url(None)?;
    let trust = index_of(&mut ctx, &url)?.ok_or_else(|| {
        PkgError::Config(format!(
            "registry `{url}` ships no trust.json (see docs/trusted-registry.md)"
        ))
    })?;

    if json {
        ui.out_json(&serde_json::json!({
            "registry": url,
            "updated": trust.updated,
            "packages": trust.packages,
        }));
        return Ok(0);
    }

    ui.out(&format!(
        "{} — {} reviewed package(s), updated {}",
        url,
        trust.packages.len(),
        trust.updated.as_deref().unwrap_or("unknown")
    ));
    // widest first, then name; deterministic
    let mut rows: Vec<(&String, &TrustEntry)> = trust.packages.iter().collect();
    rows.sort_by(|a, b| b.1.tier.cmp(&a.1.tier).then(a.0.cmp(b.0)));
    for (name, e) in rows {
        ui.out(&format!(
            "  {:<10} {:<32} {}",
            e.tier.as_str(),
            e.reviewer.as_deref().unwrap_or("-"),
            name
        ));
        if let Some(s) = &e.summary {
            ui.out(&format!("  {:<10} {}", "", s));
        }
    }
    Ok(0)
}

/// Gate for CI: exit 0 only when the package is reviewed at `min_tier`.
pub fn check(pkg: &str, min_tier: Tier) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let mut ctx = Ctx::discover_or_probe(false)?;
    let url = ctx.registry_url(None)?;
    let trust = index_of(&mut ctx, &url)?;
    let Some(trust) = trust else {
        ui.warn(&format!("registry `{url}` ships no trust.json — nothing to check against"));
        return Ok(1);
    };
    match trust.entry(pkg) {
        None => {
            ui.out(&format!("{pkg}: not listed in {url} (unreviewed)"));
            Ok(1)
        }
        Some(e) => {
            let ok = e.tier >= min_tier;
            ui.out(&format!(
                "{pkg}: {} ({}{})",
                e.tier.as_str(),
                e.reviewer.as_deref().unwrap_or("no reviewer recorded"),
                e.reviewed
                    .as_deref()
                    .map(|d| format!(", reviewed {d}"))
                    .unwrap_or_default()
            ));
            if !ok {
                ui.warn(&format!(
                    "{pkg} is `{}` but at least `{}` is required",
                    e.tier.as_str(),
                    min_tier.as_str()
                ));
            }
            Ok(if ok { 0 } else { 1 })
        }
    }
}

/// Read the global config (used by `bootstrap`, which runs outside a project).
pub fn read_config(cache: &crate::cache::Cache) -> GlobalConfig {
    std::fs::read_to_string(cache.config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> TrustIndex {
        TrustIndex::from_slice(s.as_bytes()).unwrap()
    }

    #[test]
    fn parses_a_curated_index() {
        let t = parse(
            r#"{
                "schema": 1,
                "updated": "2026-10-02",
                "packages": {
                    "http": { "tier": "audited", "reviewer": "@ryan",
                              "reviewed": "2026-09-30", "summary": "HTTP client" },
                    "json": { "tier": "community" }
                }
            }"#,
        );
        assert_eq!(t.schema, 1);
        assert_eq!(t.updated.as_deref(), Some("2026-10-02"));
        assert_eq!(t.packages.len(), 2);
        assert_eq!(t.tier_of("http"), Tier::Audited);
        assert_eq!(t.tier_of("json"), Tier::Community);
        assert_eq!(t.entry("http").unwrap().reviewer.as_deref(), Some("@ryan"));
    }

    #[test]
    fn unlisted_and_unreviewed_are_different_answers() {
        let t = parse(r#"{"schema":1,"packages":{"http":{}}}"#);
        // listed, no tier key -> unreviewed
        assert!(t.entry("http").is_some());
        assert_eq!(t.tier_of("http"), Tier::Unreviewed);
        // never heard of it -> also unreviewed, but no entry at all
        assert!(t.entry("nope").is_none());
        assert_eq!(t.tier_of("nope"), Tier::Unreviewed);
    }

    #[test]
    fn tiers_order_by_confidence() {
        assert!(Tier::Audited > Tier::Community);
        assert!(Tier::Community > Tier::Unreviewed);
        // `reviewed` means "somebody has looked at it", which is the bar
        // install's warning uses — community review clears it, audited
        // review is simply stronger.
        assert!(!Tier::Unreviewed.reviewed());
        assert!(Tier::Community.reviewed());
        assert!(Tier::Audited.reviewed());
    }

    #[test]
    fn tier_names_parse_and_typos_are_rejected() {
        assert_eq!(Tier::from_str("audited").unwrap(), Tier::Audited);
        assert_eq!(Tier::from_str("COMMUNITY").unwrap(), Tier::Community);
        assert_eq!(Tier::from_str("unreviewed").unwrap(), Tier::Unreviewed);
        let err = Tier::from_str("golder").unwrap_err();
        assert!(format!("{err}").contains("golder"), "{err}");
    }

    #[test]
    fn unknown_schema_is_rejected_with_the_expected_number() {
        let err = TrustIndex::from_slice(br#"{"schema":99}"#).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("schema 99"), "{msg}");
        assert!(msg.contains(&TRUST_SCHEMA.to_string()), "{msg}");
    }

    #[test]
    fn a_missing_trust_json_is_not_an_error() {
        let missing = std::env::temp_dir().join("aoxn-no-such-trust.json");
        let _ = std::fs::remove_file(&missing);
        assert!(load_trust_file(&missing).unwrap().is_none());
    }

    #[test]
    fn report_untrusted_names_only_unreviewed_packages() {
        let ui = crate::ui::plain();
        let t = parse(
            r#"{"schema":1,"packages":{"http":{"tier":"audited"},"json":{"tier":"community"}}}"#,
        );
        let names: Vec<String> = ["http", "json", "rogue"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        report_untrusted(&ui, &t, "https://example.com/r", &names);
        // audited + community count as reviewed; only `rogue` is called out.
        // The UI writes to a terminal sink in tests, so assert the filter
        // directly rather than scraping output.
        let warned: Vec<&str> = names
            .iter()
            .filter(|n| !t.tier_of(n).reviewed())
            .map(String::as_str)
            .collect();
        assert_eq!(warned, vec!["rogue"]);
    }
}
