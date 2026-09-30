//! Aoxn compiler pipeline: lex -> parse -> typecheck -> codegen -> native object -> link.

pub mod ast;
pub mod codegen;
pub mod codegen_c;
pub mod files;
pub mod hashing;
pub mod lexer;
pub mod llvm;
pub mod parser;
pub mod platform;
pub mod ts;
pub mod typecheck;

use crate::ast::{FnDecl, Program, StructDecl};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Diag {
    pub stage: &'static str, // "lex" | "parse" | "type" | "internal" | "link" | "io"
    pub file: u32,           // index into the compilation file registry
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl Diag {
    /// constructor at a source position (B1 in docs/p2-compiler-performance.md:
    /// one place to build diagnostics, keeping error-site diffs small)
    pub fn at(stage: &'static str, file: u32, line: usize, col: usize, message: impl Into<String>) -> Diag {
        Diag { stage, file, line, col, message: message.into() }
    }

    fn internal(message: impl Into<String>) -> Diag {
        Diag { stage: "internal", file: u32::MAX, line: 0, col: 0, message: message.into() }
    }

    fn to_json(&self) -> String {
        fn esc(s: &str) -> String {
            let mut out = String::with_capacity(s.len() + 8);
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out
        }
        format!(
            "{{\"stage\":\"{}\",\"file\":\"{}\",\"line\":{},\"col\":{},\"message\":\"{}\"}}",
            self.stage,
            esc(&files::name(self.file)),
            self.line,
            self.col,
            esc(&self.message)
        )
    }
}

pub fn diags_to_json(diags: &[Diag]) -> String {
    let items: Vec<String> = diags.iter().map(|d| d.to_json()).collect();
    format!("{{\"ok\":false,\"errors\":[{}]}}", items.join(","))
}

/// human-readable one-line form used by the CLI
pub fn diag_to_string(d: &Diag) -> String {
    let file = files::name(d.file);
    let loc = if d.line > 0 {
        format!("{file}:{}:{}: ", d.line, d.col)
    } else if file != "?" {
        format!("{file}: ")
    } else {
        String::new()
    };
    format!("[{}] {}{}", d.stage, loc, d.message)
}

/// AOXN_TIME=1 prints per-pipeline-stage wall-clock to stderr (lex / parse /
/// typecheck / codegen / link), in the style of AOXN_TC_TRACE / AOXN_CG_TRACE.
fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    if std::env::var("AOXN_TIME").is_ok() {
        let t = std::time::Instant::now();
        let out = f();
        eprintln!("[time] {label}: {:?}", t.elapsed());
        out
    } else {
        f()
    }
}

/// Full compile pipeline from Aoxn source text to a native object file.
pub fn compile_to_object(src: &str, obj_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    compile_sources_to_object(&[src.to_string()], obj_path, opt)
}

/// Optimization-level form of [`compile_to_object`] (`0` = O0 .. `3` = O3).
pub fn compile_to_object_lvl(src: &str, obj_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    compile_sources_to_object_lvl(&[src.to_string()], obj_path, opt_level)
}

/// Compile multiple Aoxn sources as one program (merged namespace).
pub fn compile_sources_to_object(sources: &[String], obj_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    compile_sources_to_object_lvl(sources, obj_path, codegen::level_of(opt))
}

/// Optimization-level form of [`compile_sources_to_object`].
pub fn compile_sources_to_object_lvl(sources: &[String], obj_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    let program = parse_sources(sources)?;
    finish_to_object(program, obj_path, opt_level)
}

/// Compile from file paths, resolving `import "..."` recursively
/// (include-once per canonical path, circular imports rejected).
pub fn compile_paths_to_object(paths: &[String], obj_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    compile_paths_to_object_lvl(paths, obj_path, codegen::level_of(opt))
}

/// Optimization-level form of [`compile_paths_to_object`].
pub fn compile_paths_to_object_lvl(paths: &[String], obj_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    let program = load_program(paths)?;
    finish_to_object(program, obj_path, opt_level)
}

/// Full compile pipeline from Aoxn source text to LLVM IR text (for `Aoxn ir`).
pub fn compile_to_ir(src: &str, opt: bool) -> Result<String, Vec<Diag>> {
    compile_sources_to_ir(&[src.to_string()], opt)
}

/// Multiple sources → LLVM IR text.
pub fn compile_sources_to_ir(sources: &[String], opt: bool) -> Result<String, Vec<Diag>> {
    compile_sources_to_ir_lvl(sources, codegen::level_of(opt))
}

