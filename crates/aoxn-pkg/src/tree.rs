//! `aoxn tree` and `aoxn why` — dependency graph inspection from the
//! lockfile.

use std::collections::{HashMap, HashSet};

use crate::context::Ctx;
use crate::errors::PkgError;
use crate::lockfile::Lockfile;

fn load_lock() -> Result<(Ctx, Lockfile), PkgError> {
    let ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(Lockfile::empty);
    Ok((ctx, lock))
}

/// Dependency edges: `name -> [(dep, locked version)]`.
fn edges(lock: &Lockfile) -> HashMap<String, Vec<(String, String)>> {
    let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for p in &lock.packages {
        let mut deps: Vec<(String, String)> = p
            .dependencies
            .iter()
            .filter_map(|d| {
                d.split_once('@')
                    .map(|(n, v)| (n.to_string(), v.to_string()))
            })
            .collect();
        deps.sort();
        out.insert(p.name.clone(), deps);
    }
    out
}

pub fn run_tree(depth: Option<usize>, json: bool) -> Result<i32, PkgError> {
    let (ctx, lock) = load_lock()?;
    let e = edges(&lock);

    if json {
        let mut obj = serde_json::Map::new();
        for p in &lock.packages {
            obj.insert(
                format!("{}@{}", p.name, p.version),
                serde_json::Value::Array(
                    e.get(&p.name)
                        .unwrap()
                        .iter()
                        .map(|(d, v)| serde_json::Value::String(format!("{d}@{v}")))
                        .collect(),
                ),
            );
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(obj))?);
        return Ok(0);
    }

    let ui = crate::ui::current();
    if lock.packages.is_empty() {
        ui.out("(no dependencies installed — aoxn.lock is empty)");
        return Ok(0);
    }

    // print from every manifest's direct deps; `*` marks nodes already shown
    let manifests = ctx.all_package_manifests()?;
    let mut printed: HashSet<String> = HashSet::new();
    for (dir, m) in &manifests {
        let label = if manifests.len() > 1 {
            format!("{} ({})", m.name, dir.display())
        } else {
            m.name.clone()
        };
        ui.out(&label);
        let mut roots: Vec<&String> = m.dependencies.keys().filter(|d| lock.get(d).is_some()).collect();
        roots.sort();
        for (i, r) in roots.iter().enumerate() {
            let lp = lock.get(r).unwrap();
            print_subtree(&e, &lp.name, &lp.version, "", i == roots.len() - 1, &mut printed, depth.unwrap_or(usize::MAX), 0);
        }
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn print_subtree(
    e: &HashMap<String, Vec<(String, String)>>,
    name: &str,
    version: &str,
    prefix: &str,
    last: bool,
    printed: &mut HashSet<String>,
    max_depth: usize,
    level: usize,
) {
    let branch = if prefix.is_empty() {
        String::new()
    } else if last {
        "└── ".to_string()
    } else {
        "├── ".to_string()
    };
    let seen = !printed.insert(name.to_string());
    let star = if seen { " *" } else { "" };
    crate::ui::current().out(&format!("{prefix}{branch}{name}@{version}{star}"));
    if seen {
        return; // don't explode shared subtrees twice
    }
    if level >= max_depth {
        return;
    }
    let children = e.get(name).cloned().unwrap_or_default();
    let child_prefix = format!("{prefix}{}", if prefix.is_empty() { "" } else if last { "    " } else { "│   " });
    for (i, (dep, depver)) in children.iter().enumerate() {
        print_subtree(e, dep, depver, &child_prefix, i == children.len() - 1, printed, max_depth, level + 1);
    }
}

pub fn run_why(pkg: &str, json: bool) -> Result<i32, PkgError> {
    let (ctx, lock) = load_lock()?;
    let e = edges(&lock);
    let Some(lp) = lock.get(pkg) else {
        return Err(PkgError::PackageNotFound(format!(
            "{pkg} is not in aoxn.lock (is it installed?)"
        )));
    };
    let target_version = lp.version.clone();

    let mut roots: Vec<String> = Vec::new();
    for (_, m) in ctx.all_package_manifests()? {
        for d in m.dependencies.keys() {
            if lock.get(d).is_some() && !roots.contains(d) {
                roots.push(d.clone());
            }
        }
    }
    roots.sort();

    // all simple paths root -> pkg
    let mut paths: Vec<Vec<String>> = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    for r in &roots {
        dfs(&e, r, pkg, &mut path, &mut visited, &mut paths);
    }

    if paths.is_empty() {
        crate::ui::current()
            .out(&format!("{pkg}@{target_version} is locked but not reachable from any manifest"));
        return Ok(0);
    }
    if json {
        let arr: Vec<_> = paths
            .iter()
            .map(|p| {
                serde_json::Value::Array(
                    p.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                )
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr))?);
        return Ok(0);
    }
    let ui = crate::ui::current();
    ui.out(&format!(
        "{pkg}@{target_version}: {} requirement path{}",
        paths.len(),
        if paths.len() == 1 { "" } else { "s" }
    ));
    for p in &paths {
        ui.out(&format!("  {}", p.join(" -> ")));
    }
    Ok(0)
}

fn dfs(
    e: &HashMap<String, Vec<(String, String)>>,
    cur: &str,
    target: &str,
    path: &mut Vec<String>,
    visited: &mut HashSet<String>,
    out: &mut Vec<Vec<String>>,
) {
    if !visited.insert(cur.to_string()) {
        return;
    }
    path.push(cur.to_string());
    if cur == target {
        out.push(path.clone());
    } else if let Some(deps) = e.get(cur) {
        for (d, _) in deps {
            dfs(e, d, target, path, visited, out);
        }
    }
    path.pop();
    visited.remove(cur);
}
