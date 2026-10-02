//! `aoxn list` / `aoxn freeze` — what is actually installed, in human and
//! machine form.
//!
//! `list` is the table view (`tree` shows shape, `list` shows inventory);
//! `freeze` is pip's `pip freeze`: one `name==version` per line, sorted, no
//! prose — the thing you paste into a CI baseline or diff against the
//! previous release.

use crate::context::{Ctx, DepScope};
use crate::errors::PkgError;

/// One row of `aoxn list`.
struct Row {
    name: String,
    version: String,
    scope: &'static str,
    source: String,
}

fn rows(lock: &crate::lockfile::Lockfile, scope: DepScope) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for p in &lock.packages {
        let dev = p.dev;
        if scope == DepScope::Prod && dev {
            continue;
        }
        if scope == DepScope::DevOnly && !dev {
            continue;
        }
        out.push(Row {
            name: p.name.clone(),
            version: p.version.clone(),
            scope: if dev { "dev" } else { "prod" },
            source: if p.registry.starts_with("path+") {
                "local".to_string()
            } else {
                short_registry(&p.registry)
            },
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    out
}

/// Enough of a registry URL to identify it in a table cell.
fn short_registry(url: &str) -> String {
    if let Some(rest) = url.split("://").nth(1) {
        let host_path = rest.splitn(2, '/').next().unwrap_or(rest);
        let host = host_path.split(':').next().unwrap_or(host_path);
        return host.to_string();
    }
    url.to_string()
}

pub fn run(scope: DepScope, json: bool) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(crate::lockfile::Lockfile::empty);
    let rows = rows(&lock, scope);

    if json {
        ui.out_json(&serde_json::json!({
            "packages": rows
                .iter()
                .map(|r| serde_json::json!({
                    "name": r.name,
                    "version": r.version,
                    "scope": r.scope,
                    "source": r.source,
                }))
                .collect::<Vec<_>>(),
        }));
        return Ok(0);
    }

    if rows.is_empty() {
        ui.out("nothing installed yet (`aoxn install`)");
        return Ok(0);
    }
    // widths must fit the header as well as the data, or the header pushes
    // every column right of it out of alignment
    const HEADERS: [&str; 4] = ["Package", "Version", "Scope", "Source"];
    let widths = |f: fn(&Row) -> usize| -> usize {
        rows.iter().map(f).max().unwrap_or(0).max(0)
    };
    let w_name = widths(|r| r.name.len()).max(HEADERS[0].len());
    let w_ver = widths(|r| r.version.len()).max(HEADERS[1].len());
    let w_scope = widths(|r| r.scope.len()).max(HEADERS[2].len());
    let w_src = widths(|r| r.source.len()).max(HEADERS[3].len());
    ui.out(&format!(
        "{:w_name$}  {:w_ver$}  {:w_scope$}  {:w_src$}",
        HEADERS[0], HEADERS[1], HEADERS[2], HEADERS[3]
    ));
    for r in &rows {
        ui.out(&format!(
            "{:w_name$}  {:w_ver$}  {:w_scope$}  {:w_src$}",
            r.name, r.version, r.scope, r.source
        ));
    }
    Ok(0)
}

/// pip-freeze equivalent: `name==version`, sorted, one per line.
pub fn freeze(scope: DepScope) -> Result<i32, PkgError> {
    let ui = crate::ui::current();
    let ctx = Ctx::discover(&std::env::current_dir()?, false)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(crate::lockfile::Lockfile::empty);
    for r in rows(&lock, scope) {
        ui.out(&format!("{}=={}", r.name, r.version));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{Lockfile, LockedPackage};

    fn lock() -> Lockfile {
        let mut lf = Lockfile::empty();
        for (name, version, dev, registry) in [
            ("zlib", "1.2.0", false, "https://github.com/AlonechatWorkspace/r"),
            ("http", "2.1.0", false, "https://github.com/AlonechatWorkspace/r"),
            ("harness", "2.0.0", true, "https://github.com/AlonechatWorkspace/r"),
            ("local-lib", "0.1.0", false, "path+C:/src/local-lib"),
        ] {
            lf.insert(LockedPackage {
                name: name.into(),
                version: version.into(),
                registry: registry.into(),
                checksum: "c".into(),
                tarball_sha256: None,
                dev,
                dependencies: vec![],
            });
        }
        lf
    }

    fn freeze_lines(scope: DepScope) -> Vec<String> {
        rows(&lock(), scope)
            .into_iter()
            .map(|r| format!("{}=={}", r.name, r.version))
            .collect()
    }

    #[test]
    fn freeze_is_sorted_and_stable() {
        assert_eq!(
            freeze_lines(DepScope::All),
            vec!["harness==2.0.0", "http==2.1.0", "local-lib==0.1.0", "zlib==1.2.0"]
        );
        // deterministic across calls
        assert_eq!(freeze_lines(DepScope::All), freeze_lines(DepScope::All));
    }

    #[test]
    fn freeze_can_be_restricted_to_production_packages() {
        let prod = freeze_lines(DepScope::Prod);
        assert!(!prod.iter().any(|l| l.starts_with("harness")), "{prod:?}");
        assert!(prod.contains(&"http==2.1.0".to_string()));
    }

    #[test]
    fn the_dev_only_slice_holds_only_dev_packages() {
        assert_eq!(freeze_lines(DepScope::DevOnly), vec!["harness==2.0.0"]);
    }

    #[test]
    fn local_dependencies_are_marked_not_shown_as_a_url() {
        let all = rows(&lock(), DepScope::All);
        let local = all.iter().find(|r| r.name == "local-lib").unwrap();
        assert_eq!(local.source, "local");
        assert_eq!(local.scope, "prod");
    }

    #[test]
    fn a_registry_url_is_reduced_to_its_host() {
        assert_eq!(
            short_registry("https://github.com/AlonechatWorkspace/r"),
            "github.com"
        );
        assert_eq!(short_registry("http://127.0.0.1:8080/aoxn"), "127.0.0.1");
        assert_eq!(short_registry("C:/src/reg"), "C:/src/reg");
    }
}
