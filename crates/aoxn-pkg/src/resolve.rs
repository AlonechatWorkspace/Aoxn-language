//! Dependency resolution built on the PubGrub algorithm.
//!
//! **Minimal disturbance via soft preference** (single stage): candidate
//! versions for a package are ordered *locked-version first, then newest
//! first*. The solver picks the preferred version whenever the constraints
//! allow it and backtracks to the next candidate when they don't — so
//! adding a package never bumps unrelated packages, yet a genuine conflict
//! upgrades exactly the packages that must change. The result is unique and
//! independent of the order requirements are listed in (PubGrub is
//! deterministic and the candidate order depends only on the lockfile).
//!
//! Yanked versions are candidates only when they are the locked preference
//! (an existing lockfile keeps working; fresh selections never pick yanked).
//!
//! The `pubgrub` dependency (0.3.x) and its `DependencyProvider` trait are
//! confined to this file; swapping the solver version must not leak out.
//!
//! Failures render as a PubGrub derivation ("a 1.x depends on shared ^1, b
//! 1.0 depends on shared ^2, so shared cannot be chosen") instead of a bare
//! "could not resolve".

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use pubgrub::{DefaultStringReporter, Dependencies, DependencyProvider, PubGrubError, Reporter};
use pubgrub::{Ranges, SemanticVersion};

use crate::errors::PkgError;
use crate::registry::PackageIndex;

type SemVS = Ranges<SemanticVersion>;
type Constraints = pubgrub::DependencyConstraints<String, SemVS>;

/// Where the resolver gets package indexes from. Implemented by the CLI
/// context on top of the registry backends; `registry_hint` is the resolved
/// registry URL carried by a root requirement (transitive deps inherit
/// their parent's registry).
pub trait PackageSource {
    fn index_for(&mut self, name: &str, registry_hint: Option<&str>) -> Result<PackageIndex, PkgError>;
}

/// A root requirement: (dependency name, range, resolved registry URL).
#[derive(Clone)]
pub struct RootReq {
    pub name: String,
    pub req: SemVS,
    pub registry: Option<String>,
}

/// Convert a semver requirement string to a PubGrub range.
/// Supports what `semver::VersionReq` supports: `^`, `~`, `=`, `>`, `>=`,
/// `<`, `<=`, `*` and wildcards (`1.x`, `1.2.x`).
pub fn parse_range(req: &str) -> Result<SemVS, PkgError> {
    let vr: semver::VersionReq = req.parse().map_err(|e| {
        PkgError::Manifest(format!("invalid version requirement `{req}`: {e}"))
    })?;
    let mut acc = SemVS::full();
    for comp in &vr.comparators {
        let r = match comp.op {
            semver::Op::Exact => SemVS::singleton(mk(comp.major, comp.minor, comp.patch)),
            semver::Op::Greater => SemVS::strictly_higher_than(mk3(comp)),
            semver::Op::GreaterEq => SemVS::higher_than(mk3(comp)),
            semver::Op::Less => SemVS::strictly_lower_than(mk3(comp)),
            semver::Op::LessEq => SemVS::lower_than(mk3(comp)),
            semver::Op::Caret => {
                let (lo_major, lo_minor, lo_patch, hi) = caret_bounds(comp);
                SemVS::between(mk(lo_major, lo_minor, lo_patch), hi)
            }
            semver::Op::Tilde => {
                let hi = match (comp.minor, comp.patch) {
                    (Some(minor), _) => sv(comp.major, minor + 1, 0),
                    (None, _) => sv(comp.major + 1, 0, 0),
                };
                SemVS::between(sv(comp.major, comp.minor.unwrap_or(0), 0), hi)
            }
            semver::Op::Wildcard => {
                let hi = match comp.minor {
                    Some(minor) => sv(comp.major, minor + 1, 0),
                    None => sv(comp.major + 1, 0, 0),
                };
                SemVS::between(sv(comp.major, comp.minor.unwrap_or(0), 0), hi)
            }
            _ => {
                return Err(PkgError::Manifest(format!(
                    "unsupported operator in requirement `{req}`"
                )))
            }
        };
        acc = acc.intersection(&r);
    }
    Ok(acc)
}

