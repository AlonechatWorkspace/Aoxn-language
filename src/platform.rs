//! Platform abstraction (v0.26.1): centralize OS-specific decisions so
//! `lib.rs`/`main.rs`/`codegen.rs`/`build.rs` stop scattering `cfg!(windows)`
//! branches. Behavior on Windows is byte-identical to v0.26.0.

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
