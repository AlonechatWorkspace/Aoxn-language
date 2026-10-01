//! `aoxn.json` — the package manifest.
//!
//! ```json
//! {
//!   "name": "my-app",
//!   "version": "0.1.0",
//!   "main": "src/main.ax",
//!   "description": "optional",
//!   "dependencies": {
//!     "http": "^1.2",
//!     "json": { "version": "^1", "registry": "main" },
//!     "my-utils": { "path": "../my-utils" }
//!   },
//!   "workspace": { "members": ["packages/*"] }
//! }
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, DependencySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceDecl>,
    /// named registries; the key `default` is used when a dependency has no
    /// explicit `registry` hint. Values are git URLs (kind inferred) or
    /// directory paths with an explicit `{"kind": "dir"}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, RegistryDecl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDecl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>, // "git" | "dir"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDecl {
    /// glob patterns relative to this manifest's directory (`packages/*`)
    pub members: Vec<String>,
}

/// One entry in `dependencies`. Either a plain semver requirement string
/// (registry dependency) or an object with a `path` (local / workspace
/// member) and optional `version` + `registry` override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySpec {
    Registry {
        req: String,
        registry: Option<String>,
    },
    Path {
        path: PathBuf,
    },
}

impl Serialize for DependencySpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            DependencySpec::Registry { req, registry: None } => s.serialize_str(req),
            DependencySpec::Registry { req, registry: Some(r) } => {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(2))?;
                map.serialize_entry("version", req)?;
                map.serialize_entry("registry", r)?;
                map.end()
            }
            DependencySpec::Path { path } => {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry("path", &path.to_string_lossy())?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for DependencySpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = DependencySpec;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("version requirement string or {\"path\": ...} / {\"version\": ..., \"registry\": ...}")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(DependencySpec::Registry {
                    req: v.to_string(),
                    registry: None,
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
                let mut path = None;
                let mut version = None;
                let mut registry = None;
                while let Some(k) = m.next_key::<String>()? {
                    match k.as_str() {
                        "path" => path = Some(m.next_value::<String>()?),
                        "version" => version = Some(m.next_value::<String>()?),
                        "registry" => registry = Some(m.next_value::<String>()?),
                        other => return Err(serde::de::Error::unknown_field(other, &["path", "version", "registry"])),
                    }
                }
                if let Some(p) = path {
                    return Ok(DependencySpec::Path { path: PathBuf::from(p) });
                }
                match version {
                    Some(v) => Ok(DependencySpec::Registry { req: v, registry }),
                    None => Err(serde::de::Error::missing_field("version")),
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// Package name rule: lowercase letters, digits, `-`, `_`; 1-64 chars;
/// must not start/end with `-` (keeps `name@version` keys unambiguous).
pub fn validate_name(name: &str) -> Result<(), PkgError> {
    let bad = |why: &str| Err(PkgError::Manifest(format!("invalid package name `{name}`: {why}")));
    if name.is_empty() || name.len() > 64 {
        return bad("must be 1-64 characters");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return bad("only lowercase a-z, 0-9, '-' and '_' allowed");
    }
    if name.starts_with('-') || name.ends_with('-') {
        return bad("must not start or end with '-'");
    }
    Ok(())
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Manifest, PkgError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| PkgError::Manifest(format!("cannot read {}: {e}", path.display())))?;
        let m: Manifest = serde_json::from_str(&text).map_err(|e| {
            PkgError::Manifest(format!("invalid {}: {e}", path.display()))
        })?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), PkgError> {
        validate_name(&self.name)?;
        if self.version.parse::<semver::Version>().is_err() {
            return Err(PkgError::Manifest(format!(
                "invalid version `{}` (semver expected, e.g. 1.2.3)",
                self.version
            )));
        }
        for (dep, spec) in &self.dependencies {
            match spec {
                DependencySpec::Registry { req, registry: _ } => {
                    if req.parse::<semver::VersionReq>().is_err() {
                        return Err(PkgError::Manifest(format!(
                            "invalid requirement for dependency `{dep}`: `{req}`"
                        )));
                    }
                }
                DependencySpec::Path { path } => {
                    if path.as_os_str().is_empty() {
                        return Err(PkgError::Manifest(format!(
                            "dependency `{dep}`: empty path"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), PkgError> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text + "\n")?;
        Ok(())
    }

    /// Path to the package's `aoxn.json`.
    pub fn path_for(dir: &Path) -> PathBuf {
        dir.join("aoxn.json")
    }

    pub fn exists_in(dir: &Path) -> bool {
        Manifest::path_for(dir).exists()
    }

    /// The declared entry file, or the conventional probe order
    /// (`index.ax` then `main.ax`, also `.ts`/`.tsx`) when unset.
    /// Returns a path relative to the package root, or None when nothing exists.
    pub fn entry_file(&self, pkg_dir: &Path) -> Option<PathBuf> {
        if let Some(main) = &self.main {
            let p = PathBuf::from(main);
            return if pkg_dir.join(&p).exists() { Some(p) } else { None };
        }
        for cand in ["index.ax", "index.ts", "index.tsx", "main.ax", "main.ts", "main.tsx"] {
            if pkg_dir.join(cand).exists() {
                return Some(PathBuf::from(cand));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_object_deps() {
        let m: Manifest = serde_json::from_str(
            r#"{
                "name": "app",
                "version": "0.1.0",
                "dependencies": {
                    "http": "^1.2",
                    "json": {"version": "^1", "registry": "mirror"},
                    "local-lib": {"path": "../local-lib"}
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            m.dependencies.get("http"),
            Some(&DependencySpec::Registry { req: "^1.2".into(), registry: None })
        );
        assert_eq!(
            m.dependencies.get("local-lib"),
            Some(&DependencySpec::Path { path: PathBuf::from("../local-lib") })
        );
    }

    #[test]
    fn rejects_bad_names() {
        assert!(validate_name("MyPkg").is_err());
        assert!(validate_name("-x").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("ok_name-2").is_ok());
    }

    #[test]
    fn roundtrip_is_stable() {
        let m: Manifest = serde_json::from_str(
            r#"{"name":"a","version":"1.0.0","dependencies":{"b":"^0.1"}}"#,
        )
        .unwrap();
        let text = serde_json::to_string_pretty(&m).unwrap();
        let m2: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!(serde_json::to_string_pretty(&m2).unwrap(), text);
    }
}
