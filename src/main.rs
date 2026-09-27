//! Aoxn compiler driver.
//!
//! Usage:
//!   Aoxn build <file.ax> [-o out.exe] [--O0|--O1|--O2|--O3] [--json]
//!   Aoxn run   <file.ax> [args...] [--O0|--O1] [--json]
//!   Aoxn ir    <file.ax> [--O0|--O1] [--json]
//!   Aoxn --help
//!
//! Optimizer selection (default O3, the documented "parity with clang -O3"):
//!   --O0  no IR pipeline + fast-isel backend   (fastest compile, slow code)
//!   --O1  `default<O1>` pipeline               (~2x faster compile on big
//!         inputs; inlining-heavy code runs slower, loops are unaffected)
//!   --O2  `default<O2>` pipeline               (compile time ~= O3)
//!   --O3  `default<O3>` pipeline               (default)
//! `AOXN_PASSES=<pipeline>` overrides the pipeline text for any level > 0.

use std::path::{Path, PathBuf};
use std::process::Command;

use aoxn::Diag;

/// default optimization level (the documented O3 promise)
const DEFAULT_OPT_LEVEL: u8 = 3;

struct Opts {
    out: Option<String>,
    /// LLVM optimization level: 0 = O0, 1 = O1, 2 = O2, 3 = O3
    opt_level: u8,
    json: bool,
    cpu: Option<String>,
    positional: Vec<String>,
    // everything after "--" (used by `run` to pass args to the compiled program)
    passthrough: Vec<String>,
    /// additional libraries to link (`-l LLVM-C`), repeatable
    libs: Vec<String>,
    /// additional library search paths (`-L C:\...\lib`), repeatable
    lib_paths: Vec<String>,
}

