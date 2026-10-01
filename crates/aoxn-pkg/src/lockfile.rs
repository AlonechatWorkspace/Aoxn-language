//! `aoxn.lock` — the lockfile.
//!
//! Design constraints:
//! - **Determinism**: packages are stored sorted by name (then version), keys
//!   in fixed field order, JSON pretty-printed with 2-space indent and a
//!   trailing newline. The same resolution always produces byte-identical
//!   files, so diffs are minimal and reviews are meaningful.
//! - **Integrity**: every registry entry carries the package's manifest hash
//!   (see `tarball::manifest_hash`); install verifies it over the extracted
//!   tree before anything compiles. Path dependencies are recorded with an
//!   empty checksum and a `path+<dir>` registry — their source of truth is
//!   the local tree.
//! - A yanked version already locked here still installs (supply-chain safe:
//!   your build cannot be broken by a remote yank), but fresh resolution
//!   refuses to pick it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

pub const LOCKFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub lockfile_version: u32,
    /// manifest requirements fingerprint (`dep -> "req@registry@dir"`):
    /// when it matches the current manifests, install uses the locked
    /// versions as-is without re-resolving (and without any network)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub requirements: BTreeMap<String, String>,
    /// sorted by `name`; one entry per resolved package (single version per
    /// package — the import model merges namespaces, so two versions of the
    /// same package cannot coexist anyway)
    pub packages: Vec<LockedPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    /// where the package came from: a registry URL, or `path+<dir>` for
    /// workspace/path dependencies (which have no integrity checksum —
    /// their source of truth is the local tree)
    pub registry: String,
    /// manifest hash of the package tree; empty for path dependencies
    pub checksum: String,
    /// resolved dependency edges, `name@version`, sorted
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

impl Lockfile {
    pub fn empty() -> Self {
        Lockfile {
            lockfile_version: LOCKFILE_VERSION,
            requirements: BTreeMap::new(),
            packages: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Option<Lockfile>, PkgError> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let lf: Lockfile = serde_json::from_str(&text).map_err(|e| {
                    PkgError::Lockfile(format!("invalid {}: {e}", path.display()))
                })?;
                Ok(Some(lf))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(PkgError::Io(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), PkgError> {
        let mut lf = self.clone();
        lf.normalize();
        let text = serde_json::to_string_pretty(&lf)?;
        std::fs::write(path, text + "\n")?;
        Ok(())
    }

    fn normalize(&mut self) {
        self.packages.sort_by(|a, b| {
            (a.name.clone(), a.version.clone()).cmp(&(b.name.clone(), b.version.clone()))
        });
        for p in &mut self.packages {
            p.dependencies.sort();
            p.dependencies.dedup();
        }
    }

    pub fn get(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    pub fn insert(&mut self, pkg: LockedPackage) {
        self.packages.retain(|p| p.name != pkg.name);
        self.packages.push(pkg);
        self.normalize();
    }

    /// Map form used by the resolver's soft-preference input.
    pub fn locked_versions(&self) -> BTreeMap<String, String> {
        self.packages
            .iter()
            .map(|p| (p.name.clone(), p.version.clone()))
            .collect()
    }

    /// Explicit manifest↔lock sync check: every manifest dependency
    /// constraint is satisfied by the locked version. `--frozen` installs
    /// refuse to run when this fails.
    pub fn satisfies_manifests(
        &self,
        requirements: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for (dep, spec) in requirements {
            // spec = "req@registry@dir"
            let req = spec.split('@').next().unwrap_or("*");
            let Ok(req) = semver::VersionReq::parse(req) else {
                continue;
            };
            match self.get(dep) {
                None => return Err(format!("dependency `{dep}` is not in the lockfile")),
                Some(lp) => {
                    let Ok(v) = semver::Version::parse(&lp.version) else {
                        return Err(format!("`{dep}` has non-semver version `{}`", lp.version));
                    };
                    if !req.matches(&v) {
                        return Err(format!(
                            "manifest requires `{dep} {req}` but the lockfile has {}",
                            lp.version
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Lockfile {
        let mut lf = Lockfile::empty();
        lf.insert(LockedPackage {
            name: "zlib".into(),
            version: "1.0.0".into(),
            registry: "https://example.com/r".into(),
            checksum: "aa".into(),
            dependencies: vec![],
        });
        lf.insert(LockedPackage {
            name: "http".into(),
            version: "2.1.0".into(),
            registry: "https://example.com/r".into(),
            checksum: "bb".into(),
            dependencies: vec!["zlib@1.0.0".into()],
        });
        lf
    }

    #[test]
    fn deterministic_output() {
        let a = serde_json::to_string_pretty(&sample()).unwrap();
        let b = serde_json::to_string_pretty(&sample()).unwrap();
        assert_eq!(a, b);
        assert!(a.find("\"http\"").unwrap() < a.find("\"zlib\"").unwrap());
    }

    #[test]
    fn roundtrip() {
        let text = serde_json::to_string_pretty(&sample()).unwrap() + "\n";
        let lf: Lockfile = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string_pretty(&lf).unwrap(), text.trim_end());
    }

    #[test]
    fn remove_and_get() {
        let mut lf = sample();
        assert_eq!(lf.packages.len(), 2);
        lf.packages.retain(|p| p.name != "http");
        assert!(lf.get("http").is_none());
        assert_eq!(lf.get("zlib").unwrap().version, "1.0.0");
    }

    #[test]
    fn satisfies_manifests_checks_constraints() {
        let lf = sample();
        let mut reqs = BTreeMap::new();
        reqs.insert("http".to_string(), "^2.0@https://example.com/r@/x".to_string());
        assert!(lf.satisfies_manifests(&reqs).is_ok());

        reqs.insert("http".to_string(), "^3.0@https://example.com/r@/x".to_string());
        let err = lf.satisfies_manifests(&reqs).unwrap_err();
        assert!(err.contains("^3.0"), "{err}");

        reqs.insert("missing".to_string(), "^1@r@/x".to_string());
        assert!(lf.satisfies_manifests(&reqs).is_err());
    }
}