fn mk(major: u64, minor: Option<u64>, patch: Option<u64>) -> SemanticVersion {
    SemanticVersion::new(major as u32, minor.unwrap_or(0) as u32, patch.unwrap_or(0) as u32)
}

fn mk3(comp: &semver::Comparator) -> SemanticVersion {
    mk(comp.major, comp.minor, comp.patch)
}

fn sv(major: u64, minor: u64, patch: u64) -> SemanticVersion {
    SemanticVersion::new(major as u32, minor as u32, patch as u32)
}

/// `^x.y.z` keeps the leftmost non-zero component fixed (semver caret rule).
fn caret_bounds(comp: &semver::Comparator) -> (u64, Option<u64>, Option<u64>, SemanticVersion) {
    match (comp.minor, comp.patch) {
        (Some(minor), Some(patch)) => {
            let hi = if comp.major == 0 {
                if minor == 0 {
                    sv(0, 0, patch + 1)
                } else {
                    sv(0, minor + 1, 0)
                }
            } else {
                sv(comp.major + 1, 0, 0)
            };
            (comp.major, Some(minor), Some(patch), hi)
        }
        (Some(minor), None) => {
            // ^1.2 == ^1.2.0
            let hi = if comp.major == 0 {
                sv(0, minor + 1, 0)
            } else {
                sv(comp.major + 1, 0, 0)
            };
            (comp.major, Some(minor), Some(0), hi)
        }
        (None, _) => {
            // ^1 == ^1.0.0
            (comp.major, Some(0), Some(0), sv(comp.major + 1, 0, 0))
        }
    }
}

/// Outcome of a successful resolution.
#[derive(Debug)]
pub struct Resolution {
    /// `name -> selected metadata`
    pub packages: BTreeMap<String, Selected>,
}

#[derive(Debug, Clone)]
pub struct Selected {
    pub version: String,
    pub registry: String,
    /// manifest hash of the published package (see `tarball::manifest_hash`)
    pub checksum: String,
    /// sha256 of the tarball bytes as served, when the registry publishes it
    /// (the *transport* digest; `checksum` is the content anchor)
    pub tarball_sha256: Option<String>,
    /// direct dependencies (name -> requirement as published in the index)
    pub dependencies: BTreeMap<String, String>,
    pub deprecated: Option<String>,
    pub yanked: bool,
    /// minimum compiler version the package declares (`IndexVersion.aoxn`).
    /// Parsed by registries since v0.29.0 but unenforced until v0.31.0.
    pub min_aoxn: Option<String>,
}

/// The resolver's `DependencyProvider`: lazily pulls package indexes from
/// the [`PackageSource`], orders candidates by the soft-preference rule,
/// filters yanked versions, and records which registry each package came
/// from (transitive deps inherit their parent's registry).
struct Provider<'a> {
    source: RefCell<&'a mut dyn PackageSource>,
    /// registry URL for packages whose ownership is not otherwise recorded
    default_registry: String,
    /// soft preference: locked version tried first per package
    locked: BTreeMap<String, SemanticVersion>,
    indexes: RefCell<HashMap<String, Rc<PackageIndex>>>,
    /// which registry each package was required from (first requirement wins)
    registry_of: RefCell<HashMap<String, String>>,
    /// per-(package,version) dependency constraints cache
    deps: RefCell<HashMap<(String, SemanticVersion), Constraints>>,
    /// root requirements (the virtual root package)
    roots: Vec<RootReq>,
    /// forced requirements from the manifest's `overrides` (pnpm/Cargo
    /// `patch` semantics): wherever the graph mentions this package, the
    /// published requirement is replaced by ours
    overrides: BTreeMap<String, SemVS>,
    /// index fetch failures encountered (reported on resolution failure)
    errors: RefCell<Vec<String>>,
}

/// The virtual root package name.
const ROOT: &str = "";

