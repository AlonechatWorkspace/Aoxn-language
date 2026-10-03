//! Package-manager integration for the workbench.
//!
//! Two rules keep this honest and safe:
//!
//! 1. **Drive the real `aoxn pkg`.** Like Check/Build/Run, the IDE does not
//!    reimplement resolution, installation or auditing — it shells out to
//!    the same binary with the arguments a user would type, from the
//!    workspace root, and shows the output verbatim in the output panel.
//! 2. **A whitelist, not free-form.** The webview names a SUBCOMMAND, never
//!    a command line, and the list below is the whole surface: inspection
//!    (`list`/`tree`/`why`/`outdated`/`audit`/`freeze`) plus the local,
//!    project-scoped mutations (`init`/`install`/`update`/`add`/`remove`/
//!    `uninstall`). Outward-facing or irreversible commands (`publish`,
//!    `yank`), global state (`cache`, `trust bootstrap`) and the npm
//!    bridge are deliberately not offered — a compromised webview can at
//!    worst run `aoxn pkg install` inside the folder the user opened,
//!    which is the same action the user could have typed themselves.

use crate::fsops::Workspace;
use crate::model::{ExecResult, InstalledPkg, OutdatedPkg, PkgDep, PkgManifestInfo, PkgReport};
use crate::toolchain;

/// Subcommands the workbench may run. Everything else — publish, yank,
/// trust, cache, npm-import, build — is refused at the gate.
pub const ALLOWED_SUBCOMMANDS: &[&str] = &[
    "init", "install", "update", "add", "remove", "uninstall", //
    "list", "freeze", "tree", "outdated", "why", "audit",
];

/// Subcommands that change the manifest, the lockfile or `aox_modules/` —
/// after one of these the packages panel and the explorer re-read.
pub const MUTATING_SUBCOMMANDS: &[&str] =
    &["init", "install", "update", "add", "remove", "uninstall"];

pub fn subcommand_allowed(sub: &str) -> bool {
    ALLOWED_SUBCOMMANDS.contains(&sub)
}

pub fn subcommand_is_mutating(sub: &str) -> bool {
    MUTATING_SUBCOMMANDS.contains(&sub)
}

/// Validate a package name (or `name@req`) typed into the workbench.
///
/// `Command::new` with an argument array involves no shell, so this is not
/// an injection defence — it is what keeps the panel's error messages
/// honest and stops flag-shaped strings (`-x`), paths and separators from
/// reaching clap, where they would mean something else.
pub fn validate_pkg_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("Type a package name.".to_string());
    }
    if n.len() > 100 {
        return Err("That name is too long.".to_string());
    }
    if n.starts_with('-') {
        return Err("A package name cannot start with '-'.".to_string());
    }
    if n.chars().any(char::is_whitespace) {
        return Err("A package name cannot contain spaces.".to_string());
    }
    if n.contains('/') || n.contains('\\') {
        return Err("A package name is not a path.".to_string());
    }
    // `name@^1.2` is the shape `aoxn pkg add` documents, so the requirement
    // characters ride along; everything exotic is refused.
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '^' | '~' | '*'))
    {
        return Err(
            "Only letters, digits and - _ . @ ^ ~ * are allowed in a package name.".to_string(),
        );
    }
    Ok(())
}

/// Run `aoxn pkg <sub> [args…]` from the workspace root.
pub fn pkg_command(ws: &Workspace, sub: &str, args: &[String]) -> Result<ExecResult, String> {
    if !subcommand_allowed(sub) {
        return Err(format!(
            "'pkg {sub}' is not offered in the IDE (allowed: {})",
            ALLOWED_SUBCOMMANDS.join(", ")
        ));
    }
    for a in args {
        validate_pkg_name(a)?;
    }
    let mut full = vec!["pkg".to_string(), sub.to_string()];
    full.extend_from_slice(args);
    toolchain::run(
        &toolchain::compiler_command(),
        &full,
        Some(&ws.root().to_string_lossy()),
    )
}

