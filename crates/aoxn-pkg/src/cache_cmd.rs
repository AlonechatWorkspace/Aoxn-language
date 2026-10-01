//! `aoxn cache` — global cache management.

use crate::cli::CacheCmd;
use crate::errors::PkgError;

pub fn run(sub: CacheCmd) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let cache = crate::cache::Cache::global();
    match sub {
        CacheCmd::Dir => {
            ui.out(&cache.root.to_string_lossy());
            Ok(0)
        }
        CacheCmd::Size => {
            let bytes = cache.size_on_disk();
            ui.out(&format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0)));
            Ok(0)
        }
        CacheCmd::Gc => {
            // keep tarballs referenced by any aoxn.lock under the cwd tree;
            // everything else is only deleted after a TTL, so a concurrent
            // install that resolved but has not fetched yet never loses its
            // tarball out from under it. A lock file serializes gc runs.
            const TTL_DAYS: u64 = 14;
            let _lock = cache_lock(&cache)?;
            let keep = collect_referenced_checksums(&std::env::current_dir()?, 6);
            let deleted = cache.gc_old(&keep, TTL_DAYS)?;
            ui.success(&format!(
                "removed {deleted} tarball(s) unreferenced for over {TTL_DAYS} days"
            ));
            Ok(0)
        }
        CacheCmd::Prune => {
            let removed = cache.prune_registries()?;
            ui.success(&format!("dropped {removed} registry snapshot(s) (re-fetched on demand)"));
            Ok(0)
        }
        CacheCmd::Clean => {
            cache.clean()?;
            ui.success("cache deleted");
            Ok(0)
        }
    }
}

/// Advisory exclusive lock over gc runs (`gc.lock` in the cache root), so
/// two gc processes don't interleave deletes. Stale locks older than an
/// hour are broken.
fn cache_lock(cache: &crate::cache::Cache) -> Result<LockGuard, PkgError> {
    let path = cache.root.join("gc.lock");
    cache.create_dirs()?;
    if let Ok(meta) = std::fs::metadata(&path) {
        if let Ok(age) = meta.modified().map(|m| m.elapsed().unwrap_or_default()) {
            if age > std::time::Duration::from_secs(3600) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    for attempt in 0..3 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(LockGuard(path.clone())),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1)));
            }
            Err(e) => return Err(PkgError::Io(e)),
        }
    }
    Err(PkgError::other(
        "another `aoxn cache gc` appears to be running (gc.lock present); retry later",
    ))
}

struct LockGuard(std::path::PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Scan `root` (bounded depth) for aoxn.lock files and collect all referenced
/// tarball checksums.
fn collect_referenced_checksums(root: &std::path::Path, max_depth: u32) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    fn walk(dir: &std::path::Path, depth: u32, out: &mut std::collections::HashSet<String>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "target" && name != ".git" && name != "aox_modules" {
                    walk(&p, depth - 1, out);
                }
            } else if name == "aoxn.lock" {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    if let Ok(lf) = serde_json::from_str::<crate::lockfile::Lockfile>(&text) {
                        for pkg in lf.packages {
                            out.insert(pkg.checksum);
                        }
                    }
                }
            }
        }
    }
    walk(root, max_depth, &mut out);
    out
}
