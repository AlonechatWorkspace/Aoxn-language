//! Registry abstraction: where packages and their metadata live.
//!
//! Two backends ship today:
//! - [`git::GitRegistry`] — a git repository is the registry (publish =
//!   commit + tag + push; install = shallow fetch). Zero infrastructure.
//! - [`dir::DirRegistry`] — a plain directory tree (air-gapped CI, tests).
//!
//! The [`Registry`] trait is deliberately close to what an HTTP registry
//! (npm-API subset) needs, so a future server backend is one more impl.

pub mod dir;
pub mod git;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

/// Metadata for one version of one package, as stored in the registry's
/// `packages/<name>/index.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexVersion {
    /// sha256 hex of the version's tarball
    pub checksum: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yanked: bool,
    /// deprecation message (`"use http 2"`); empty/null = not deprecated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    /// registry dependency requirements, `name -> semver req`
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    /// minimum compiler version this package requires (semver, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoxn: Option<String>,
    /// provenance/attestation records (reserved for future signing; the
    /// format is an open list of JSON objects)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attestations: Vec<serde_json::Value>,
}

/// The full version map for one package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageIndex {
    pub name: String,
    /// `version -> metadata`, sorted by semver on read (keys of the JSON
    /// object are strings; iterate in resolved semver order, not lex order)
    pub versions: BTreeMap<String, IndexVersion>,
    /// reserved for future external blob storage, e.g.
    /// `{"kind": "external", "base": "https://blobs.example.com/<name>"}`.
    /// The current git/dir backends keep tarballs in-tree and ignore this;
    /// the field exists so the format does not need a breaking change when
    /// registry growth demands offloading tarballs from the git tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<serde_json::Value>,
}

impl PackageIndex {
    /// Versions sorted in semver order (ascending).
    pub fn sorted_versions(&self) -> Vec<(String, &IndexVersion)> {
        let mut vs: Vec<(semver::Version, String, &IndexVersion)> = self
            .versions
            .iter()
            .filter_map(|(k, v)| k.parse::<semver::Version>().ok().map(|sv| (sv, k.clone(), v)))
            .collect();
        vs.sort_by(|a, b| a.0.cmp(&b.0));
        vs.into_iter().map(|(_, k, v)| (k, v)).collect()
    }

    /// Highest version (semver order), optionally ignoring yanked ones.
    pub fn latest(&self, include_yanked: bool) -> Option<(String, &IndexVersion)> {
        self.sorted_versions()
            .into_iter()
            .filter(|(_, v)| include_yanked || !v.yanked)
            .next_back()
            .map(|(k, v)| (k, v))
    }

    pub fn get(&self, version: &str) -> Option<&IndexVersion> {
        self.versions.get(version)
    }
}

pub struct PublishRequest {
    pub name: String,
    pub version: String,
    /// locally built tarball
    pub tarball: PathBuf,
    /// sha256 hex of `tarball`
    pub checksum: String,
    pub dependencies: BTreeMap<String, String>,
    pub aoxn: Option<String>,
}

pub enum PublishOutcome {
    Published,
    /// same name+version+checksum already present (idempotent retry)
    AlreadyPublished,
}

pub trait Registry {
    /// Registry identity (URL or path); part of the trait API — a future
    /// HTTP backend reports its endpoint here.
    #[allow(dead_code)]
    fn url(&self) -> &str;

    /// Full version map of one package. Errors with [`PkgError::PackageNotFound`]
    /// when the package is unknown.
    fn index(&mut self, name: &str) -> Result<PackageIndex, PkgError>;

    /// All package names in the registry (used by the typosquat guard).
    fn all_names(&mut self) -> Result<Vec<String>, PkgError>;

    /// Ensure the version's tarball is in the global cache; return its path.
    /// Verifies the sha256 checksum. Honors offline mode via `Cache`.
    fn fetch_tarball(
        &mut self,
        cache: &crate::cache::Cache,
        name: &str,
        version: &str,
        checksum: &str,
        offline: bool,
    ) -> Result<PathBuf, PkgError>;

    fn publish(&mut self, req: &PublishRequest, dry_run: bool) -> Result<PublishOutcome, PkgError>;

    fn set_yank(&mut self, name: &str, version: &str, yanked: bool) -> Result<(), PkgError>;
}

/// Read+parse a JSON file, with a NotFound → default fallback.
pub(crate) fn read_json_or_default<T: Default + serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> Result<T, PkgError> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| PkgError::Registry(format!("invalid {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(PkgError::Io(e)),
    }
}

/// Write JSON deterministically (sorted keys, pretty, trailing newline).
pub(crate) fn write_json<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<(), PkgError> {
    let text = serde_json::to_string_pretty(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text + "\n")?;
    Ok(())
}