impl<'a> Provider<'a> {
    fn new(
        source: &'a mut dyn PackageSource,
        default_registry: &str,
        locked: BTreeMap<String, SemanticVersion>,
        roots: Vec<RootReq>,
        overrides: BTreeMap<String, SemVS>,
    ) -> Self {
        Provider {
            source: RefCell::new(source),
            default_registry: default_registry.to_string(),
            locked,
            indexes: RefCell::new(HashMap::new()),
            registry_of: RefCell::new(HashMap::new()),
            deps: RefCell::new(HashMap::new()),
            roots,
            overrides,
            errors: RefCell::new(Vec::new()),
        }
    }

    fn index_of(&self, name: &str, registry: Option<&str>) -> Option<Rc<PackageIndex>> {
        let key = registry.map(|r| format!("{r}::{name}")).unwrap_or_else(|| name.to_string());
        if let Some(idx) = self.indexes.borrow().get(&key) {
            return Some(idx.clone());
        }
        match self.source.borrow_mut().index_for(name, registry) {
            Ok(idx) => {
                let rc = Rc::new(idx);
                self.indexes.borrow_mut().insert(key, rc.clone());
                Some(rc)
            }
            Err(PkgError::PackageNotFound(n)) => {
                self.errors.borrow_mut().push(format!("package `{n}` not found"));
                None
            }
            Err(e) => {
                self.errors.borrow_mut().push(format!("{e}"));
                None
            }
        }
    }

    /// Record which registry owns each package (first requirement wins).
    /// A per-entry hint overrides the parent chain; otherwise the parent's
    /// registry (or the process default) is inherited.
    fn record_registries<'i, I>(&self, entries: I, parent_registry: Option<&str>)
    where
        I: Iterator<Item = (&'i String, Option<&'i str>)>,
    {
        let mut reg = self.registry_of.borrow_mut();
        for (name, hint) in entries {
            let owner = hint
                .map(|h| h.to_string())
                .or_else(|| parent_registry.map(|p| p.to_string()))
                .unwrap_or_else(|| self.default_registry.clone());
            reg.entry(name.clone()).or_insert(owner);
        }
    }
}

/// Marker error type for provider failures (indexes are fetched lazily and
/// errors are collected; resolution fails with the rendered derivation plus
/// the fetch errors).
#[derive(Debug)]
pub struct ResolveFail;

impl std::error::Error for ResolveFail {}

impl fmt::Display for ResolveFail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "package index unavailable")
    }
}

