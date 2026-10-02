//! Registry abstraction: where packages and their metadata live.
//!
//! Three backends ship today:
//! - [`git::GitRegistry`] — a git repository is the registry (publish =
//!   commit + tag + push; install = shallow fetch). Zero infrastructure.
//! - [`dir::DirRegistry`] — a plain directory tree (air-gapped CI, tests).
//! - [`http::HttpRegistry`] — the same tree served over plain HTTP/1.1
//!   (read-only: install/resolve only; a static file server or a mirror
//!   does the hosting).
//!
//! The [`Registry`] trait is deliberately close to what an HTTP registry
//! (npm-API subset) needs, so a fuller server backend is one more impl.

pub mod dir;
pub mod git;
pub mod http;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

/// Metadata for one version of one package, as stored in the registry's
/// `packages/<name>/index.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexVersion {
    /// **content** digest — the manifest hash over the unpacked file tree
    /// (`tarball::manifest_hash`). This is what `aoxn.lock` pins and what
    /// install re-verifies over the extracted tree; it is deliberately not a
    /// hash of the tarball, so it survives re-packing.
    pub checksum: String,
    /// **transport** digest — sha256 of the `<version>.tar.gz` bytes exactly
    /// as served. Added in v0.31.0; `None` on older registries. When present,
    /// the HTTP backend checks the download against it, so truncation and
    /// tampering fail before the tarball ever enters the cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tarball_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yanked: bool,
    /// deprecation message (`"use http 2"`); empty/null = not deprecated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    /// registry dependency requirements, `name -> semver req`
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    /// minimum compiler version this package requires (semver, optional).
    /// Enforced at install since v0.31.0 (`PkgError::Engines`).
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
    /// manifest hash of the package tree (the content anchor)
    pub checksum: String,
    /// sha256 hex of the `tarball` bytes (the transport digest)
    pub tarball_sha256: Option<String>,
    pub dependencies: BTreeMap<String, String>,
    pub aoxn: Option<String>,
}

pub enum PublishOutcome {
    Published,
    /// same name+version+checksum already present (idempotent retry)
    AlreadyPublished,
}

/// `Send` so a forked handle can be moved into a download worker thread
/// (install runs a pool of them — see `install::prefetch`). Every backend is
/// plain configuration, so this costs nothing.
pub trait Registry: Send {
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
    ///
    /// `checksum` is the **manifest hash** and names the cache entry;
    /// `tarball_sha256` is the optional **transport digest** published
    /// alongside it — a backend that can detect a bad transfer on the wire
    /// (today: HTTP) verifies against it, the rest verify the extracted tree
    /// against `checksum` later in install. Honors offline mode.
    fn fetch_tarball(
        &mut self,
        cache: &crate::cache::Cache,
        name: &str,
        version: &str,
        checksum: &str,
        tarball_sha256: Option<&str>,
        offline: bool,
    ) -> Result<PathBuf, PkgError>;

    /// A second, independent handle to the same registry.
    ///
    /// Install downloads tarballs from a thread pool (v0.31.0), and each
    /// worker needs its own handle: the backends carry mutable memo state
    /// (a clone step, an HTTP connection) and are not shared across threads.
    /// All three shipping backends are plain config, so this is a cheap copy.
    fn fork(&self) -> Result<Box<dyn Registry>, PkgError>;

    /// The registry's trust index (`trust.json` at its root), when it has one.
    /// A registry without a trust index makes no trust claims — `None` means
    /// "no opinion", which is not the same as "untrusted".
    fn trust(&mut self) -> Result<Option<crate::trust::TrustIndex>, PkgError> {
        let _ = self;
        Ok(None)
    }

    /// The registry's advisory database, when it ships one and this backend
    /// has a local copy of the tree to read it from. `aoxn audit` falls back
    /// to this when no advisory source is configured, which is what makes a
    /// curated registry work with one URL.
    fn advisories_dir(&mut self) -> Result<Option<PathBuf>, PkgError> {
        let _ = self;
        Ok(None)
    }

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
