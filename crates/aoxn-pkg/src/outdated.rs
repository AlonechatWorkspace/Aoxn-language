//! `aoxn outdated` — locked version vs newest available in the registry.

use std::collections::BTreeMap;

use crate::context::Ctx;
use crate::lockfile::Lockfile;
use crate::errors::PkgError;

pub fn run(offline: bool, json: bool) -> Result<i32, PkgError> {
    let mut ctx = Ctx::discover(&std::env::current_dir()?, offline)?;
    let lock = ctx.load_lockfile()?.unwrap_or_else(Lockfile::empty);

    // collect every manifest dependency with its registry
    let mut wanted: BTreeMap<String, Option<String>> = BTreeMap::new(); // name -> registry hint
    for (_, m) in ctx.all_package_manifests()? {
        for (name, spec) in &m.dependencies {
            if let crate::manifest::DependencySpec::Registry { registry, .. } = spec {
                wanted.entry(name.clone()).or_insert(registry.clone());
            }
        }
    }
    if wanted.is_empty() {
        let ui = crate::ui::current();
        ui.out("no registry dependencies declared");
        return Ok(0);
    }

    struct Row {
        name: String,
        locked: String,
        newest: String,
        note: String,
    }
    let mut rows: Vec<Row> = Vec::new();
    for (name, hint) in &wanted {
        let url = ctx.registry_url(hint.as_deref())?;
        let idx = {
            let reg = ctx.registry_for(&url)?;
            reg.index(name)?
        };
        let locked = lock.get(name).map(|p| p.version.clone()).unwrap_or_else(|| "-".into());
        let (newest, note) = match idx.latest(false) {
            Some((v, entry)) => {
                let mut note = String::new();
                if let Some(msg) = &entry.deprecated {
                    note.push_str(&format!(" deprecated: {msg}"));
                }
                (v, note)
            }
            None => ("(none)".into(), String::new()),
        };
        rows.push(Row {
            name: name.clone(),
            locked,
            newest,
            note,
        });
    }

    if json {
        let arr: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "locked": r.locked,
                    "newest": r.newest,
                    "note": r.note,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr))?);
        return Ok(0);
    }

    let ui = crate::ui::current();
    let w_name = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let w_locked = rows.iter().map(|r| r.locked.len()).max().unwrap_or(6).max(6);
    let w_new = rows.iter().map(|r| r.newest.len()).max().unwrap_or(6).max(6);
    ui.out(&format!(
        "{:<w_name$}  {:<w_locked$}  {:<w_new$}  note",
        "package", "locked", "newest",
    ));
    for r in &rows {
        let up_to_date = r.locked == r.newest;
        let line = format!(
            "{:<w_name$}  {:<w_locked$}  {:<w_new$}  {}",
            r.name, r.locked, r.newest, r.note
        );
        if up_to_date {
            ui.out(&line);
        } else {
            ui.out(&format!("{}  {}", line, if r.locked == "-" { "" } else { "OUTDATED" }));
        }
    }
    if offline {
        ui.info("(offline mode: newest versions come from the cached registry snapshot)");
    }
    Ok(0)
}
