//! Filesystem access for the workbench: the project tree, and reading and
//! writing one file.
//!
//! The security posture is the interesting part. An editor is a program the
//! user points at a folder, and "the frontend asked for a path" is not a
//! reason to read it: the frontend is a webview, and a webview should not
//! be able to turn a rendering bug into `read_file("C:/Users/you/.ssh/id_rsa")`.
//! So every path crosses a [`Workspace`] first, which canonicalises it and
//! refuses anything that resolves outside the folder the user opened. The
//! rules are in [`Workspace::resolve`] and are covered by the tests at the
//! bottom of this file.

use crate::model::{FileContents, TreeNode};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Directories that are never worth showing in a file tree: they are either
/// build output, dependency trees, or VCS internals. Listing `node_modules`
/// is what makes a naive tree useless — it is thousands of files that are
/// never edited by hand.
const SKIP_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "aox_modules",
    ".next",
    "__pycache__",
    ".venv",
    "dist",
    "obj",
    ".idea",
    ".vscode",
];

/// Ceiling on the tree so a pathological root (a home directory, a drive
/// root) cannot lock the UI up. When it is hit, the returned tree is
/// truncated but still valid — the UI reports the truncation rather than
/// pretending it saw everything.
const MAX_NODES: usize = 8000;

/// The folder the user opened, and the boundary every path must stay inside.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: &str) -> Result<Self, String> {
        let path = fs::canonicalize(root).map_err(|e| format!("cannot open '{root}': {e}"))?;
        if !path.is_dir() {
            return Err(format!("'{root}' is not a folder"));
        }
        Ok(Self { root: path })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Map a frontend path to a real one, or refuse.
    ///
    /// Three separate things have to hold, and each catches a different bug:
    ///
    /// 1. `canonicalize` collapses `..`, symlinks and duplicated separators,
    ///    so the comparison below is on the resolved path, not the text.
    /// 2. The resolved path must start with the root, compared COMPONENT by
    ///    component. Comparing strings would let `C:\proj-evil` pass a
    ///    `starts_with("C:\proj")` test — the classic sibling-directory
    ///    escape.
    /// 3. A path that does not exist yet still has to be checked (a Save As
    ///    target), so its PARENT is canonicalised and bounded instead.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let raw = Path::new(path);
        // An absolute path is taken as-is; a relative one is relative to the
        // root, which is what the tree actually sends (paths are absolute,
        // but hand-edited ones in tests and the dialog are not).
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.root.join(raw)
        };

        let resolved = match fs::canonicalize(&joined) {
            Ok(p) => p,
            Err(_) => {
                // The file itself may not exist yet. Its parent normally
                // does; canonicalise that and let the final component ride.
                let parent = joined
                    .parent()
                    .ok_or_else(|| format!("bad path '{path}'"))?
                    .to_path_buf();
                let real_parent =
                    fs::canonicalize(&parent).map_err(|e| format!("cannot resolve '{path}': {e}"))?;
                let name = joined
                    .file_name()
                    .ok_or_else(|| format!("bad path '{path}'"))?;
                real_parent.join(name)
            }
        };

        if !self.within(&resolved) {
            return Err(format!(
                "'{}' is outside the open folder '{}'",
                path,
                self.root.display()
            ));
        }
        Ok(resolved)
    }

    fn within(&self, candidate: &Path) -> bool {
        candidate
            .strip_prefix(&self.root)
            .map(|rest| {
                // `..` can survive canonicalisation on some platforms when
                // the path does not exist; refuse it outright rather than
                // reasoning about which components are real.
                !rest.components().any(|c| matches!(c, Component::ParentDir))
            })
            .unwrap_or(false)
    }
}

/// Read one file inside the workspace.
pub fn read(ws: &Workspace, path: &str) -> Result<FileContents, String> {
    let real = ws.resolve(path)?;
    let bytes = fs::read(&real).map_err(|e| format!("cannot read '{}': {e}", real.display()))?;
    // Lossy rather than an error: a source tree with one latin-1 file in it
    // should still open, with the odd byte replaced, not refuse to start.
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(FileContents {
        path: real.to_string_lossy().into_owned(),
        text,
    })
}

/// Write one file inside the workspace, creating parent directories.
pub fn write(ws: &Workspace, path: &str, text: &str) -> Result<(), String> {
    let real = ws.resolve(path)?;
    if let Some(parent) = real.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create '{}': {e}", parent.display()))?;
    }
    fs::write(&real, text).map_err(|e| format!("cannot write '{}': {e}", real.display()))
}