/// Read the workspace manifest, tolerantly.
///
/// No `aoxn.json` is a normal state (the panel offers Init); broken JSON is
/// reported through `parse_error` instead of failing the command.
pub fn read_manifest(ws: &Workspace) -> PkgManifestInfo {
    let mut info = PkgManifestInfo {
        has_lockfile: ws.root().join("aoxn.lock").is_file(),
        installed: installed(ws),
        ..Default::default()
    };
    let text = match std::fs::read_to_string(ws.root().join("aoxn.json")) {
        Ok(t) => t,
        Err(_) => return info,
    };
    info.has_manifest = true;
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            info.parse_error = Some(format!("aoxn.json is not valid JSON: {e}"));
            return info;
        }
    };
    info.name = str_field(&v, "name");
    info.version = str_field(&v, "version");
    for (dev, key) in [(false, "dependencies"), (true, "devDependencies")] {
        if let Some(deps) = v.get(key).and_then(|d| d.as_object()) {
            for (name, req) in deps {
                let req = match req {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                info.dependencies.push(PkgDep {
                    name: name.clone(),
                    req,
                    dev,
                });
            }
        }
    }
    info
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Names of the packages unpacked into `aox_modules/` — what the lockfile
/// promised AND what the resolver actually delivered, in one glance.
pub fn installed(ws: &Workspace) -> Vec<String> {
    let entries = match std::fs::read_dir(ws.root().join("aox_modules")) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by_key(|a| a.to_lowercase());
    names
}

/// Ask the package manager what is actually installed and what is behind.
///
/// Both halves come from the tool's own `--json` reports — `aoxn list
/// --json` for the resolved inventory, `aoxn outdated --json` for what could
/// be newer. The panel therefore cannot disagree with the package manager
/// about what the project has: it is reading the same answer, not
/// re-deriving one from the manifest.
///
/// Neither report is allowed to be fatal. A folder with no lockfile yet, a
/// registry that does not answer, a compiler that predates `--json` — each
/// costs the panel ONE section and leaves the other intact, and says which.
/// A panel that showed nothing at all would be indistinguishable from a
/// project with no dependencies.
pub fn read_report(ws: &Workspace) -> PkgReport {
    let mut report = PkgReport::default();

    match json_report(ws, "list") {
        Ok(rows) => report.installed = parse_installed(&rows),
        Err(e) => report.error = Some(format!("list: {e}")),
    }

    match json_report(ws, "outdated") {
        Ok(rows) => report.outdated = parse_outdated(&rows),
        // An outdated failure is usually a registry that could not be
        // reached — offline, or a mirror that is down. That must NOT read
        // as "everything is up to date", so it is reported and the panel
        // says the check could not run.
        Err(e) => {
            let msg = format!("outdated: {e}");
            report.error = Some(match report.error.take() {
                Some(prev) => format!("{prev}; {msg}"),
                None => msg,
            });
        }
    }

    report
}

/// Run `aoxn pkg <sub> --json` and return the parsed document's items.
///
/// Returns the raw JSON text; the caller maps it, because `list` wraps its
/// rows in an object (`{"packages":[…]}`) while `outdated` emits a bare
/// array, and pretending they share a shape would mean one of them is
/// silently wrong.
fn json_report(ws: &Workspace, sub: &str) -> Result<String, String> {
    let args = vec!["pkg".to_string(), sub.to_string(), "--json".to_string()];
    let result = toolchain::run(
        &toolchain::compiler_command(),
        &args,
        Some(&ws.root().to_string_lossy()),
    )?;
    if result.code != 0 {
        // The tool's own message is more useful than an exit code.
        let msg = result.output.trim();
        return Err(if msg.is_empty() {
            format!("`aoxn pkg {sub}` exited {}", result.code)
        } else {
            msg.lines().last().unwrap_or(msg).to_string()
        });
    }
    toolchain::json_document(&result.output)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("`aoxn pkg {sub}` printed no JSON report"))
}

/// Parse `aoxn list --json`'s `{"packages":[…]}` into rows.
pub fn parse_installed(json: &str) -> Vec<InstalledPkg> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(items) = v.get("packages").and_then(|p| p.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .map(|e| InstalledPkg {
            name: s(e, "name"),
            version: s(e, "version"),
            scope: s(e, "scope"),
            source: s(e, "source"),
        })
        .filter(|p| !p.name.is_empty())
        .collect()
}

/// Parse `aoxn outdated --json`'s bare `[…]` array into rows.
pub fn parse_outdated(json: &str) -> Vec<OutdatedPkg> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(items) = v.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|e| OutdatedPkg {
            name: s(e, "name"),
            locked: s(e, "locked"),
            newest: s(e, "newest"),
            note: s(e, "note"),
        })
        .filter(|p| !p.name.is_empty())
        .collect()
}

