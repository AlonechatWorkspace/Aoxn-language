//! The per-invocation context: discovered project + workspace, cache,
//! registries, global config. Every command receives a `Ctx`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::errors::PkgError;
use crate::lockfile::Lockfile;
use crate::manifest::{DependencySpec, Manifest};
use crate::registry::dir::DirRegistry;
use crate::registry::git::GitRegistry;
use crate::registry::{PackageIndex, Registry};
use crate::resolve::{parse_range, PackageSource, RootReq};
use crate::workspace::{self, Workspace};

/// Global config (`$AOXN_HOME/config.json`).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GlobalConfig {
    /// registry used when a manifest declares none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_registry: Option<String>,
    /// advisory database: a git repo or directory of advisory JSON files
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisories: Option<AdvisoriesSource>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdvisoriesSource {
    pub git: Option<String>,
    pub path: Option<String>,
}

pub struct Ctx {
    /// directory of the nearest aoxn.json
    pub project: PathBuf,
    pub manifest: Manifest,
    pub ws: Option<Workspace>,
    pub cache: Cache,
    pub offline: bool,
    pub config: GlobalConfig,
    registries: HashMap<String, Box<dyn Registry>>,
}

impl Ctx {
    /// Discover the project (and workspace) from `cwd` and open the cache.
    /// Fails when there is no manifest — except for `skip_manifest` commands
    /// (`init`, `cache`).
    pub fn discover(cwd: &Path, offline: bool) -> Result<Ctx, PkgError> {
        let (project, ws) = workspace::discover(cwd)?;
        let manifest = Manifest::load(&Manifest::path_for(&project))?;
        Ok(Ctx::build(project, manifest, ws, offline))
    }

    fn build(project: PathBuf, manifest: Manifest, ws: Option<Workspace>, offline: bool) -> Ctx {
        let cache = Cache::global();
        let config = read_global_config(&cache);
        Ctx {
            project,
            manifest,
            ws,
            cache,
            offline,
            config,
            registries: HashMap::new(),
        }
    }

    /// Path of the lockfile governing this project.
    pub fn lockfile_path(&self) -> PathBuf {
        workspace::lockfile_path(&self.project, self.ws.as_ref())
    }

    pub fn load_lockfile(&self) -> Result<Option<Lockfile>, PkgError> {
        Lockfile::load(&self.lockfile_path())
    }

    /// Resolve a registry hint to a registry URL. Priority: explicit hint
    /// (name in the manifest's `registries` map, or a literal URL) >
    /// manifest `default` > `AOXN_REGISTRY` env > global config.
    pub fn registry_url(&mut self, hint: Option<&str>) -> Result<String, PkgError> {
        let declared = self.manifest.registries.clone();
        let lookup = |key: &str| -> Option<String> {
            match declared.get(key) {
                Some(decl) => Some(decl.url.clone()),
                None => None,
            }
        };
        if let Some(h) = hint {
            if let Some(url) = lookup(h) {
                return Ok(url);
            }
            // allow a raw URL / path as the hint itself
            if h.contains("://") || h.contains('/') || h.contains('\\') {
                return Ok(h.to_string());
            }
            return Err(PkgError::Config(format!(
                "registry `{h}` is not declared in aoxn.json `registries`"
            )));
        }
        if let Some(url) = lookup("default") {
            return Ok(url);
        }
        if let Some(url) = self.manifest.registries.values().next().map(|d| d.url.clone()) {
            return Ok(url);
        }
        if let Some(url) = std::env::var_os("AOXN_REGISTRY") {
            return Ok(url.to_string_lossy().to_string());
        }
        if let Some(url) = &self.config.default_registry {
            return Ok(url.clone());
        }
        Err(PkgError::Config(
            "no registry configured. Add to aoxn.json:\n  \
             \"registries\": { \"default\": { \"url\": \"https://github.com/you/aoxn-registry\" } }\n\
             or set the AOXN_REGISTRY environment variable"
                .into(),
        ))
    }