/// Walk the workspace into a flat, sorted tree.
pub fn scan(ws: &Workspace, max_depth: u32) -> Vec<TreeNode> {
    let mut out = Vec::new();
    walk(ws.root(), ws.root(), 0, max_depth, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, depth: u32, max_depth: u32, out: &mut Vec<TreeNode>) {
    if depth > max_depth || out.len() >= MAX_NODES {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Collect first, then sort: `read_dir` yields in whatever order the
    // filesystem feels like, and an explorer that reshuffles on every
    // refresh is unreadable. Directories first, then case-insensitive by
    // name — the order every file browser uses.
    let mut items: Vec<(String, bool)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let is_dir = entry.path().is_dir();
        items.push((name, is_dir));
    }
    items.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            .then_with(|| a.0.cmp(&b.0))
    });

    for (name, is_dir) in items {
        if out.len() >= MAX_NODES {
            return;
        }
        let path = dir.join(&name);
        out.push(TreeNode {
            path: path.to_string_lossy().into_owned(),
            name,
            is_dir,
            depth,
        });
        if is_dir {
            walk(root, &path, depth + 1, max_depth, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().expect("tempdir");
        let inner = dir.path().join("proj");
        fs::create_dir_all(inner.join("src")).unwrap();
        fs::write(inner.join("src/main.ax"), "def main() -> int:\n    return 1\n").unwrap();
        let ws = Workspace::open(inner.to_str().unwrap()).expect("open workspace");
        (dir, ws)
    }

    #[test]
    fn reads_and_writes_inside_the_workspace() {
        let (_d, ws) = tmp_ws();
        let f = read(&ws, "src/main.ax").expect("read");
        assert!(f.text.contains("def main"));
        write(&ws, "src/new.ax", "x = 1\n").expect("write");
        assert_eq!(read(&ws, "src/new.ax").unwrap().text, "x = 1\n");
    }

    #[test]
    fn refuses_to_escape_the_workspace() {
        let (_d, ws) = tmp_ws();
        // Classic traversal, both spellings.
        let err = read(&ws, "../secret.txt").unwrap_err();
        assert!(err.contains("outside"), "unexpected error: {err}");
        let err = read(&ws, "src/../../secret.txt").unwrap_err();
        assert!(err.contains("outside"), "unexpected error: {err}");
    }

    #[test]
    fn refuses_a_sibling_directory_with_the_root_as_a_prefix() {
        // `C:\proj` vs `C:\proj-evil`: a string prefix test would let this
        // through, which is exactly why `within` compares components.
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("proj");
        let evil = dir.path().join("proj-evil");
        fs::create_dir_all(&proj).unwrap();
        fs::create_dir_all(&evil).unwrap();
        fs::write(evil.join("secret.txt"), "hunter2").unwrap();
        let ws = Workspace::open(proj.to_str().unwrap()).unwrap();
        let target = evil.join("secret.txt");
        let err = ws.resolve(target.to_str().unwrap()).unwrap_err();
        assert!(err.contains("outside"), "unexpected error: {err}");
    }

    #[test]
    fn scan_is_sorted_with_directories_first_and_skips_noise() {
        let (_d, ws) = tmp_ws();
        fs::create_dir_all(ws.root().join("target/debug")).unwrap();
        fs::write(ws.root().join("target/debug/junk.bin"), "x").unwrap();
        fs::write(ws.root().join("zz.ax"), "z").unwrap();
        fs::write(ws.root().join("aa.ax"), "a").unwrap();

        let tree = scan(&ws, 4);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        // Depth-first, so `main.ax` (inside src) comes immediately after it,
        // before the root-level files that sort before it by name.
        assert_eq!(names, vec!["src", "main.ax", "aa.ax", "zz.ax"]);
        assert!(tree[0].is_dir, "src should come first and be a directory");
        assert!(tree.iter().all(|n| n.name != "junk.bin"));
    }

    #[test]
    fn scan_reports_depth_for_indentation() {
        let (_d, ws) = tmp_ws();
        let tree = scan(&ws, 4);
        let depths: std::collections::HashMap<&str, u32> =
            tree.iter().map(|n| (n.name.as_str(), n.depth)).collect();
        assert_eq!(depths["src"], 0);
        assert_eq!(depths["main.ax"], 1);
    }
}