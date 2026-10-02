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
use crate::registry::http::HttpRegistry;
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

/// Which manifest dependency tables take part in resolution / materialization.
///
/// Dev dependencies are resolved together with prod ones into a *single*
/// lockfile (pip-style two-file setups and pnpm's two lockfiles both make
/// reproducibility harder); the scope only decides which of the resolved
/// packages actually get written into `aox_modules/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepScope {
    /// `dependencies` only
    Prod,
    /// `dependencies` + `devDependencies`
    All,
    /// `devDependencies` only (a dev-tool slice)
    DevOnly,
}

impl DepScope {
    pub fn include_prod(self) -> bool {
        !matches!(self, DepScope::DevOnly)
    }

    pub fn include_dev(self) -> bool {
        !matches!(self, DepScope::Prod)
    }
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

    /// A context with no project behind it — just enough to talk to a
    /// registry. `aoxn trust bootstrap` runs outside any project and only
    /// needs to read a registry's `trust.json`, so it must not be forced to
    /// discover a manifest first.
    pub fn probe(url: &str, offline: bool) -> Result<Ctx, PkgError> {
        let cache = Cache::global();
        let config = read_global_config(&cache);
        let mut ctx = Ctx::build(PathBuf::new(), Manifest::synthetic(), None, offline);
        ctx.cache = cache;
        ctx.config = config;
        ctx.open_registry(url, None)?;
        Ok(ctx)
    }

    /// The default registry with no project around it, from the global
    /// config or `AOXN_REGISTRY`. `aoxn trust check` is a CI gate and has to
    /// work from a directory that has no `aoxn.json`.
    pub fn probe_default(offline: bool) -> Result<Ctx, PkgError> {
        let url = read_global_config(&Cache::global())
            .default_registry
            .or_else(|| std::env::var("AOXN_REGISTRY").ok())
            .ok_or_else(|| {
                PkgError::Config(
                    "no registry configured. Run \
                     `aoxn trust bootstrap <curated-registry-url>` once."
                        .into(),
                )
            })?;
        Ctx::probe(&url, offline)
    }

    /// The project's context when there is one, otherwise the global
    /// default registry's. Commands that only read a registry should not
    /// require a manifest.
    pub fn discover_or_probe(offline: bool) -> Result<Ctx, PkgError> {
        let cwd = std::env::current_dir().map_err(PkgError::Io)?;
        match Ctx::discover(&cwd, offline) {
            Ok(ctx) => Ok(ctx),
            // no manifest anywhere up the tree: fall back to the globally
            // configured registry. A malformed manifest is a different
            // matter and is reported as-is.
            Err(e) if is_no_project(&e) => match Ctx::probe_default(offline) {
                Ok(ctx) => Ok(ctx),
                Err(_) => Err(e),
            },
            Err(e) => Err(e),
        }
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
        // backend creates `packages/` on first publish), plain `http://`
        // URLs are HTTP registries (read-only), and everything else
        // (`https://` URLs, git@ hosts, missing paths) is git.
        let as_path = Path::new(url);
        let inferred = if as_path.is_dir() {
            "dir"
        } else if url.to_ascii_lowercase().starts_with("http://") {
            "http"
        } else {
            "git"
        };
        let reg: Box<dyn Registry> = match kind.unwrap_or(inferred) {
            "dir" => Box::new(DirRegistry::new(as_path)),
            "http" => Box::new(HttpRegistry::new(url)),
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
    pub fn roots(&mut self, scope: DepScope) -> Result<Vec<RootReq>, PkgError> {
        let mut roots = Vec::new();
        let mut deps: Vec<(String, DependencySpec)> = Vec::new();
        if scope.include_prod() {
            deps.extend(
                self.manifest
                    .dependencies
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        if scope.include_dev() {
            deps.extend(
                self.manifest
                    .dev_dependencies
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        // a name listed in both tables resolves once, under the prod spec
        deps.sort_by(|a, b| a.0.cmp(&b.0));
        deps.dedup_by(|a, b| a.0 == b.0);
        // `overrides` replace the requirement for a package wherever it
        // appears — including when *this manifest* is the one requiring it.
        // Applying them only to transitive edges would leave a direct
        // dependency contradicting its own override.
        let overrides = self.manifest.overrides.clone();
        for (name, spec) in deps {
            if let DependencySpec::Registry { req, registry } = spec {
                let url = self.registry_url(registry.as_deref())?;
                let range = match overrides.get(&name) {
                    Some(forced) => parse_range(forced)?,
                    None => parse_range(&req)?,
                };
                roots.push(RootReq {
                    name,
                    req: range,
                    registry: Some(url),
                });
            }
        }
        // An override on a package nothing depends on yet still pulls it in,
        // so a forced version is always represented in the graph.
        for (name, req) in overrides {
            if roots.iter().any(|r| r.name == name) {
                continue;
            }
            roots.push(RootReq {
                name,
                req: parse_range(&req)?,
                registry: None,
            });
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
    /// project (self + members): `name -> resolved absolute dir -> declared
    /// as a dev dependency`. Dev path dependencies count too — they are
    /// materialized the same way, and only pruned under `--prod`.
    pub fn path_dependencies(&self) -> Result<Vec<(String, PathBuf, bool)>, PkgError> {
        let mut out: Vec<(String, PathBuf, bool)> = Vec::new();
        for (dir, manifest) in self.all_package_manifests()? {
            let mut specs: Vec<(&String, &DependencySpec, bool)> = manifest
                .dependencies
                .iter()
                .map(|(k, v)| (k, v, false))
                .collect();
            specs.extend(manifest.dev_dependencies.iter().map(|(k, v)| (k, v, true)));
            for (dep, spec, declared_dev) in specs {
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
                    // first declaration wins; a package declared as a prod
                    // path dep anywhere stays a prod one
                    let already = out.iter_mut().find(|(n, _, _)| n == &m.name);
                    match already {
                        Some(entry) => entry.2 &= declared_dev,
                        None => out.push((m.name.clone(), abs, declared_dev)),
                    }
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

/// Is this the "there is no project here" error rather than a real problem
/// with one? Discovery reports both as config errors, and only the former
/// may be answered by falling back to a registry-only context.
fn is_no_project(e: &PkgError) -> bool {
    matches!(e, PkgError::Config(msg) if msg.contains("no aoxn.json found"))
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