fn s(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_ws(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().expect("tempdir");
        let proj = dir.path().join("proj");
        fs::create_dir_all(&proj).unwrap();
        for (path, text) in files {
            let real = proj.join(path);
            if let Some(parent) = real.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(real, text).unwrap();
        }
        let ws = Workspace::open(proj.to_str().unwrap()).expect("open workspace");
        (dir, ws)
    }

    #[test]
    fn only_whitelisted_subcommands_are_offered() {
        for ok in ["init", "install", "update", "add", "remove", "uninstall", "list", "freeze", "tree", "outdated", "why", "audit"] {
            assert!(subcommand_allowed(ok), "{ok} should be allowed");
        }
        for banned in ["publish", "yank", "trust", "cache", "npm-import", "build", "", "run"] {
            assert!(!subcommand_allowed(banned), "{banned:?} must be refused");
        }
        let (_d, ws) = tmp_ws(&[]);
        let err = pkg_command(&ws, "publish", &[]).unwrap_err();
        assert!(err.contains("not offered"), "unexpected error: {err}");
    }

    #[test]
    fn mutations_are_flagged_for_refresh() {
        assert!(subcommand_is_mutating("install"));
        assert!(subcommand_is_mutating("add"));
        assert!(!subcommand_is_mutating("tree"));
        assert!(!subcommand_is_mutating("audit"));
    }

    #[test]
    fn package_names_are_validated() {
        for ok in ["http", "json", "http@^2", "my-pkg_2.ax", "a@~1.0"] {
            assert!(validate_pkg_name(ok).is_ok(), "{ok} should be accepted");
        }
        let long = "x".repeat(101);
        for bad in ["", "   ", "-x", "a b", "../evil", "a/b", "a\\b", "a;b", "$PATH", long.as_str()] {
            assert!(validate_pkg_name(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn manifest_is_read_tolerantly() {
        let manifest = r#"{
            "name": "demo",
            "version": "0.1.0",
            "dependencies": { "http": "^2" },
            "devDependencies": { "testkit": "*" }
        }"#;
        let (_d, ws) = tmp_ws(&[
            ("aoxn.json", manifest),
            ("aoxn.lock", "{}"),
            ("aox_modules/http/aoxn.json", "{}"),
            ("aox_modules/afile.txt", "not a dir"),
        ]);
        let m = read_manifest(&ws);
        assert!(m.has_manifest);
        assert_eq!(m.name, "demo");
        assert_eq!(m.version, "0.1.0");
        assert!(m.has_lockfile);
        assert_eq!(m.dependencies.len(), 2);
        assert!(m.dependencies.iter().any(|d| d.dev && d.name == "testkit"));
        assert!(m.dependencies.iter().all(|d| !d.dev || d.name != "http"));
        assert_eq!(m.installed, vec!["http".to_string()]);
    }

    #[test]
    fn a_workspace_without_a_manifest_is_a_normal_state() {
        let (_d, ws) = tmp_ws(&[]);
        let m = read_manifest(&ws);
        assert!(!m.has_manifest);
        assert!(m.parse_error.is_none());
        assert!(m.dependencies.is_empty());
        assert!(m.installed.is_empty());
    }

    #[test]
    fn a_broken_manifest_reports_a_parse_error_instead_of_failing() {
        let (_d, ws) = tmp_ws(&[("aoxn.json", "{ not json")]);
        let m = read_manifest(&ws);
        assert!(m.has_manifest);
        assert!(m.parse_error.is_some());
    }

    #[test]
    fn the_list_report_becomes_installed_rows() {
        // The whole point of the panel: the manifest says `^2`, the
        // lockfile says what actually got installed.
        let json = r#"{"packages":[
            {"name":"http","version":"2.1.0","scope":"prod","source":"registry.local"},
            {"name":"axtest","version":"0.3.0","scope":"dev","source":"local"}
        ]}"#;
        let rows = parse_installed(json);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "http");
        assert_eq!(rows[0].version, "2.1.0");
        assert_eq!(rows[0].scope, "prod");
        assert_eq!(rows[1].scope, "dev");
        assert_eq!(rows[1].source, "local");
    }

    #[test]
    fn the_outdated_report_becomes_rows_from_a_bare_array() {
        // `aoxn outdated --json` emits a top-level ARRAY, not an object —
        // a parser written for `list`'s shape would find nothing here.
        let json = r#"[
            {"name":"http","locked":"2.1.0","newest":"2.4.0","note":""},
            {"name":"old","locked":"1.0.0","newest":"1.1.0","note":"pre-release"}
        ]"#;
        let rows = parse_outdated(json);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].newest, "2.4.0");
        assert_eq!(rows[1].note, "pre-release");
    }

    #[test]
    fn a_malformed_or_unexpected_report_yields_no_rows_not_a_panic() {
        // The panel must survive a compiler that prints something else, a
        // truncated report, or an object where an array was expected.
        for bad in [
            "",
            "{ not json",
            r#"{"packages":"not an array"}"#,
            r#"{"unrelated":true}"#,
            r#"[{"name":""}]"#,
        ] {
            assert!(parse_installed(bad).is_empty(), "installed: {bad:?}");
            assert!(parse_outdated(bad).is_empty(), "outdated: {bad:?}");
        }
    }

    #[test]
    fn a_row_without_a_name_is_dropped_rather_than_shown_blank() {
        let rows = parse_installed(r#"{"packages":[{"version":"1.0.0"},{"name":"ok"}]}"#);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "ok");
    }

    #[test]
    fn the_report_readers_use_only_whitelisted_readonly_subcommands() {
        // `read_report` shells out; what it may shell out to is the whole
        // security story, so the names are pinned here rather than trusted
        // to the caller.
        assert!(subcommand_allowed("list"));
        assert!(subcommand_allowed("outdated"));
        assert!(!subcommand_is_mutating("list"));
        assert!(!subcommand_is_mutating("outdated"));
    }
}
