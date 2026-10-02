//! The Aoxn IDE — Tauri backend.
//!
//! The command surface is deliberately small and deliberately narrow. Every
//! filesystem path that arrives from the webview goes through
//! [`fsops::Workspace::resolve`] first, so the frontend cannot reach outside
//! the folder the user opened; there is no general "read any path" command,
//! and no shell plugin (see `capabilities/default.json`).
//!
//! The commands fall into four groups:
//!
//! * **workspace** — `ide_open_folder`, `ide_scan`, `ide_rescan`
//! * **files**     — `ide_read`, `ide_save`
//! * **toolchain** — `ide_toolchain`
//! * **run**       — `ide_check`, `ide_build`, `ide_run`
//!
//! Nothing here parses compiler output. The log comes back verbatim and the
//! frontend parses it, which keeps the diagnostic format in one place and
//! means a new stage in the compiler shows up as a frontend change instead
//! of a coordinated rebuild.

mod fsops;
mod model;
mod toolchain;

use fsops::Workspace;
use model::{ExecResult, FileContents, ToolchainInfo, TreeNode};
use std::sync::Mutex;
use tauri::{Manager, State};

/// The open folder, if any. `None` until the user picks one.
#[derive(Default)]
struct AppState(Mutex<Option<Workspace>>);

fn workspace(state: &State<AppState>) -> Result<Workspace, String> {
    state
        .0
        .lock()
        .map_err(|_| "workspace lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "no folder is open".to_string())
}

fn set_workspace(state: &State<AppState>, ws: Workspace) -> Result<(), String> {
    let mut guard = state
        .0
        .lock()
        .map_err(|_| "workspace lock poisoned".to_string())?;
    *guard = Some(ws);
    Ok(())
}

// ---- workspace ----

/// Open a folder and make it the boundary for every later command.
#[tauri::command]
fn ide_open_folder(root: String, state: State<AppState>) -> Result<Vec<TreeNode>, String> {
    let ws = Workspace::open(&root)?;
    let tree = fsops::scan(&ws, 8);
    set_workspace(&state, ws)?;
    Ok(tree)
}

/// Re-walk the open folder (after a build wrote `target/`, or the user
/// created a file outside the IDE).
#[tauri::command]
fn ide_scan(state: State<AppState>) -> Result<Vec<TreeNode>, String> {
    let ws = workspace(&state)?;
    Ok(fsops::scan(&ws, 8))
}

// ---- files ----

#[tauri::command]
fn ide_read(path: String, state: State<AppState>) -> Result<FileContents, String> {
    fsops::read(&workspace(&state)?, &path)
}

#[tauri::command]
fn ide_save(path: String, text: String, state: State<AppState>) -> Result<(), String> {
    fsops::write(&workspace(&state)?, &path, &text)
}

// ---- toolchain ----

#[tauri::command]
fn ide_toolchain() -> ToolchainInfo {
    toolchain::probe()
}

// ---- run ----

/// Build, check and run all reduce to "run the compiler with these
/// arguments"; keeping them separate gives the UI honest button labels and
/// a place to put a future per-kind flag set.
#[tauri::command]
fn ide_check(path: String, state: State<AppState>) -> Result<ExecResult, String> {
    let ws = workspace(&state)?;
    let path = ws.resolve(&path)?.to_string_lossy().into_owned();
    toolchain::run(&toolchain::compiler_command(), &toolchain::check_args(&path), None)
}

#[tauri::command]
fn ide_build(path: String, state: State<AppState>) -> Result<ExecResult, String> {
    let ws = workspace(&state)?;
    let path = ws.resolve(&path)?.to_string_lossy().into_owned();
    let out = toolchain::artifact_path(&path);
    toolchain::run(
        &toolchain::compiler_command(),
        &toolchain::build_args(&path, &out),
        None,
    )
}

#[tauri::command]
fn ide_run(path: String, state: State<AppState>) -> Result<ExecResult, String> {
    let ws = workspace(&state)?;
    let path = ws.resolve(&path)?.to_string_lossy().into_owned();
    toolchain::run(
        &toolchain::compiler_command(),
        &toolchain::run_args(&path),
        None,
    )
}

/// Build and then execute the artifact. Separate from `ide_run` because
/// `aoxn run` recompiles through the cache, while "run what I just built"
/// must not silently recompile a source the user has since changed.
#[tauri::command]
fn ide_build_and_run(path: String, state: State<AppState>) -> Result<ExecResult, String> {
    let ws = workspace(&state)?;
    let path = ws.resolve(&path)?.to_string_lossy().into_owned();
    let out = toolchain::artifact_path(&path);
    let built = toolchain::run(
        &toolchain::compiler_command(),
        &toolchain::build_args(&path, &out),
        None,
    )?;
    if built.code != 0 {
        return Ok(built);
    }
    let exe = toolchain::run(&out, &[], None)?;
    Ok(ExecResult {
        code: exe.code,
        output: format!("{}{}", built.output, exe.output),
        duration_ms: built.duration_ms + exe.duration_ms,
    })
}

// ---- app ----

/// Best guess at the folder to open on launch.
///
/// Explicit beats implicit: `AOXN_IDE_ROOT`, then a folder named on the
/// command line, then the current directory. The IDE opens with a tree, not
/// with an empty pane, because an empty pane with nothing to click is the
/// worst first impression an editor can make.
fn initial_root() -> String {
    if let Ok(v) = std::env::var("AOXN_IDE_ROOT") {
        if !v.is_empty() {
            return v;
        }
    }
    // A folder named on the command line. Only an argument that actually
    // IS an existing directory is taken, so a stray flag cannot be mistaken
    // for a path.
    for arg in std::env::args().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        if std::path::Path::new(&arg).is_dir() {
            return arg;
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            ide_open_folder,
            ide_scan,
            ide_read,
            ide_save,
            ide_toolchain,
            ide_check,
            ide_build,
            ide_run,
            ide_build_and_run,
        ])
        .setup(|app| {
            let root = initial_root();
            let state = app.state::<AppState>();
            match Workspace::open(&root) {
                Ok(ws) => {
                    let _ = set_workspace(&state, ws);
                }
                Err(e) => {
                    // Not fatal: the workbench opens with no folder and the
                    // user picks one. A wrong working directory should not
                    // stop the IDE from starting.
                    eprintln!("aoxn-ide: {e}");
                }
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title("Aoxn IDE");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Aoxn IDE");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_root_prefers_the_env_var() {
        let dir = tempfile::tempdir().unwrap();
        let want = dir.path().to_string_lossy().into_owned();
        // SAFETY: no other thread exists while this test body runs.
        unsafe { std::env::set_var("AOXN_IDE_ROOT", &want) };
        assert_eq!(initial_root(), want);
        unsafe { std::env::remove_var("AOXN_IDE_ROOT") };
    }

    #[test]
    fn initial_root_falls_back_to_the_working_directory() {
        unsafe { std::env::remove_var("AOXN_IDE_ROOT") };
        let root = initial_root();
        assert!(
            std::path::Path::new(&root).is_dir(),
            "fell back to '{root}', which is not a directory"
        );
    }
}