/// Optimization-level form of [`compile_sources_to_ir`].
pub fn compile_sources_to_ir_lvl(sources: &[String], opt_level: u8) -> Result<String, Vec<Diag>> {
    let program = parse_sources(sources)?;
    finish_to_ir(program, opt_level)
}

/// File paths (with imports) → LLVM IR text.
pub fn compile_paths_to_ir(paths: &[String], opt: bool) -> Result<String, Vec<Diag>> {
    compile_paths_to_ir_lvl(paths, codegen::level_of(opt))
}

/// Optimization-level form of [`compile_paths_to_ir`].
pub fn compile_paths_to_ir_lvl(paths: &[String], opt_level: u8) -> Result<String, Vec<Diag>> {
    let program = load_program(paths)?;
    finish_to_ir(program, opt_level)
}

/// `true` when the experimental C-emitting backend is selected via
/// `--backend c` / `AOXN_BACKEND=c` (docs/llvm-independence-report.md).
/// Anything else keeps the default LLVM backend.
pub fn backend_is_c() -> bool {
    matches!(std::env::var("AOXN_BACKEND").as_deref(), Ok("c") | Ok("C"))
}

fn finish_to_object(program: Program, obj_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    let out = timed("typecheck", || typecheck::check(&program)).map_err(|d| vec![d])?;
    let mut program = program;
    // only concrete functions reach codegen: drop generic declarations,
    // append their monomorphized instances
    program.funcs.retain(|f| f.type_params.is_empty());
    program.funcs.extend(out.instances);
    if backend_is_c() {
        timed("codegen", || {
            codegen_c::generate_to_object(&program, obj_path, opt_level, &out.call_map)
        })
        .map_err(|m| vec![Diag::internal(m)])?;
    } else {
        timed("codegen", || {
            codegen::generate_to_object(&program, obj_path, opt_level, &out.call_map)
        })
        .map_err(|m| vec![Diag::internal(m)])?;
    }
    Ok(())
}

fn finish_to_ir(program: Program, opt_level: u8) -> Result<String, Vec<Diag>> {
    let out = timed("typecheck", || typecheck::check(&program)).map_err(|d| vec![d])?;
    let mut program = program;
    program.funcs.retain(|f| f.type_params.is_empty());
    program.funcs.extend(out.instances);
    codegen::generate_ir_text(&program, opt_level, &out.call_map).map_err(|m| vec![Diag::internal(m)])
}

/// source-code based entry (no import resolution; imports are an error)
fn parse_sources(sources: &[String]) -> Result<Program, Vec<Diag>> {
    files::clear();
    let mut imports = Vec::new();
    let mut structs = Vec::new();
    let mut funcs = Vec::new();
    for src in sources {
        let file_id = files::register("<source>");
        let tokens = timed("lex", || lexer::lex(src, file_id)).map_err(|d| vec![d])?;
        let program = timed("parse", || parser::parse(tokens)).map_err(|d| vec![d])?;
        imports.extend(program.imports);
        structs.extend(program.structs);
        funcs.extend(program.funcs);
    }
    if let Some(imp) = imports.first() {
        return Err(vec![Diag {
            stage: "io",
            file: imp.pos.file,
            line: imp.pos.line,
            col: imp.pos.col,
            message: format!(
                "import \"{}\" requires compiling from files (imports resolve relative to the importing file)",
                imp.path
            ),
        }]);
    }
    Ok(Program { imports: vec![], structs, funcs })
}

/// file-path based entry: resolve imports recursively
fn load_program(entries: &[String]) -> Result<Program, Vec<Diag>> {
    files::clear();
    let mut state = LoadState {
        visited: HashSet::new(),
        stack: Vec::new(),
        structs: Vec::new(),
        funcs: Vec::new(),
    };
    for entry in entries {
        load_file(Path::new(entry), &mut state)?;
    }
    Ok(Program { imports: vec![], structs: state.structs, funcs: state.funcs })
}

struct LoadState {
    visited: HashSet<PathBuf>,
    stack: Vec<PathBuf>,
    structs: Vec<StructDecl>,
    funcs: Vec<FnDecl>,
}