impl DependencyProvider for Provider<'_> {
    type P = String;
    type V = SemanticVersion;
    type VS = SemVS;
    type M = String;
    type Err = ResolveFail;
    type Priority = (u32, std::cmp::Reverse<usize>);

    fn prioritize(
        &self,
        _package: &Self::P,
        _range: &Self::VS,
        stats: &pubgrub::PackageResolutionStatistics,
    ) -> Self::Priority {
        // heuristic mirrored from OfflineDependencyProvider: decide packages
        // with more conflicts first
        (stats.conflict_count(), std::cmp::Reverse(0))
    }

    fn choose_version(&self, package: &Self::P, range: &Self::VS) -> Result<Option<Self::V>, Self::Err> {
        if package.as_str() == ROOT {
            return Ok(Some(SemanticVersion::zero()));
        }
        let registry = self.registry_of.borrow().get(package).cloned();
        let Some(idx) = self.index_of(package, registry.as_deref()) else {
            return Ok(None);
        };
        let preference = self.locked.get(package).cloned();
        let mut candidates: Vec<SemanticVersion> = idx
            .versions
            .keys()
            .filter_map(|v| v.parse::<SemanticVersion>().ok())
            .filter(|v| {
                if let Some(entry) = idx.versions.get::<str>(&v.to_string()) {
                    if entry.yanked {
                        // yanked only as the locked preference (supply-chain:
                        // existing lockfiles keep installing, fresh picks skip)
                        return preference.as_ref() == Some(v);
                    }
                }
                range.contains(v)
            })
            .collect();
        // soft preference: locked version first, then newest first
        candidates.sort_by_key(|v| (preference.as_ref() != Some(v), std::cmp::Reverse(*v)));
        Ok(candidates.into_iter().next())
    }

    fn get_dependencies(
        &self,
        package: &Self::P,
        version: &Self::V,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        if package.as_str() == ROOT {
            let mut cons = Constraints::default();
            for r in &self.roots {
                cons.insert(r.name.clone(), r.req.clone());
            }
            // root requirements carry their own (already resolved) registry URLs
            let hints: Vec<(String, Option<String>)> = self
                .roots
                .iter()
                .map(|r| (r.name.clone(), r.registry.clone()))
                .collect();
            self.record_registries(hints.iter().map(|(n, h)| (n, h.as_deref())), None);
            return Ok(Dependencies::Available(cons));
        }
        let registry = self.registry_of.borrow().get(package).cloned();
        let Some(idx) = self.index_of(package, registry.as_deref()) else {
            return Ok(Dependencies::Unavailable(format!(
                "no usable index for `{package}`"
            )));
        };
        let key = (package.clone(), *version);
        if let Some(cached) = self.deps.borrow().get(&key).cloned() {
            let names: Vec<String> = cached.keys().cloned().collect();
            self.record_registries(names.iter().map(|n| (n, None)), registry.as_deref());
            return Ok(Dependencies::Available(cached));
        }
        let Some(entry) = idx.versions.get::<str>(&version.to_string()) else {
            return Ok(Dependencies::Unavailable(format!(
                "`{package}@{version}` missing from registry index"
            )));
        };
        let mut cons = Constraints::default();
        for (dep_name, req) in &entry.dependencies {
            // an `overrides` entry wins over what the index declares — the
            // whole point is forcing a version the graph did not ask for
            if let Some(forced) = self.overrides.get(dep_name) {
                cons.insert(dep_name.clone(), forced.clone());
                continue;
            }
            match parse_range(req) {
                Ok(r) => {
                    cons.insert(dep_name.clone(), r);
                }
                Err(_) => {
                    return Ok(Dependencies::Unavailable(format!(
                        "`{package}@{version}` has invalid requirement `{dep_name}: {req}`"
                    )))
                }
            }
        }
        self.deps.borrow_mut().insert(key, cons.clone());
        // transitive deps inherit the parent's registry (no hints in indexes)
        let names: Vec<String> = cons.keys().cloned().collect();
        self.record_registries(names.iter().map(|n| (n, None)), registry.as_deref());
        Ok(Dependencies::Available(cons))
    }
}

/// Resolve `roots` given the package source, the default registry URL and
/// the locked versions (soft preference — see the module docs).
///
/// `overrides` are forced requirements (`aoxn.json` `"overrides"`): wherever
/// the graph mentions one of these packages, the published requirement is
/// discarded in favour of ours.
pub fn resolve_deps(
    roots: Vec<RootReq>,
    overrides: BTreeMap<String, String>,
    source: &mut dyn PackageSource,
    default_registry: &str,
    locked: &BTreeMap<String, String>,
) -> Result<Resolution, PkgError> {
    let locked: BTreeMap<String, SemanticVersion> = locked
        .iter()
        .filter_map(|(k, v)| v.parse().ok().map(|sv| (k.clone(), sv)))
        .collect();
    let mut forced: BTreeMap<String, SemVS> = BTreeMap::new();
    for (name, req) in &overrides {
        forced.insert(name.clone(), parse_range(req)?);
    }
    let p = Provider::new(source, default_registry, locked, roots, forced);
    match pubgrub::resolve(&p, ROOT.to_string(), SemanticVersion::zero()) {
        Ok(sel) => Ok(finish(&p, &sel)),
        Err(PubGrubError::NoSolution(tree)) => {
            let mut detail = DefaultStringReporter::report(&tree);
            let fetch_errors = p.errors.borrow();
            if !fetch_errors.is_empty() {
                detail.push_str("\n\nalso encountered while resolving:");
                for e in fetch_errors.iter() {
                    detail.push_str(&format!("\n  - {e}"));
                }
            }
            Err(PkgError::Resolve {
                message: "no version set satisfies all dependencies".into(),
                explain: Some(detail),
            })
        }
        Err(PubGrubError::ErrorRetrievingDependencies { package, version, .. }) => Err(PkgError::Resolve {
            message: format!("could not read dependencies of {package}@{version}"),
            explain: None,
        }),
        Err(PubGrubError::ErrorChoosingVersion { package, .. }) => Err(PkgError::Resolve {
            message: format!("could not enumerate versions of `{package}`"),
            explain: None,
        }),
        Err(e) => Err(PkgError::Resolve {
            message: format!("resolution failed: {e}"),
            explain: None,
        }),
    }
}

