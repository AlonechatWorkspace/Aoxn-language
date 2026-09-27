//! Build script (v0.26.1): locate the LLVM install per platform and link it.
//!
//! Probe order (all platforms): `AOXN_LLVM_DIR` (legacy alias `AXON_LLVM_DIR`)
//! → `<repo>/LLVM` → platform defaults.
//!
//! Link name differs by platform:
//! - Windows: `LLVM-C` (`lib/LLVM-C.lib` + `bin/LLVM-C.dll`)
//! - Linux:   `LLVM-C` when `libLLVM-C.so` exists, else versioned `libLLVM-XX.so`
//!            (Ubuntu/Debian) or unversioned `libLLVM.so`
//! - macOS:   `LLVM` (`libLLVM.dylib`, Homebrew `opt/llvm/lib`)
//!
//! On non-Windows we also emit an rpath so the produced `aoxn` binary finds
//! `libLLVM` at runtime without `LD_LIBRARY_PATH`/`DYLD_LIBRARY_PATH`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=AOXN_LLVM_DIR");
    println!("cargo:rerun-if-env-changed=AXON_LLVM_DIR"); // legacy alias

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // ---- candidate install roots -------------------------------------------
    let mut candidates: Vec<PathBuf> = Vec::new();
    for var in ["AOXN_LLVM_DIR", "AXON_LLVM_DIR"] {
        if let Ok(d) = env::var(var) {
            if !d.is_empty() {
                candidates.push(PathBuf::from(d));
            }
        }
    }
    // repo-local toolchain layout: <repo>/LLVM
    candidates.push(manifest.join("LLVM"));
    if let Some(parent) = manifest.parent() {
        candidates.push(parent.join("LLVM"));
    }
    if target_os == "windows" {
        candidates.push(PathBuf::from(r"C:\Program Files\LLVM"));
    } else if target_os == "macos" {
        candidates.push(PathBuf::from("/opt/homebrew/opt/llvm")); // arm64 brew
        candidates.push(PathBuf::from("/usr/local/opt/llvm")); // Intel brew
    } else {
        // Linux: apt.llvm.org / distro packages install under /usr/lib/llvm-XX
        for v in (15..=23).rev() {
            candidates.push(PathBuf::from(format!("/usr/lib/llvm-{v}")));
        }
        candidates.push(PathBuf::from("/usr/lib/llvm"));
        candidates.push(PathBuf::from("/usr"));
        candidates.push(PathBuf::from("/usr/local"));
    }

    // ---- probe for a usable shared library ---------------------------------
    // Returns (install_root, lib_search_dir, link_name).
    let mut found: Option<(PathBuf, PathBuf, String)> = None;
    'outer: for root in &candidates {
        let libdirs = [root.join("lib"), root.join("lib64"), root.to_path_buf()];
        for libdir in &libdirs {
            if !libdir.is_dir() {
                continue;
            }
            if let Some(name) = probe_libdir(&target_os, libdir) {
                found = Some((root.clone(), libdir.clone(), name));
                break 'outer;
            }
        }
    }

    let (root, libdir, link_name) = match found {
        Some(t) => t,
        None => panic!(
            "LLVM not found for target OS '{target_os}'. Searched: {:?}.\n\
             Set AOXN_LLVM_DIR to an LLVM install containing the shared library \
             (Windows: lib/LLVM-C.lib + bin/LLVM-C.dll).",
            candidates
        ),
    };

    println!("cargo:rustc-link-search=native={}", libdir.display());
    println!("cargo:rustc-link-lib=dylib={link_name}");
    // keep runtime lookup working without env vars
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libdir.display());

    println!(
        "cargo:warning=aoxn: linking LLVM as '{link_name}' from {}",
        libdir.display()
    );

    // ---- Windows: stage LLVM-C.dll next to every artifact dir ---------------
    // (cargo run/test/examples then load it without PATH setup)
    if target_os == "windows" {
        let dll = root.join("bin").join("LLVM-C.dll");
        if let Ok(out) = env::var("OUT_DIR") {
            // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out
            if let Some(profile_dir) = PathBuf::from(&out).ancestors().nth(3) {
                for sub in ["", "deps", "examples"] {
                    let dir = if sub.is_empty() {
                        profile_dir.to_path_buf()
                    } else {
                        profile_dir.join(sub)
                    };
                    if dir.is_dir() {
                        let _ = fs::copy(&dll, dir.join("LLVM-C.dll"));
                    }
                }
            }
        }
    }
}

/// Does `libdir` contain a linkable LLVM library for this target OS?
/// Returns the link name to use.
fn probe_libdir(target_os: &str, libdir: &Path) -> Option<String> {
    let has = |name: &str| libdir.join(name).exists();

    if target_os == "windows" {
        // the Windows installer ships only the C API
        return if has("LLVM-C.lib") {
            Some("LLVM-C".to_string())
        } else {
            None
        };
    }

    // Non-Windows: the C API symbols live in the main LLVM shared library.
    if target_os == "macos" {
        return if has("libLLVM.dylib") {
            Some("LLVM".to_string())
        } else {
            None
        };
    }

    // Linux: prefer a C-API-specific library, then versioned, then unversioned.
    if has("libLLVM-C.so") {
        return Some("LLVM-C".to_string());
    }
    for v in (15..=23).rev() {
        if has(&format!("libLLVM-{v}.so")) {
            return Some(format!("LLVM-{v}"));
        }
    }
    if has("libLLVM.so") {
        return Some("LLVM".to_string());
    }
    None
}
