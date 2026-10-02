//! Platform abstraction: the OS-specific decisions that used to be scattered
//! as `cfg!(windows)` branches across `lib.rs` / `main.rs` / `codegen_c.rs`.
//!
//! Aoxn targets **Windows only** (v0.30.0): one installer, one CI platform,
//! one windowing backend (Win32/GDI). The helpers stay so call sites read as
//! intent ("ask the platform") rather than as cfg noise — they simply have
//! exactly one answer each.
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

/// Linker flag to give the main thread 8MB of stack: the default is 1MB and
/// large stack-allocated arrays (the compiler's own allocas) overflow it.
/// MSVC's `link.exe` spells this `/STACK:<bytes>`.
pub fn stack_link_flag() -> Option<&'static str> {
    Some("-Wl,/STACK:8388608")
}

/// `true` when targeting Windows — the only supported target.
pub fn is_windows() -> bool {
    cfg!(windows)
}

/// Platform name for the `target_os()` builtin. Aoxn is a Windows-only
/// language, so user programs folding it always get "windows"; the builtin
/// stays because it is part of the language and the self-hosted compiler must
/// fold it identically (`selfhost/codegen.ax`).
pub fn target_os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else {
        "other"
    }
}