fn parse_opts(args: &[String]) -> Opts {
    let mut opts = Opts {
        out: None,
        opt_level: DEFAULT_OPT_LEVEL,
        json: false,
        cpu: None,
        positional: Vec::new(),
        passthrough: Vec::new(),
        libs: Vec::new(),
        lib_paths: Vec::new(),
    };
    let mut level_flags = 0usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" if i + 1 < args.len() => {
                opts.out = Some(args[i + 1].clone());
                i += 2;
            }
            "-l" if i + 1 < args.len() => {
                opts.libs.push(args[i + 1].clone());
                i += 2;
            }
            "-L" if i + 1 < args.len() => {
                opts.lib_paths.push(args[i + 1].clone());
                i += 2;
            }
            "--O0" | "-O0" => {
                opts.opt_level = 0;
                level_flags += 1;
                i += 1;
            }
            "--O1" | "-O1" => {
                opts.opt_level = 1;
                level_flags += 1;
                i += 1;
            }
            "--O2" | "-O2" => {
                opts.opt_level = 2;
                level_flags += 1;
                i += 1;
            }
            "--O3" | "-O3" => {
                opts.opt_level = 3;
                level_flags += 1;
                i += 1;
            }
            "--cpu" if i + 1 < args.len() => {
                opts.cpu = Some(args[i + 1].clone());
                i += 2;
            }
            "--json" => {
                opts.json = true;
                i += 1;
            }
            "--" => {
                opts.passthrough = args[i + 1..].to_vec();
                break;
            }
            other => {
                opts.positional.push(other.to_string());
                i += 1;
            }
        }
    }
    if level_flags > 1 {
        eprintln!("error: at most one of --O0/--O1/--O2/--O3 may be given");
        std::process::exit(2);
    }
    // target CPU for the LLVM backend (e.g. `native`); also settable via AOXN_CPU
    if let Some(cpu) = &opts.cpu {
        std::env::set_var("AOXN_CPU", cpu);
    }
    opts
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        std::process::exit(2);
    }
    match args[0].as_str() {
        "--help" | "-h" | "help" => print_help(),
        "build" => cmd_build(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "ir" => cmd_ir(&args[1..]),
        other => {
            eprintln!("error: unknown command '{other}' (try: Aoxn --help)");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "Aoxn compiler v{}\n\n\
         USAGE:\n  \
         Aoxn build <file.ax> [-o out] [--O0|--O1] [--json]   compile to a native executable\n  \
         Aoxn run <file.ax> [--O0|--O1] [--json] [-- args...]  compile and run in one step\n  \
         Aoxn ir <file.ax> [--O0|--O1] [--json]               print the LLVM IR\n\n\
         FLAGS:\n  \
         -o <path>   output executable path (default: <file>.exe)\n  \
         --O0        disable all optimizations (no IR pipeline + fast-isel backend)\n  \
         --O1        fast compile (default<O1>): for iteration and compile-time-sensitive CI\n  \
         --O2        default<O2> pipeline (compile time ~= O3)\n  \
         --O3        default<O3> pipeline (default: best runtime performance)\n  \
         --cpu <c>   target CPU for codegen, e.g. native (default: generic)\n  \
         --json      emit diagnostics as JSON (AI-agent friendly)\n\n\
         ENV:\n  \
         AOXN_PASSES=<pipeline>  override the LLVM pass pipeline (e.g. default<O1>)\n  \
         AOXN_CPU=native         same as --cpu native\n  \
         AOXN_NO_CACHE=1         disable the `Aoxn run` build cache\n  \
         AOXN_CACHE_DIR=<dir>    build-cache location (default: <cwd>/target/cache)",
        env!("CARGO_PKG_VERSION")
    );
}

fn report(diags: &[Diag], json: bool) {
    if json {
        eprintln!("{}", aoxn::diags_to_json(diags));
    } else {
        for d in diags {
            eprintln!("{}", aoxn::diag_to_string(d));
        }
    }
}

fn cmd_build(args: &[String]) {
    let opts = parse_opts(args);
    if opts.positional.is_empty() {
        eprintln!("error: 'Aoxn build' needs an input file (.ax)");
        std::process::exit(2);
    }
    // the first file is the entry; `import "..."` pulls in the rest
    let exe = opts.out.map(PathBuf::from).unwrap_or_else(|| default_exe(&opts.positional[0]));
    match aoxn::build_paths_opts_lvl(
        &opts.positional,
        &exe,
        opts.opt_level,
        &opts.libs,
        &opts.lib_paths,
    ) {
        Ok(()) => println!("{}", exe.display()),
        Err(diags) => {
            report(&diags, opts.json);
            std::process::exit(1);
        }
    }
}

fn cmd_run(args: &[String]) {
    let opts = parse_opts(args);
    if opts.positional.is_empty() {
        eprintln!("error: 'Aoxn run' needs an input file (.ax)");
        std::process::exit(2);
    }
    // program args: anything after "--"
    let prog_args = &opts.passthrough;

    // Build cache (F3): `Aoxn run` is usually "edit one line, run again", and
    // for small programs the link + process startup dominate (the compile
    // itself is ~18ms). Key the cached executable on the *content* of the
    // whole program (entry + all transitive imports), the compiler binary
    // itself, and every option that changes codegen.
    let cache_path = cache_key(&opts).map(|k| cache_dir().join(format!("{k}{}", exe_suffix())));
    if let Some(cached) = &cache_path {
        if cached.is_file() {
            // bump the mtime so a hot entry survives `prune_cache`'s
            // oldest-first pass (approximate LRU, best-effort)
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .open(cached)
                .and_then(|f| f.set_modified(std::time::SystemTime::now()));
            std::process::exit(run_exe(cached, prog_args));
        }
    }

    // Build into the cache directory under a unique name, then publish it, so
    // concurrent `Aoxn run` invocations never race on the same output file.
    let build_path = match &cache_path {
        Some(final_path) => {
            let stem = final_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            cache_dir().join(format!("{stem}.{}{}", std::process::id(), exe_suffix()))
        }
        None => temp_exe(&opts.positional[0]),
    };
    if let Err(diags) = aoxn::build_paths_opts_lvl(
        &opts.positional,
        &build_path,
        opts.opt_level,
        &opts.libs,
        &opts.lib_paths,
    ) {
        report(&diags, opts.json);
        std::process::exit(1);
    }

    let exe = match &cache_path {
        Some(final_path) => {
            let _ = std::fs::remove_file(final_path);
            if std::fs::rename(&build_path, final_path).is_err() {
                build_path // publish failed (e.g. AV lock): run the unique copy
            } else {
                prune_cache();
                final_path.clone()
            }
        }
        None => build_path,
    };
    let status = run_exe(&exe, prog_args);
    if cache_path.is_none() {
        let _ = std::fs::remove_file(&exe); // temp copy, no cache entry
    }
    std::process::exit(status);
}

fn cmd_ir(args: &[String]) {
    let opts = parse_opts(args);
    if opts.positional.is_empty() {
        eprintln!("error: 'Aoxn ir' needs an input file (.ax)");
        std::process::exit(2);
    }
    match aoxn::compile_paths_to_ir_lvl(&opts.positional, opts.opt_level) {
        Ok(ir) => print!("{ir}"),
        Err(diags) => {
            report(&diags, opts.json);
            std::process::exit(1);
        }
    }
}

fn default_exe(input: &str) -> PathBuf {
    let mut p = PathBuf::from(input);
    p.set_extension(aoxn::platform::exe_ext());
    p
}

/// Executable suffix with the leading dot (`".exe"` on Windows, `""` elsewhere)
/// for use in `format!`; `platform::exe_ext()` is dot-less for `set_extension`.
fn exe_suffix() -> String {
    let e = aoxn::platform::exe_ext();
    if e.is_empty() {
        String::new()
    } else {
        format!(".{e}")
    }
}

fn temp_exe(input: &str) -> PathBuf {
    let stem = PathBuf::from(input)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "program".into());
    let dir = std::env::temp_dir().join("Aoxn-run");
    let _ = std::fs::create_dir_all(&dir);
    let unique = format!("{}-{}{}", stem, std::process::id(), exe_suffix());
    dir.join(unique)
}

