//! Platform abstraction (v0.26.1): centralize OS-specific decisions so
//! `lib.rs`/`main.rs`/`codegen.rs`/`build.rs` stop scattering `cfg!(windows)`
//! branches. Behavior on Windows is byte-identical to v0.26.0.

use std::path::{Path, PathBuf};

/// Executable file extension: `.exe` on Windows, empty elsewhere.
pub fn exe_ext() -> &'static str {
    if cfg!(windows) {
        "exe"
    } else {
        ""
    }
}

/// Object file extension: `.obj` on Windows (MSVC convention), `.o` elsewhere
/// (ELF/Mach-O convention).
pub fn obj_ext() -> &'static str {
    if cfg!(windows) {
        "obj"
    } else {
        "o"
    }
}

/// Linker flag to give the main thread 8MB of stack (needed for large
/// stack-allocated arrays in user programs).
///
/// - Windows: default main-thread stack is 1MB; the `/STACK` linker flag
///   (MSVC link.exe) raises it to 8MB.
/// - Linux: main-thread stack is controlled by `RLIMIT_STACK` (8MB default on
///   mainstream distros); linker flags don't affect the main thread, so we
///   don't emit any.
/// - macOS: main-thread stack is fixed at 8MB; no linker flag needed.
///
/// Returns `None` when no flag should be passed.
pub fn stack_link_flag() -> Option<&'static str> {
    if cfg!(windows) {
        Some("-Wl,/STACK:8388608")
    } else {
        None
    }
}

/// `true` when targeting Windows.
pub fn is_windows() -> bool {
    cfg!(windows)
}

/// `true` when targeting Linux.
pub fn is_linux() -> bool {
    cfg!(target_os = "linux")
}

/// `true` when targeting macOS (Darwin).
pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

/// Default LLVM library directory candidates for the host platform, in
/// probe order. `build.rs` uses this to locate `libLLVM-C`/`libLLVM`.
pub fn llvm_dir_candidates() -> Vec<&'static str> {
    if cfg!(windows) {
        vec![
            r"C:\Program Files\LLVM\lib",
            r"C:\Program Files\LLVM\lib\x64",
            r"C:\LLVM\lib",
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "/opt/homebrew/opt/llvm/lib",
            "/usr/local/opt/llvm/lib",
            "/opt/local/libexec/llvm/lib",
        ]
    } else {
        // Linux: probe versioned LLVM packages (newest first), then unversioned
        vec![
            "/usr/lib/llvm-23/lib",
            "/usr/lib/llvm-22/lib",
            "/usr/lib/llvm-21/lib",
            "/usr/lib/llvm-20/lib",
            "/usr/lib/llvm-19/lib",
            "/usr/lib/llvm-18/lib",
            "/usr/lib/llvm-17/lib",
            "/usr/lib/llvm-16/lib",
            "/usr/lib/llvm-15/lib",
            "/usr/lib/llvm/lib",
            "/usr/lib64",
            "/usr/lib",
        ]
    }
}

/// Platform name for `target_os()` builtin: "windows" | "linux" | "macos" | "other".
pub fn target_os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

/// `-l` name for the LLVM C API library on this host.
///
/// Windows and macOS ship a dedicated `LLVM-C` library, but the Debian/Ubuntu
/// LLVM packages put the C API inside a versioned `libLLVM-<N>.so`, so the
/// name has to be probed from the install instead of hardcoded ("cannot find
/// -lLLVM-C" on Linux CI). `AOXN_LLVM_LIB` overrides the probe result.
///
/// The C API symbols are always exported from whichever library is found
/// (`LLVM-C`, `libLLVM-<N>`, or `libLLVM`), so the probed name is the only
/// platform difference for linking the self-hosted LLVM drivers.
pub fn llvm_link_name() -> String {
    if let Ok(v) = std::env::var("AOXN_LLVM_LIB") {
        if !v.is_empty() {
            return v;
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for key in ["AOXN_LLVM_DIR", "AXON_LLVM_DIR"] {
        if let Ok(v) = std::env::var(key) {
            // accept either the install root or its lib directory
            dirs.push(PathBuf::from(&v).join("lib"));
            dirs.push(PathBuf::from(v));
        }
    }
    for cand in llvm_dir_candidates() {
        dirs.push(PathBuf::from(cand));
    }
    // Distro multiarch dirs: Debian/Ubuntu's `libllvm<N>` runtime package puts
    // `libLLVM-<N>.so.1` there, and it is a default linker search path — so
    // `-lLLVM-<N>` resolves even when the LLVM install dir only has the
    // development symlink or nothing linkable at all.
    if !cfg!(windows) {
        dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        dirs.push(PathBuf::from("/usr/lib/aarch64-linux-gnu"));
        dirs.push(PathBuf::from("/usr/lib64"));
        dirs.push(PathBuf::from("/usr/lib"));
    }
    for dir in dirs {
        if let Some(name) = probe_llvm_lib(&dir) {
            return name;
        }
    }
    // Windows/macOS default; harmless if the probe found nothing there
    "LLVM-C".to_string()
}

/// `-l` name for the LLVM C API library inside `dir`, if it can be identified.
/// Exposed so the layout rules (dedicated `LLVM-C` vs versioned
/// `libLLVM-<N>` vs unversioned `libLLVM`) are testable on any host.
pub fn llvm_link_name_in(dir: &Path) -> Option<String> {
    probe_llvm_lib(dir)
}

/// `-l` name for the LLVM C API library inside `dir`, if it can be identified.
/// Prefers a dedicated `LLVM-C` library, else the newest `libLLVM-<N>`, else
/// an unversioned `libLLVM`.
fn probe_llvm_lib(dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut versioned: Vec<(u32, String)> = Vec::new();
    let mut unversioned = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Windows "LLVM-C.lib" / POSIX "libLLVM-C.so" / "libLLVM-C.dylib"
        let c_api = name.starts_with("LLVM-C.") || name.starts_with("libLLVM-C.");
        if c_api && name != "LLVM-C.dll" {
            return Some("LLVM-C".to_string());
        }
        if let Some(rest) = name.strip_prefix("libLLVM-") {
            // libLLVM-18.so, libLLVM-18.so.1, libLLVM-18.dylib
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u32>() {
                versioned.push((n, format!("LLVM-{n}")));
            }
            continue;
        }
        if name.starts_with("libLLVM.") || name == "LLVM.lib" {
            unversioned = true;
        }
    }
    versioned.sort_by(|a, b| b.0.cmp(&a.0));
    if let Some((_, name)) = versioned.into_iter().next() {
        return Some(name);
    }
    if unversioned {
        return Some("LLVM".to_string());
    }
    None
}
