//! stdlib UI tests.
//!
//! Two layers:
//! - stdlib/ui.ax (the portable half: UTF-16 encoding, COLORREF packing,
//!   widget ids, i32 reads, palettes, arena helpers) runs on EVERY platform
//!   — it declares no platform externs;
//! - stdlib/ui_win.ax (Windows backend) gets a window smoke test that
//!   creates a real window, runs a bounded frame loop and closes itself.
//!   It is Windows-only by construction: importing the backend pulls Win32
//!   references into uncalled external functions, which is exactly why the
//!   backend lives in its own file (the web suite's sock_win.ax pattern).

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn abs(rel: &str) -> String {
    // forward slashes: backslashes would read as escape sequences inside
    // the Aoxn import string literal
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(rel)
        .display()
        .to_string()
        .replace('\\', "/")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aoxn-ui-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// run an exe with a hard timeout so a wedged frame loop can never hang CI
fn run_with_timeout(exe: &PathBuf, secs: u64) -> (Option<i32>, String) {
    let mut child = Command::new(exe).stdout(std::process::Stdio::piped()).spawn().expect("spawn failed");
    let deadline = Instant::now() + Duration::from_secs(secs);
    let code = loop {
        match child.try_wait().expect("try_wait failed") {
            Some(status) => break status.code(),
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    // stdout was piped; after exit read whatever the child printed
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        let _ = stdout.read_to_string(&mut out);
    }
    (code, out)
}

#[test]
fn ui_pure_helpers_utf16_rgb_ids() {
    // exercises the platform-independent half of stdlib/ui.ax: UTF-8 ->
    // UTF-16 (ASCII, 2/3-byte and astral), COLORREF packing, i32 assembly,
    // widget id injectivity, palette structs. No window is created, so this
    // compiles and runs on every CI platform (the Win32 externs are only
    // declared; the gated call paths are folded away at O3).
    let dir = temp_dir("pure");
    let src = dir.join("ui_pure.ax");
    let exe = dir.join("ui_pure.exe");
    std::fs::write(
        &src,
        format!(
            "import \"{}\"\n\n{}",
            abs("stdlib/ui.ax"),
            r#"def main() -> int:
    print(ui_rgb(1, 2, 3))
    p = ui_utf16("A")
    print(load_u8(p, 0))
    print(load_u8(p, 1))
    p = ui_utf16("ö")
    print(load_u8(p, 0))
    p = ui_utf16("€")
    print(load_u8(p, 0) + load_u8(p, 1) * 256)
    p = ui_utf16("😀")
    print(load_u8(p, 0) + load_u8(p, 1) * 256)
    print(load_u8(p, 2) + load_u8(p, 3) * 256)
    buf = malloc(8)
    store_u8(buf, 0, 240)
    store_u8(buf, 1, 255)
    store_u8(buf, 2, 255)
    store_u8(buf, 3, 255)
    print(i32_at(buf))
    print(ui_wid_id(3, 7))
    print(light_palette().accent)
    print(dark_palette().text)
    return 0
"#
        ),
    )
    .unwrap();

    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .expect("ui pure-logic driver failed to compile");
    let (code, out) = run_with_timeout(&exe, 60);
    assert_eq!(code, Some(0), "ui pure driver exited abnormally");
    let lines: Vec<&str> = out.lines().collect();
    let expected = [
        "197121",      // rgb(1,2,3) = 1 + 2*256 + 3*65536
        "65", "0",     // "A" -> 41 00
        "246",         // U+00F6 -> F6 00
        "8364",        // U+20AC (euro) -> AC 20
        "55357",       // U+1F600 -> D83D (high surrogate)
        "56832",       //           DE00 (low surrogate)
        "-16",         // FF FF FF F0 little-endian i32
        "12884901895", // 3 * 2^32 + 7
        "14120960",    // light accent rgb(0,120,215)
        "15461355",    // dark text rgb(235,235,235) = 235 * 65793
    ];
    assert_eq!(lines.len(), expected.len(), "unexpected output: {out:?}");
    for (got, want) in lines.iter().zip(expected.iter()) {
        assert_eq!(got, want, "ui pure output mismatch");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(windows)]
#[test]
fn ui_window_selfclose_smoke() {
    // creates a real window, runs ~90 frames (~1.5s at the 15ms frame cap),
    // then closes itself cleanly. Exit code 3 = no window (headless CI
    // session) -> the test skips.
    let dir = temp_dir("selfclose");
    let src = dir.join("ui_selfclose.ax");
    let exe = dir.join("ui_selfclose.exe");
    std::fs::write(
        &src,
        format!(
            "import \"{}\"\n\n{}",
            abs("stdlib/ui_win.ax"),
            r#"def main() -> int:
    c = ui_init("ui selfclose", 320, 200)
    n = 0
    clicks = 0
    while c.open:
        c = ui_frame(c)
        if not c.open:
            break
        n = n + 1
        if ui_button(c, 12, 40, 100, 30, "noop"):
            clicks = clicks + 1
        ui_label(c, 12, 12, "self close test")
        ui_present(c)
        if n >= 90:
            ui_close(c)
    ui_fini(c)
    if c.w == 0 and c.h == 0:
        print("ui-window: none")
        return 3
    print("ui-selfclose-ok frames=" + str(n) + " clicks=" + str(clicks))
    return 0
"#
        ),
    )
    .unwrap();

    let libs: Vec<String> = vec!["user32".to_string(), "gdi32".to_string()];
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &libs, &[])
        .expect("ui selfclose driver failed to compile");
    let (code, out) = run_with_timeout(&exe, 60);
    let code = code.expect("selfclose driver timed out (killed)");
    if code == 3 {
        // no desktop/window available on this host: the driver reported and
        // skipped cleanly
        assert!(out.contains("ui-window: none"), "skip without report: {out:?}");
        eprintln!("skipped: no window could be created on this host");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    assert_eq!(code, 0, "selfclose driver failed: {out:?}");
    assert!(out.contains("ui-selfclose-ok frames="), "missing ok line: {out:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