fn finish(
    provider: &Provider<'_>,
    sel: &pubgrub::SelectedDependencies<Provider<'_>>,
) -> Resolution {
    let mut packages = BTreeMap::new();
    for (name, version) in sel.iter() {
        if name.as_str() == ROOT {
            continue;
        }
        let registry = provider
            .registry_of
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or_default();
        // Look the index entry up by the (registry, name) the resolver
        // actually used. Scanning the memo for "some index whose name
        // matches" picked a same-named package from a *different* registry
        // whenever one existed, silently attaching the wrong checksum.
        let entry = provider
            .indexes
            .borrow()
            .get(&format!("{registry}::{name}"))
            .and_then(|idx| idx.versions.get::<str>(&version.to_string()).cloned());
        let (dependencies, checksum, tarball_sha256, deprecated, yanked, min_aoxn) = entry
            .map(|e| {
                (
                    e.dependencies.clone(),
                    e.checksum.clone(),
                    e.tarball_sha256.clone(),
                    e.deprecated.clone(),
                    e.yanked,
                    e.aoxn.clone(),
                )
            })
            .unwrap_or((Default::default(), String::new(), None, None, false, None));
        packages.insert(
            name.clone(),
            Selected {
                version: version.to_string(),
                registry,
                checksum,
                tarball_sha256,
                dependencies,
                deprecated,
                yanked,
                min_aoxn,
            },
        );
    }
    Resolution { packages }
}