    /// Open (and memoize) a registry backend for a URL. Kind inference: an
    /// existing local directory containing `packages/` is a dir registry,
    /// anything else is git.
    pub fn open_registry(&mut self, url: &str, kind: Option<&str>) -> Result<(), PkgError> {
        if self.registries.contains_key(url) {
            return Ok(());
        }
        // Kind inference: an explicit `kind` wins; otherwise any existing
        // local directory is a dir registry (fresh ones included — the
        // backend creates `packages/` on first publish), everything else
        // (http(s) URLs, git@ hosts, missing paths) is git.
        let as_path = Path::new(url);
        let inferred = if as_path.is_dir() { "dir" } else { "git" };
        let reg: Box<dyn Registry> = match kind.unwrap_or(inferred) {
            "dir" => Box::new(DirRegistry::new(as_path)),
            _ => Box::new(GitRegistry::new(url, &self.cache)),
        };
        self.registries.insert(url.to_string(), reg);
        Ok(())
    }

    pub(crate) fn registry_for(&mut self, url: &str) -> Result<&mut dyn Registry, PkgError> {
        if !self.registries.contains_key(url) {
            self.open_registry(url, None)?;
        }
        Ok(self.registries.get_mut(url).unwrap().as_mut())
    }

    /// Root requirements for the resolver, built from the manifest deps.
    /// Path dependencies are not part of registry resolution.
    pub fn roots(&mut self) -> Result<Vec<RootReq>, PkgError> {
        let mut roots = Vec::new();
        let deps: Vec<(String, DependencySpec)> = self
            .manifest
            .dependencies
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, spec) in deps {
            if let DependencySpec::Registry { req, registry } = spec {
                let url = self.registry_url(registry.as_deref())?;
                roots.push(RootReq {
                    name,
                    req: parse_range(&req)?,
                    registry: Some(url),
                });
            }
        }
        Ok(roots)
    }

    /// All packages that participate in the shared lockfile: this project
    /// plus (in a workspace) every member and the root.
    pub fn all_package_manifests(&self) -> Result<Vec<(PathBuf, Manifest)>, PkgError> {
        if let Some(ws) = &self.ws {
            let root_manifest = Manifest::load(&Manifest::path_for(&ws.root))?;
            return ws.all_packages(Some(root_manifest));
        }
        Ok(vec![(self.project.clone(), self.manifest.clone())])
    }

    /// Collect the `{"path": ...}` dependencies of every package in the
    /// project (self + members): `name -> resolved absolute dir`.
    pub fn path_dependencies(&self) -> Result<Vec<(String, PathBuf)>, PkgError> {
        let mut out: Vec<(String, PathBuf)> = Vec::new();
        for (dir, manifest) in self.all_package_manifests()? {
            for (dep, spec) in &manifest.dependencies {
                if let DependencySpec::Path { path } = spec {
                    let abs = dir.join(path).canonicalize().map_err(|_| {
                        PkgError::Manifest(format!(
                            "path dependency `{dep}` of `{}`: `{}` does not exist",
                            manifest.name,
                            path.display()
                        ))
                    })?;
                    if !Manifest::exists_in(&abs) {
                        return Err(PkgError::Manifest(format!(
                            "path dependency `{dep}` of `{}`: {} has no aoxn.json",
                            manifest.name,
                            abs.display()
                        )));
                    }
                    let m = Manifest::load(&Manifest::path_for(&abs))?;
                    if out.iter().any(|(n, _)| n == &m.name) {
                        continue;
                    }
                    out.push((m.name.clone(), abs));
                }
            }
        }
        Ok(out)
    }
}

fn read_global_config(cache: &Cache) -> GlobalConfig {
    std::fs::read_to_string(cache.config_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

impl PackageSource for Ctx {
    fn index_for(&mut self, name: &str, registry_hint: Option<&str>) -> Result<PackageIndex, PkgError> {
        // the hint arriving here is already a resolved registry URL (see
        // Ctx::roots) — but tolerate a name for direct calls in tests
        let url = if registry_hint.is_none() {
            self.registry_url(None)?
        } else {
            registry_hint.unwrap().to_string()
        };
        let reg = self.registry_for(&url)?;
        reg.index(name)
    }
}
