//! Types shared between the Rust backend and the workbench UI.
//!
//! Everything the frontend receives is one of these; there is no free-form
//! JSON shape anywhere in the command surface, so the TypeScript side can
//! mirror it exactly (`lib/types.ts`).

/// One entry in the project explorer.
///
/// The tree is sent FLAT with a `depth` column rather than nested: a 4000
/// node JSON tree is expensive to build and re-serialise on every refresh,
/// while a flat array renders with a trivial map in React and matches how
/// the model is actually consumed (expand/collapse over a sorted list).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    /// Full path, so the frontend never has to reassemble it (and never
    /// has to agree with us about separators).
    pub path: String,
    pub is_dir: bool,
    pub depth: u32,
}

/// A file's contents plus enough metadata for the status bar.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContents {
    pub path: String,
    pub text: String,
}

/// Result of running an external command.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub code: i32,
    /// stdout and stderr interleaved, as one stream. The Aoxn compiler writes
    /// diagnostics to stderr, and a user reading a build log wants them in
    /// the order they happened, so they are merged rather than separated.
    pub output: String,
    /// How long the command took, in milliseconds — the status bar shows it
    /// and it is the only honest feedback that anything happened at all.
    pub duration_ms: u64,
}

/// What the UI needs to know about the Aoxn toolchain before it offers to
/// build anything.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainInfo {
    /// The compiler command as configured (never empty — falls back to
    /// "aoxn", which is what a normal install puts on PATH).
    pub compiler: String,
    /// Whether that command actually resolves on this machine. A false here
    /// is the single most useful thing the IDE can tell a new user.
    pub compiler_found: bool,
    /// Whether clang is visible, because the compiler shells out to it and
    /// every build fails without it. `AOXN_CLANG` wins over PATH.
    pub clang_found: bool,
    pub version: String,
}