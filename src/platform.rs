//! Platform abstraction (v0.26.1): centralize OS-specific decisions so
//! `lib.rs`/`main.rs`/`codegen_c.rs` stop scattering `cfg!(windows)`
//! branches. Behavior on Windows is byte-identical to v0.26.0.
//!
//! Since v0.29.0 this module carries no LLVM surface: the compiler has no
//! LLVM dependency (the C-emitting backend + clang replaced it) and the
//! `llvm_dir_candidates`/`llvm_link_name*` probes were removed with it.

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