fn load_file(path: &Path, state: &mut LoadState) -> Result<(), Vec<Diag>> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        vec![Diag {
            stage: "io",
            file: u32::MAX,
            line: 0,
            col: 0,
            message: format!("cannot open '{}': {e}", path.display()),
        }]
    })?;
    if state.stack.contains(&canonical) {
        let cycle: Vec<String> = state
            .stack
            .iter()
            .chain(std::iter::once(&canonical))
            .map(|p| p.display().to_string())
            .collect();
        return Err(vec![Diag {
            stage: "io",
            file: u32::MAX,
            line: 0,
            col: 0,
            message: format!("circular import: {}", cycle.join(" -> ")),
        }]);
    }
    if state.visited.contains(&canonical) {
        return Ok(()); // include-once
    }
    state.visited.insert(canonical.clone());
    state.stack.push(canonical.clone());

    let src = std::fs::read_to_string(path).map_err(|e| {
        vec![Diag {
            stage: "io",
            file: u32::MAX,
            line: 0,
            col: 0,
            message: format!("cannot read '{}': {e}", path.display()),
        }]
    })?;
    let file_id = files::register(path.display().to_string());
    let label = path.display().to_string();
    // .ts/.tsx go through the TS-M1 front end, everything else the Aoxn one;
    // both lower into the same AST (docs/ts-m1-spec.md)
    let is_ts = path.extension().map(|e| e == "ts" || e == "tsx").unwrap_or(false);
    let program = if is_ts {
        timed(&format!("parse {label}"), || ts::parser::parse(file_id, &src)).map_err(|d| vec![d])?
    } else {
        let tokens = timed(&format!("lex {label}"), || lexer::lex(&src, file_id)).map_err(|d| vec![d])?;
        timed(&format!("parse {label}"), || parser::parse(tokens)).map_err(|d| vec![d])?
    };

    // resolve this file's imports relative to its own directory
    let dir = canonical.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    for imp in &program.imports {
        let imp_path = resolve_import(&dir, &imp.path);
        load_file(&imp_path, state).map_err(|diags| {
            // attach the import site to resolution errors that lack one
            let out: Vec<Diag> = diags
                .into_iter()
                .map(|mut d| {
                    if d.file == u32::MAX && d.line == 0 {
                        d.file = imp.pos.file;
                        d.line = imp.pos.line;
                        d.col = imp.pos.col;
                    }
                    d
                })
                .collect();
            out
        })?;
    }

    state.structs.extend(program.structs);
    state.funcs.extend(program.funcs);
    state.stack.pop();
    Ok(())
}

fn resolve_import(dir: &Path, import: &str) -> PathBuf {
    let p = Path::new(import);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    }
}

/// Every source file `load_program` would read for `entries`, imports
/// included (include-once, resolved relative to the importing file's
/// directory). Used by the `Aoxn run` build cache to key on the content of the
/// whole program; returns `None` when a file cannot be read, which disables
/// the cache for that invocation.
pub fn dependency_files(entries: &[String]) -> Option<Vec<PathBuf>> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = entries.iter().map(PathBuf::from).collect();
    while let Some(path) = stack.pop() {
        let canonical = std::fs::canonicalize(&path).ok()?;
        if !seen.insert(canonical.clone()) {
            continue; // include-once, exactly like `load_file`
        }
        let src = std::fs::read_to_string(&canonical).ok()?;
        let dir = canonical.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        for imp in scan_imports(&src) {
            stack.push(resolve_import(&dir, &imp));
        }
        out.push(canonical);
    }
    out.sort();
    Some(out)
}

/// Paths of every top-level `import "..."` declaration in `src`. The grammar
/// only allows imports at top level, so a line-based scan is exact for
/// well-formed programs and may only *over*-include on malformed input (which
/// is safe for a cache key: more dependencies means fewer cache hits, never a
/// stale hit).
fn scan_imports(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let rest = match line.trim_start().strip_prefix("import") {
            Some(r) => r,
            None => continue,
        };
        // word boundary: `imports` / `important` are not import declarations
        if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = rest.trim_start();
        if let Some(quoted) = rest.strip_prefix('"') {
            if let Some(end) = quoted.find('"') {
                out.push(quoted[..end].to_string());
            }
        }
    }
    out
}

/// Locate the clang driver used for final linking.
/// Order: AOXN_CLANG env -> PATH -> repo-local LLVM -> standard install dir.
pub fn find_clang() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOXN_CLANG") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let names: Vec<&str> = if cfg!(windows) { vec!["clang.exe", "clang"] } else { vec!["clang"] };
    for name in names {
        if let Some(path) = which(name) {
            return Some(path);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // compile-time repo-local toolchain layout: <repo>/LLVM/bin/clang.exe
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("LLVM").join("bin").join("clang.exe"));
    candidates.push(PathBuf::from(r"C:\Program Files\LLVM\bin\clang.exe"));
    candidates.into_iter().find(|p| p.is_file())
}

fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Compile Aoxn source files to an executable at `exe_path` (links via clang).
pub fn build_exe(src: &str, exe_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    build_sources_exe(&[src.to_string()], exe_path, opt)
}