/// Run a compiled program, inheriting stdio, and return its exit code.
fn run_exe(exe: &Path, args: &[String]) -> i32 {
    let status = Command::new(exe).args(args).status().unwrap_or_else(|e| {
        eprintln!("error: failed to run compiled program {}: {e}", exe.display());
        std::process::exit(1);
    });
    status.code().unwrap_or(1)
}

// ---- `Aoxn run` build cache ----

/// Directory holding cached executables: `AOXN_CACHE_DIR`, else
/// `<cwd>/target/cache`, falling back to the temp dir when that is not
/// writable (read-only source trees).
fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("AOXN_CACHE_DIR") {
        let p = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let local = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("target")
        .join("cache");
    if std::fs::create_dir_all(&local).is_ok() {
        local
    } else {
        let fallback = std::env::temp_dir().join("aoxn-cache");
        let _ = std::fs::create_dir_all(&fallback);
        fallback
    }
}

/// Cache key for this invocation, or `None` when caching is disabled or the
/// dependency set cannot be read (then we always compile).
fn cache_key(opts: &Opts) -> Option<String> {
    use std::hash::{BuildHasher, Hash, Hasher};

    if std::env::var("AOXN_NO_CACHE").is_ok() {
        return None;
    }
    // every source file the compiler will read, entry + transitive imports
    let sources = aoxn::dependency_files(&opts.positional)?;

    let mut h = aoxn::hashing::FastBuild.build_hasher();
    h.write(b"aoxn-run-cache-v1");
    // source content: the only input that usually changes
    for path in &sources {
        path.to_string_lossy().hash(&mut h);
        std::fs::read(path).ok()?.hash(&mut h);
    }
    // the compiler binary itself: rebuilding `aoxn` must invalidate the cache
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(md) = std::fs::metadata(&exe) {
            md.len().hash(&mut h);
            if let Ok(t) = md.modified() {
                if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                    d.as_nanos().hash(&mut h);
                }
            }
        }
    }
    // every option that changes generated code
    opts.opt_level.hash(&mut h);
    std::env::var("AOXN_CPU").unwrap_or_default().hash(&mut h);
    std::env::var("AOXN_PASSES").unwrap_or_default().hash(&mut h);
    opts.libs.hash(&mut h);
    opts.lib_paths.hash(&mut h);
    // linked-in toolchain identity (cheap: the resolved clang path)
    aoxn::find_clang().map(|p| p.display().to_string()).hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// Keep the cache bounded: after a publish, drop the oldest entries beyond
/// `MAX_CACHE_ENTRIES` (cache hits bump the entry's mtime, so this is an
/// approximate LRU). Best-effort — failures are ignored.
fn prune_cache() {
    const MAX_CACHE_ENTRIES: usize = 64;
    let dir = cache_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            // only published entries (16 hex digits, optionally + the exe
            // suffix) — never the `<key>.<pid>` temp file of a concurrent
            // invocation
            let name = path.file_name()?.to_string_lossy().into_owned();
            let suffix = exe_suffix();
            let core = name.strip_suffix(suffix.as_str()).unwrap_or(&name);
            if core.len() != 16 || !core.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let md = e.metadata().ok()?;
            if !md.is_file() {
                return None;
            }
            Some((md.modified().unwrap_or(std::time::UNIX_EPOCH), path))
        })
        .collect();
    if files.len() <= MAX_CACHE_ENTRIES {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    for (_, path) in files.iter().take(files.len() - MAX_CACHE_ENTRIES) {
        let _ = std::fs::remove_file(path);
    }
}
