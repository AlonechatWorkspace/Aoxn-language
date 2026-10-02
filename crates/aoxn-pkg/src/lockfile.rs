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
//!
//! Dev and prod dependencies share one lockfile (v0.31.0). The `dev` flag on
//! each package is what `aoxn install --prod` prunes — so a CI job that only
//! ships runtime code still reproduces from a lockfile that also pins the
//! test tooling.
//!
//! `lockfile_version` stays **1** across the v0.31.0 additions: every new
//! field is optional with a serde default, so a v1 lockfile written by an
//! older aoxn still loads (its packages simply read as not-dev-only).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

pub const LOCKFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub lockfile_version: u32,
    /// manifest requirements fingerprint (`"<owner>/<dep>" -> "<req>@<registry>"`):
    /// when it matches the current manifests, install uses the locked
    /// versions as-is without re-resolving (and without any network).
    ///
    /// The owner-qualified key is what makes a workspace correct — two
    /// members may pin the same dependency at different ranges, and a bare
    /// dependency-name key let the second overwrite the first. The value no
    /// longer embeds an absolute directory, so moving or renaming the
    /// project stops forcing a re-resolve. Lockfiles written before v0.31.0
    /// use the bare dependency name as key; they still load and simply
    /// mismatch the fingerprint once, costing a single re-resolve.
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
    /// sha256 of the tarball bytes as served, when the registry publishes one
    /// (v0.31.0). Lets a transport-level backend verify a download before it
    /// reaches the cache; `checksum` remains the real content anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tarball_sha256: Option<String>,
    /// true when the package is reachable ONLY through `devDependencies`
    /// (v0.31.0). Absent in older lockfiles, where nothing was dev-only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dev: bool,
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
    ///
    /// Keys are owner-qualified (`"my-app/http"`); a legacy bare name works
    /// too, so lockfiles from before v0.31.0 still validate.
    pub fn satisfies_manifests(
        &self,
        requirements: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for (key, spec) in requirements {
            let dep = dep_of(key);
            // spec = "req@registry"
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

    /// Flag every package that is reachable ONLY from dev roots. Called
    /// once, after the lockfile's edges are complete — computing it per
    /// entry would misclassify a package that only looks dev-only until a
    /// prod dependent is inserted after it.
    pub fn mark_dev_only(&mut self, prod_roots: &[String]) {
        let prod = self.closure(prod_roots);
        for p in &mut self.packages {
            p.dev = !prod.contains(&p.name);
        }
    }

    /// The set of packages reachable from `roots` by following dependency
    /// edges (roots included). This is how the prod half of the graph is
    /// carved out of a single lockfile that also pins dev tooling: whatever
    /// this returns is shipped, whatever it misses is `aoxn install --prod`
    /// materialization's business to leave out.
    pub fn closure(&self, roots: &[String]) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = roots.to_vec();
        while let Some(name) = queue.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(lp) = self.get(&name) {
                for edge in &lp.dependencies {
                    // edges are written as `name@version`; the graph holds one
                    // version per name, so the name identifies the node.
                    // A hand-edited lockfile may carry a bare name — accept it
                    // rather than silently dropping the edge.
                    let dep = edge.rsplit_once('@').map(|(n, _)| n).unwrap_or(edge);
                    if !seen.contains(dep) {
                        queue.push(dep.to_string());
                    }
                }
            }
        }
        seen
    }
}