/// Optimization-level form of [`build_exe`].
pub fn build_exe_lvl(src: &str, exe_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    build_sources_exe_lvl(&[src.to_string()], exe_path, opt_level)
}

/// Compile multiple sources (merged namespace) into an executable.
pub fn build_sources_exe(sources: &[String], exe_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    build_sources_exe_lvl(sources, exe_path, codegen::level_of(opt))
}

/// Optimization-level form of [`build_sources_exe`].
pub fn build_sources_exe_lvl(sources: &[String], exe_path: &Path, opt_level: u8) -> Result<(), Vec<Diag>> {
    let obj_path = exe_path.with_extension(crate::platform::obj_ext());
    compile_sources_to_object_lvl(sources, &obj_path, opt_level)?;
    let link_result = link(&obj_path, exe_path);
    if link_result.is_ok() {
        let _ = std::fs::remove_file(&obj_path);
    }
    link_result
}

/// Compile from file paths (with import resolution) into an executable.
pub fn build_paths_exe(paths: &[String], exe_path: &Path, opt: bool) -> Result<(), Vec<Diag>> {
    build_paths_opts(paths, exe_path, opt, &[], &[])
}

/// like `build_paths_exe` with extra link libraries and search paths
pub fn build_paths_opts(
    paths: &[String],
    exe_path: &Path,
    opt: bool,
    libs: &[String],
    lib_paths: &[String],
) -> Result<(), Vec<Diag>> {
    build_paths_opts_lvl(paths, exe_path, codegen::level_of(opt), libs, lib_paths)
}

/// Optimization-level form of [`build_paths_opts`] (`0` = O0 .. `3` = O3).
pub fn build_paths_opts_lvl(
    paths: &[String],
    exe_path: &Path,
    opt_level: u8,
    libs: &[String],
    lib_paths: &[String],
) -> Result<(), Vec<Diag>> {
    let obj_path = exe_path.with_extension(crate::platform::obj_ext());
    compile_paths_to_object_lvl(paths, &obj_path, opt_level)?;
    let link_result = link_opts(&obj_path, exe_path, libs, lib_paths);
    if link_result.is_ok() {
        let _ = std::fs::remove_file(&obj_path);
    }
    link_result
}

/// Link a native object file into an executable using clang.
pub fn link(obj_path: &Path, exe_path: &Path) -> Result<(), Vec<Diag>> {
    link_opts(obj_path, exe_path, &[], &[])
}

/// Link with additional libraries and library search paths.
pub fn link_opts(obj_path: &Path, exe_path: &Path, libs: &[String], lib_paths: &[String]) -> Result<(), Vec<Diag>> {
    let clang = find_clang().ok_or_else(|| vec![Diag {
        stage: "link",
        file: u32::MAX,
        line: 0,
        col: 0,
        message: "cannot find clang for linking. Set AOXN_CLANG to the clang executable \
                  or add LLVM's bin directory to PATH."
            .into(),
    }])?;

    let mut cmd = Command::new(&clang);
    cmd.arg(obj_path).arg("-o").arg(exe_path);
    // 8 MB stack: large fixed-size arrays live on the stack (allocas)
    if let Some(flag) = crate::platform::stack_link_flag() {
        cmd.arg(flag);
    }
    for dir in lib_paths {
        cmd.arg(format!("-L{dir}"));
        // POSIX: record an rpath too. Homebrew's `libLLVM-C.dylib` re-exports
        // `@rpath/libLLVM.dylib`, and a non-default `-L` dir (e.g.
        // /usr/lib/llvm-18/lib) is not in the loader's search path, so without
        // an LC_RPATH the linked executable cannot start. MSVC/lld-link on
        // Windows rejects `-rpath`, so this stays POSIX-only (build.rs emits
        // the same flag for the compiler itself).
        if !crate::platform::is_windows() {
            cmd.arg(format!("-Wl,-rpath,{dir}"));
        }
    }
    for lib in libs {
        cmd.arg(format!("-l{lib}"));
    }
    let status = timed("link", || {
        cmd
            .status()
            .map_err(|e| vec![Diag {
                stage: "link",
                file: u32::MAX,
                line: 0,
                col: 0,
                message: format!("failed to spawn {}: {e}", clang.display()),
            }])
    })?;

    if !status.success() {
        return Err(vec![Diag {
            stage: "link",
            file: u32::MAX,
            line: 0,
            col: 0,
            message: format!("clang linking failed with exit code {:?}", status.code()),
        }]);
    }
    Ok(())
}



