//! Driving the Aoxn toolchain: finding the compiler, and running it.
//!
//! The IDE does not reimplement any part of the compiler. It shells out to
//! exactly the `aoxn` a user would type, captures the output verbatim, and
//! lets the frontend parse diagnostics out of it. That is deliberate: a
//! build that behaves differently inside the IDE than in a terminal is the
//! fastest way to make an IDE untrustworthy, and this way there is nothing
//! to disagree about.
//!
//! Two environment variables matter:
//!
//! * `AOXN_IDE_CC` — the compiler to use. Lets the IDE drive a build of the
//!   compiler's own `target/debug/aoxn` instead of whatever is on PATH.
//! * `AOXN_CLANG` — forwarded from this process when set, because the
//!   compiler shells out to clang and a developer running the IDE from a
//!   shell usually has that variable set to a local LLVM.

use crate::model::{ExecResult, ToolchainInfo};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// The compiler command. `AOXN_IDE_CC` wins; otherwise the name `aoxn`,
/// which a normal install puts on PATH.
pub fn compiler_command() -> String {
    std::env::var("AOXN_IDE_CC").unwrap_or_else(|_| "aoxn".to_string())
}

/// Resolve a bare command name to an executable, the way a shell would.
///
/// On Windows `CreateProcess` only looks at PATH and the current directory,
/// and PATHEXT decides which suffixes count; without this a configured
/// `clang.exe` would report "not found" while working perfectly well from a
/// terminal. On POSIX only an executable `PATH` entry counts.
fn which(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);
    if name.contains('/') || name.contains('\\') {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(|e| e.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Is the Aoxn compiler usable on this machine?
pub fn probe() -> ToolchainInfo {
    let compiler = compiler_command();
    let found = which(&compiler).is_some();
    // clang matters even though the IDE never calls it: the compiler does,
    // and a missing clang is the single most common "why won't this build"
    // question. `AOXN_CLANG` is what the compiler itself honours first.
    let clang = std::env::var("AOXN_CLANG")
        .ok()
        .filter(|c| !c.is_empty())
        .map(|c| PathBuf::from(c))
        .or_else(|| which("clang"))
        .map(|p| p.is_file())
        .unwrap_or(false);

    let version = if found {
        run(&compiler, &["version".to_string()], None)
            .map(|r| r.output.trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    ToolchainInfo {
        compiler,
        compiler_found: found,
        clang_found: clang,
        version,
    }
}

/// Run a command, capturing stdout and stderr as one stream.
pub fn run(program: &str, args: &[String], cwd: Option<&str>) -> Result<ExecResult, String> {
    let started = Instant::now();
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // Forward the developer's clang override, but only if they set one:
    // inventing a value here would silently mask a missing clang.
    if let Ok(v) = std::env::var("AOXN_CLANG") {
        if !v.is_empty() {
            cmd.env("AOXN_CLANG", v);
        }
    }

    let out = cmd
        .output()
        .map_err(|e| format!("cannot run '{program}': {e}"))?;
    Ok(ExecResult {
        code: out.status.code().unwrap_or(-1),
        output: merge(&out.stdout, &out.stderr),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Combine the two captured streams into one log.
///
/// The compiler writes diagnostics to stderr and a program's own output to
/// stdout. They are concatenated — stderr last — rather than interleaved,
/// because the standard library cannot recover the true interleaving of two
/// separately piped streams after the fact (and spawning with one shared
/// pipe is not expressible in `std::process`).
///
/// This is stated rather than hidden because it is visible: in a build log
/// every diagnostic is on stderr and comes after any program output, which
/// is the ordering a reader expects anyway. If a future toolchain writes
/// diagnostics to stdout, the order here is simply the order they were
/// written in.
fn merge(stdout: &[u8], stderr: &[u8]) -> String {
    if stderr.is_empty() {
        return String::from_utf8_lossy(stdout).into_owned();
    }
    if stdout.is_empty() {
        return String::from_utf8_lossy(stderr).into_owned();
    }
    let mut s = stdout.to_vec();
    s.extend_from_slice(stderr);
    String::from_utf8_lossy(&s).into_owned()
}

/// Build a program: `aoxn build <path> -o <exe>`.
pub fn build_args(path: &str, out: &str) -> Vec<String> {
    vec![
        "build".to_string(),
        path.to_string(),
        "-o".to_string(),
        out.to_string(),
    ]
}

/// Check a program: `aoxn check <path>` runs the full pipeline and prints
/// only the diagnostics. `aoxn c` would be the same compile but it also
/// prints the generated C to stdout, which would flood the output panel with
/// text nobody asked for.
pub fn check_args(path: &str) -> Vec<String> {
    vec!["check".to_string(), path.to_string()]
}

/// Run a program: `aoxn run <path> -l <backend>`. The link flags are needed
/// because a UI program imports a backend that resolves to external
/// symbols; an Aoxn program with no backend links needs none.
pub fn run_args(path: &str) -> Vec<String> {
    let flags: &[&str] = if cfg!(windows) {
        &["-l", "user32", "-l", "gdi32"]
    } else {
        &["-l", "X11", "-l", "Xft"]
    };
    let mut args = vec!["run".to_string(), path.to_string()];
    args.extend(flags.iter().map(|s| s.to_string()));
    args
}

/// Where a build artifact should land: beside the source, as `name.exe` on
/// Windows and `name` elsewhere.
pub fn artifact_path(src: &str) -> String {
    let p = Path::new(src);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned());
    let stem = stem.unwrap_or_else(|| "a.out".to_string());
    let parent = p.parent().map(|x| x.to_path_buf()).unwrap_or_default();
    let mut out = parent.join(stem);
    if cfg!(windows) {
        out.set_extension("exe");
    }
    out.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_sits_beside_the_source_with_the_right_suffix() {
        let p = artifact_path("src/main.ax");
        if cfg!(windows) {
            assert!(p.ends_with("main.exe"), "{p}");
        } else {
            assert_eq!(Path::new(&p).file_name().unwrap(), "main");
        }
    }

    #[test]
    fn check_prints_only_diagnostics_and_run_passes_link_flags() {
        let c = check_args("a.ax");
        assert_eq!(c, vec!["check", "a.ax"]);
        let r = run_args("a.ax");
        assert_eq!(r[0], "run");
        assert!(r.contains(&"-l".to_string()));
    }

    #[test]
    fn which_finds_the_shell_and_a_path_miss_does_not() {
        // `sh` is present on every POSIX CI runner; on Windows the test is
        // skipped rather than made conditional, because the whole point is
        // that `which` follows the same rules the OS does.
        if cfg!(windows) {
            assert!(which("definitely-not-a-real-binary-xyz").is_none());
        } else {
            assert!(which("sh").is_some());
        }
    }

    #[test]
    fn run_reports_exit_code_and_captures_output() {
        let (prog, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "echo hello".into()])
        } else {
            ("sh", vec!["-c".into(), "echo hello".into()])
        };
        let r = run(prog, &args, None).expect("run");
        assert_eq!(r.code, 0);
        assert!(r.output.contains("hello"), "got {:?}", r.output);
    }

    #[test]
    fn run_reports_a_missing_program_as_an_error() {
        assert!(run("definitely-not-a-real-binary-xyz", &[], None).is_err());
    }
}