/// Names in a resolution that are not in `prev` — the newly added set (used
/// for typosquat warnings on add/install).
pub fn added_packages(res: &Resolution, prev: &HashSet<String>) -> Vec<String> {
    res.packages
        .keys()
        .filter(|k| !prev.contains(*k))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// In-memory PackageSource for tests.
    struct MemSource {
        packages: HashMap<String, BTreeMap<String, BTreeMap<String, String>>>,
        /// packages whose index carries yanked/deprecated markers
        flags: HashMap<String, HashMap<String, (bool, Option<String>)>>,
    }

    impl PackageSource for MemSource {
        fn index_for(&mut self, name: &str, _registry: Option<&str>) -> Result<PackageIndex, PkgError> {
            let Some(versions) = self.packages.get(name) else {
                return Err(PkgError::PackageNotFound(name.into()));
            };
            let mut idx = PackageIndex {
                name: name.into(),
                versions: Default::default(),
                ..Default::default()
            };
            for (v, deps) in versions {
                let (yanked, deprecated) = self
                    .flags
                    .get(name)
                    .and_then(|f| f.get(v))
                    .cloned()
                    .unwrap_or((false, None));
                idx.versions.insert(
                    v.clone(),
                    crate::registry::IndexVersion {
                        checksum: format!("fake-{}-{}", name, v),
                        dependencies: deps.clone(),
                        yanked,
                        deprecated,
                        ..Default::default()
                    },
                );
            }
            Ok(idx)
        }
    }

    fn roots(deps: &[(&str, &str)]) -> Vec<RootReq> {
        deps.iter()
            .map(|(n, r)| RootReq {
                name: n.to_string(),
                req: parse_range(r).unwrap(),
                registry: Some("test://registry".into()),
            })
            .collect()
    }

    fn no_locked() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn resolve_deps3(
        roots: Vec<RootReq>,
        source: &mut MemSource,
        locked: &BTreeMap<String, String>,
    ) -> Result<Resolution, PkgError> {
        resolve_deps3_with(roots, BTreeMap::new(), source, locked)
    }

    fn resolve_deps3_with(
        roots: Vec<RootReq>,
        overrides: BTreeMap<String, String>,
        source: &mut MemSource,
        locked: &BTreeMap<String, String>,
    ) -> Result<Resolution, PkgError> {
        resolve_deps(roots, overrides, source, "test://registry", locked)
    }

    type Spec = (&'static str, Vec<(&'static str, Vec<(&'static str, &'static str)>)>);
    fn source(specs: Vec<Spec>) -> MemSource {
        let mut packages = HashMap::new();
        for (name, versions) in specs {
            let mut vmap = BTreeMap::new();
            for (v, deps) in versions {
                let dmap: BTreeMap<String, String> = deps
                    .iter()
                    .map(|(d, r)| (d.to_string(), r.to_string()))
                    .collect();
                vmap.insert(v.to_string(), dmap);
            }
            packages.insert(name.to_string(), vmap);
        }
        MemSource {
            packages,
            flags: HashMap::new(),
        }
    }

    fn version_of(res: &Resolution, name: &str) -> String {
        res.packages.get(name).unwrap().version.clone()
    }

    #[test]
    fn picks_highest_compatible() {
        let mut src = source(vec![("a", vec![("1.0.0", vec![]), ("1.2.0", vec![]), ("2.0.0", vec![])])]);
        let res = resolve_deps3(roots(&[("a", "^1")]), &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "a"), "1.2.0");
    }

    #[test]
    fn transitive_resolution() {
        let mut src = source(vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.1.0", vec![("util", "~2.1")]), ("1.0.0", vec![])]),
            ("util", vec![("2.1.5", vec![]), ("2.2.0", vec![])]),
        ]);
        let res = resolve_deps3(roots(&[("app", "^1")]), &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "lib"), "1.1.0");
        assert_eq!(version_of(&res, "util"), "2.1.5");
    }

    #[test]
    fn conflict_produces_explainable_error() {
        let mut src = source(vec![
            ("a", vec![("1.0.0", vec![("shared", "^1")])]),
            ("b", vec![("1.0.0", vec![("shared", "^2")])]),
            ("shared", vec![("1.5.0", vec![]), ("2.5.0", vec![])]),
        ]);
        let err = resolve_deps3(roots(&[("a", "^1"), ("b", "^1")]), &mut src, &no_locked()).unwrap_err();
        match err {
            PkgError::Resolve { explain, .. } => {
                let e = explain.unwrap();
                assert!(e.contains("shared"), "report should name the conflicting package:\n{e}");
                assert!(e.contains("a") && e.contains("b"), "report should trace the requirers:\n{e}");
            }
            other => panic!("expected resolve error, got {other}"),
        }
    }

    #[test]
    fn locked_versions_are_preferred_softly() {
        // locked: a@1.0.0. Adding b whose dep is compatible with a@1.0.0:
        // the preference keeps a@1.0.0 even though a@1.9.0 exists.
        let mut src = source(vec![
            ("a", vec![("1.0.0", vec![]), ("1.9.0", vec![])]),
            ("b", vec![("1.0.0", vec![("a", "^1")])]),
        ]);
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.0.0".to_string());
        let res = resolve_deps3(roots(&[("b", "^1")]), &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "1.0.0", "preference must prevent the upgrade");
        assert_eq!(version_of(&res, "b"), "1.0.0");
    }

    #[test]
    fn preference_breaks_only_when_forced() {
        // locked: a@1.0.0; new package requires a ^2. The solver must
        // backtrack past the preference (no false conflict) and pick a@2.
        let mut src = source(vec![
            ("a", vec![("1.0.0", vec![]), ("2.3.0", vec![])]),
            ("c", vec![("1.0.0", vec![("a", "^2")])]),
        ]);
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.0.0".to_string());
        let res = resolve_deps3(roots(&[("c", "^1")]), &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "2.3.0");
    }

    #[test]
    fn minimal_disturbance_on_conflict() {
        // locked: a@1.0.0 and b@1.0.0. The manifest now requires a ^2
        // (excluding the preference) and b ^1. a MUST move to 2.x; b must
        // stay at 1.0.0 even though 1.5.0 exists — one forced change never
        // drags unrelated packages along.
        let mut src = source(vec![
            ("a", vec![("1.0.0", vec![]), ("2.3.0", vec![])]),
            ("b", vec![("1.0.0", vec![]), ("1.5.0", vec![])]),
        ]);
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.0.0".to_string());
        locked.insert("b".to_string(), "1.0.0".to_string());
        let rs = roots(&[("a", "^2"), ("b", "^1")]);
        let res = resolve_deps3(rs, &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "2.3.0");
        assert_eq!(version_of(&res, "b"), "1.0.0", "unrelated b must not be bumped");
    }

    #[test]
    fn widening_within_preference_keeps_locked() {
        // widening a to `*` still keeps the locked 1.0.0: the preference is
        // soft, and the range admits it — no churn without a reason.
        let mut src = source(vec![
            ("a", vec![("1.0.0", vec![]), ("2.3.0", vec![])]),
        ]);
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.0.0".to_string());
        let res = resolve_deps3(roots(&[("a", "*")]), &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "1.0.0");
    }

    #[test]
    fn yanked_versions_are_not_selected_fresh() {
        let mut src = source(vec![("a", vec![("1.0.0", vec![]), ("1.5.0", vec![])])]);
        src.flags.insert(
            "a".into(),
            HashMap::from([("1.5.0".to_string(), (true, None))]),
        );
        let res = resolve_deps3(roots(&[("a", "^1")]), &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "a"), "1.0.0");
    }

    #[test]
    fn yanked_but_locked_still_installs() {
        // a locked yanked version keeps resolving (preference wins)
        let mut src = source(vec![("a", vec![("1.0.0", vec![]), ("1.5.0", vec![])])]);
        src.flags.insert(
            "a".into(),
            HashMap::from([("1.5.0".to_string(), (true, None))]),
        );
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.5.0".to_string());
        let res = resolve_deps3(roots(&[("a", "^1")]), &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "1.5.0");
    }

    #[test]
    fn caret_semantics() {
        assert!(parse_range("^1.2.3").unwrap().contains(&SemanticVersion::new(1, 9, 9)));
        assert!(!parse_range("^1.2.3").unwrap().contains(&SemanticVersion::new(2, 0, 0)));
        assert!(parse_range("^0.2.3").unwrap().contains(&SemanticVersion::new(0, 2, 9)));
        assert!(!parse_range("^0.2.3").unwrap().contains(&SemanticVersion::new(0, 3, 0)));
        assert!(parse_range("~1.2.3").unwrap().contains(&SemanticVersion::new(1, 2, 9)));
        assert!(!parse_range("~1.2.3").unwrap().contains(&SemanticVersion::new(1, 3, 0)));
        assert!(parse_range("1.2.x").unwrap().contains(&SemanticVersion::new(1, 2, 7)));
        assert!(!parse_range("1.2.x").unwrap().contains(&SemanticVersion::new(1, 3, 0)));
        assert!(parse_range("*").unwrap().contains(&SemanticVersion::new(9, 9, 9)));
    }

    #[test]
    fn order_independence() {
        // resolution must not depend on the order requirements are listed
        let specs: Vec<Spec> = vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.0.0", vec![("util", "^2")])]),
            ("util", vec![("2.0.0", vec![]), ("2.9.0", vec![])]),
        ];
        let mut s1 = source(specs.clone());
        let mut s2 = source(specs);
        let r1 = resolve_deps3(roots(&[("lib", "^1"), ("app", "^1")]), &mut s1, &no_locked()).unwrap();
        let r2 = resolve_deps3(roots(&[("app", "^1"), ("lib", "^1")]), &mut s2, &no_locked()).unwrap();
        assert_eq!(version_of(&r1, "util"), version_of(&r2, "util"));
    }

    #[test]
    fn an_override_beats_a_transitive_requirement() {
        // app -> lib ^1 -> util ^2, but the manifest forces util ==2.1.0
        let mut src = source(vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.0.0", vec![("util", "^2")])]),
            ("util", vec![("2.0.0", vec![]), ("2.1.0", vec![]), ("2.9.0", vec![])]),
        ]);
        let rs = roots(&[("app", "^1")]);
        let ov = BTreeMap::from([("util".to_string(), "=2.1.0".to_string())]);
        let res = resolve_deps3_with(rs, ov, &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "util"), "2.1.0");
    }

    /// A bare version is a *caret* range everywhere in aoxn, overrides
    /// included: forcing `"2.1.0"` means the whole 2.1.x line, so 2.9.0
    /// still qualifies. Pinning exactly means writing `=2.1.0`.
    #[test]
    fn a_bare_override_version_is_a_caret_range() {
        let mut src = source(vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.0.0", vec![("util", "^2")])]),
            ("util", vec![("2.0.0", vec![]), ("2.1.0", vec![]), ("2.9.0", vec![])]),
        ]);
        let rs = roots(&[("app", "^1")]);
        let ov = BTreeMap::from([("util".to_string(), "2.1.0".to_string())]);
        let res = resolve_deps3_with(rs, ov, &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "util"), "2.9.0");
    }

    #[test]
    fn an_override_can_pin_across_majors() {
        // lib wants shared ^2, the manifest forces the 1.x line
        let mut src = source(vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.0.0", vec![("shared", "^2")])]),
            ("shared", vec![("1.5.0", vec![]), ("2.5.0", vec![])]),
        ]);
        let rs = roots(&[("app", "^1")]);
        let ov = BTreeMap::from([("shared".to_string(), "1.5.0".to_string())]);
        let res = resolve_deps3_with(rs, ov, &mut src, &no_locked()).unwrap();
        assert_eq!(version_of(&res, "shared"), "1.5.0");
    }

    #[test]
    fn an_override_must_be_satisfiable_or_it_fails_loudly() {
        // forcing a version that does not exist must be a resolve error, not
        // a silent fallback to whatever the graph asked for
        let mut src = source(vec![
            ("app", vec![("1.0.0", vec![("lib", "^1")])]),
            ("lib", vec![("1.0.0", vec![])]),
        ]);
        let rs = roots(&[("app", "^1")]);
        let ov = BTreeMap::from([("lib".to_string(), "9.9.9".to_string())]);
        let err = resolve_deps3_with(rs, ov, &mut src, &no_locked()).unwrap_err();
        assert!(matches!(err, PkgError::Resolve { .. }), "{err:?}");
    }

    #[test]
    fn an_override_with_a_bad_range_is_a_manifest_error() {
        let mut src = source(vec![("a", vec![("1.0.0", vec![])])]);
        let rs = roots(&[("a", "^1")]);
        let ov = BTreeMap::from([("a".to_string(), "not-a-range".to_string())]);
        let err = resolve_deps3_with(rs, ov, &mut src, &no_locked()).unwrap_err();
        assert!(matches!(err, PkgError::Manifest(_)), "{err:?}");
    }

    #[test]
    fn a_locked_version_survives_an_override_that_admits_it() {
        // the soft preference still wins when the override range allows the
        // locked version — no churn
        let mut src = source(vec![("a", vec![("1.0.0", vec![]), ("1.9.0", vec![])])]);
        let rs = roots(&[("a", "^1")]);
        let ov = BTreeMap::from([("a".to_string(), "^1".to_string())]);
        let mut locked = BTreeMap::new();
        locked.insert("a".to_string(), "1.0.0".to_string());
        let res = resolve_deps3_with(rs, ov, &mut src, &locked).unwrap();
        assert_eq!(version_of(&res, "a"), "1.0.0");
    }
}