/// The dependency name inside a requirements-fingerprint key: the part after
/// the last `/`. Keys written before v0.31.0 have no owner prefix at all.
pub fn dep_of(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// The requirements-fingerprint key for one owner's dependency.
pub fn req_key(owner: &str, dep: &str) -> String {
    format!("{owner}/{dep}")
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
            tarball_sha256: None,
            dev: false,
            dependencies: vec![],
        });
        lf.insert(LockedPackage {
            name: "http".into(),
            version: "2.1.0".into(),
            registry: "https://example.com/r".into(),
            checksum: "bb".into(),
            tarball_sha256: Some("cc".into()),
            dev: false,
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
        reqs.insert(req_key("my-app", "http"), "^2.0@https://example.com/r".to_string());
        assert!(lf.satisfies_manifests(&reqs).is_ok());

        reqs.insert(req_key("my-app", "http"), "^3.0@https://example.com/r".to_string());
        let err = lf.satisfies_manifests(&reqs).unwrap_err();
        assert!(err.contains("^3.0"), "{err}");

        reqs.clear();
        reqs.insert(req_key("my-app", "missing"), "^1@r".to_string());
        assert!(lf.satisfies_manifests(&reqs).is_err());
    }

    #[test]
    fn legacy_unqualified_requirement_keys_still_validate() {
        // a lockfile written before v0.31.0 keyed requirements by bare
        // dependency name; those files must keep validating
        let lf = sample();
        let mut reqs = BTreeMap::new();
        reqs.insert("http".to_string(), "^2.0@https://example.com/r".to_string());
        assert!(lf.satisfies_manifests(&reqs).is_ok());
        assert_eq!(dep_of("http"), "http");
        assert_eq!(dep_of("my-app/http"), "http");
    }

    #[test]
    fn closure_walks_transitive_edges() {
        let mut lf = sample();
        // http -> zlib, zlib -> (nothing)
        let prod = lf.closure(&["http".to_string()]);
        assert_eq!(prod.len(), 2);
        assert!(prod.contains("http") && prod.contains("zlib"));

        // a dev-only root reaches nothing else
        lf.insert(LockedPackage {
            name: "harness".into(),
            version: "2.0.0".into(),
            registry: "https://example.com/r".into(),
            checksum: "dd".into(),
            tarball_sha256: None,
            dev: true,
            dependencies: vec![],
        });
        let all = lf.closure(&["http".to_string(), "harness".to_string()]);
        assert_eq!(all.len(), 3);
        assert!(!prod.contains("harness"));
    }

    #[test]
    fn closure_terminates_on_cycles() {
        // a malformed lockfile with a dependency cycle must not hang
        let mut lf = Lockfile::empty();
        for (name, deps) in [
            ("a", vec!["b@1.0.0".to_string()]),
            ("b", vec!["a@1.0.0".to_string()]),
        ] {
            lf.insert(LockedPackage {
                name: name.into(),
                version: "1.0.0".into(),
                registry: "r".into(),
                checksum: "x".into(),
                tarball_sha256: None,
                dev: false,
                dependencies: deps,
            });
        }
        assert_eq!(lf.closure(&["a".to_string()]).len(), 2);
    }

    #[test]
    fn v1_lockfile_without_the_new_fields_still_loads() {
        // exactly what an older aoxn wrote: no tarball_sha256, no dev flag
        let legacy = r#"{
  "lockfile_version": 1,
  "requirements": { "http": "^2.0@https://example.com/r@C:/x" },
  "packages": [
    {
      "name": "http",
      "version": "2.1.0",
      "registry": "https://example.com/r",
      "checksum": "bb",
      "dependencies": []
    }
  ]
}"#;
        let lf: Lockfile = serde_json::from_str(legacy).unwrap();
        assert_eq!(lf.lockfile_version, LOCKFILE_VERSION);
        let p = lf.get("http").unwrap();
        assert_eq!(p.tarball_sha256, None);
        assert!(!p.dev, "a package with no dev flag is not dev-only");
    }

    #[test]
    fn dev_only_packages_are_marked_in_the_written_file() {
        let mut lf = Lockfile::empty();
        lf.insert(LockedPackage {
            name: "harness".into(),
            version: "2.0.0".into(),
            registry: "r".into(),
            checksum: "dd".into(),
            tarball_sha256: None,
            dev: true,
            dependencies: vec![],
        });
        lf.insert(LockedPackage {
            name: "http".into(),
            version: "1.0.0".into(),
            registry: "r".into(),
            checksum: "aa".into(),
            tarball_sha256: None,
            dev: false,
            dependencies: vec![],
        });
        lf.normalize();
        let text = serde_json::to_string_pretty(&lf).unwrap();
        assert!(text.contains("\"dev\": true"), "{text}");
        // a prod package omits the flag entirely (skip_serializing_if)
        assert_eq!(text.matches("\"dev\": true").count(), 1, "{text}");
    }
}
