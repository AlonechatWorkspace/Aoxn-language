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
            "import * from \"{}\"\n\n{}",
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
            "import * from \"{}\"\n\n{}",
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

/// v2 portable half: layout engine (vbox/hbox/grid/nesting), text editing
/// primitives, UTF-16 queue -> UTF-8, focus chain, disabled mode, the
/// 16-color palette, wheel/overlay slots and tooltip hover timing. Pure
/// logic over the shared heap block — no window, every platform.
#[test]
fn ui_layout_text_focus_portable() {
    let dir = temp_dir("v2pure");
    let src = dir.join("ui_v2pure.ax");
    let exe = dir.join("ui_v2pure.exe");
    std::fs::write(
        &src,
        format!(
            "import * from \"{}\"\n\n{}",
            abs("stdlib/ui.ax"),
            r#"def main() -> int:
    c = ui_new()
    c = ui_state_init(c)
    ui_vbox_begin(c, 10, 20, 100, 200, 4, 2)
    r = ui_v_item(c, 10)
    print(r.x)
    print(r.y)
    print(r.w)
    r = ui_v_item(c, 20)
    print(r.y)
    ui_spacer(c, 6)
    r = ui_v_item(c, 10)
    print(r.y)
    ui_layout_end(c)
    ui_hbox_begin(c, 0, 0, 1000, 50, 0, 0)
    r = ui_h_item_p(c, 400)
    print(r.w)
    r = ui_h_item_p(c, 600)
    print(r.x)
    print(r.w)
    ui_layout_end(c)
    ui_grid_begin(c, 0, 0, 100, 100, 3, 0, 2)
    ui_grid_row(c, 8)
    g = ui_grid_cell(c)
    print(g.x)
    print(g.w)
    g = ui_grid_cell(c)
    print(g.x)
    g = ui_grid_cell(c)
    print(g.w)
    g = ui_grid_cell(c)
    print(g.y)
    ui_layout_end(c)
    ui_hbox_begin(c, 0, 0, 200, 100, 0, 0)
    r = ui_h_item(c, 50)
    ui_vbox_begin(c, r.x, r.y, r.w, r.h, 0, 4)
    r2 = ui_v_item(c, 10)
    print(r2.w)
    print(r2.h)
    ui_layout_end(c)
    ui_layout_end(c)
    print(str_insert("ac", 1, "b"))
    print(str_remove("abc", 1, 2))
    print(str_sub("hello", 1, 3))
    print(caret_left("abc", 2))
    print(caret_right("abc", 1))
    print(caret_left("aé", 2))
    print(caret_right("aé", 1))
    ui_char_push(c, 65)
    ui_char_push(c, 55296)
    ui_char_push(c, 56832)
    ui_char_push(c, 66)
    s = ui_char_str(c)
    print(len(s))
    print(ui_char_count(c))
    ui_focus_reg(c, 111)
    ui_focus_reg(c, 222)
    ui_focus_reg(c, 333)
    focus_swap(c)
    ui_focus(c, 111)
    ui_focus_next(c, False)
    print(ui_focus_id(c))
    ui_focus_next(c, True)
    print(ui_focus_id(c))
    ui_focus_next(c, True)
    print(ui_focus_id(c))
    d = 0
    if ui_disabled(c):
        d = 1
    print(d)
    ui_begin_disabled(c)
    d = 0
    if ui_disabled(c):
        d = 1
    print(d)
    ui_end_disabled(c)
    d = 0
    if ui_disabled(c):
        d = 1
    print(d)
    p = ui_palette_get(c)
    print(p.sel_text)
    print(p.tooltip_bg)
    print(ui_wheel(c))
    print(ui_overlay_kind(c))
    print(ui_hover_ms(c, 7, 1000))
    print(ui_hover_ms(c, 7, 1600))
    ui_hover_reset(c)
    print(ui_hover_ms(c, 7, 1700))
    items = ["x", "y"]
    overlay_items_store(c, items, 2)
    print(overlay_item_str(c, 1))
    ui_state_free(c)
    return 0
"#
        ),
    )
    .unwrap();

    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .expect("ui v2 portable driver failed to compile");
    let (code, out) = run_with_timeout(&exe, 60);
    assert_eq!(code, Some(0), "ui v2 portable driver exited abnormally: {out:?}");
    let lines: Vec<&str> = out.lines().collect();
    let expected = [
        "14", "24", "92", // vbox item 1 (margin 4, spacing 2)
        "36",             // vbox item 2
        "64",             // vbox item 3 after a 6px spacer
        "400",            // h_item_p 400/1000 of 1000
        "400", "600",     // h_item_p 600: x after the first, its width
        "0", "32",        // grid cell 0
        "34",             // grid cell 1 (32 + spacing 2)
        "32",             // grid cell 2 (last column absorbs rounding)
        "10",             // grid cell 3 wrapped to the second row
        "50", "10",       // nested vbox inside an hbox item
        "abc",            // str_insert
        "ac",             // str_remove
        "el",             // str_sub
        "1", "2",         // caret_left / caret_right on ASCII
        "1", "3",         // caret moves land on UTF-8 boundaries (é = 2 bytes)
        "6", "4",         // WM_CHAR queue -> UTF-8: A + U+1F600 + B = 6 bytes
        "222", "111", "333", // tab chain: forward, back, back wraps
        "0", "1", "0",    // disabled nesting counter
        "16777215",       // sel_text = white
        "15793404",       // tooltip_bg = rgb(252,252,240)
        "0", "0",         // wheel + overlay slots start empty
        "0", "600", "0",  // tooltip hover timing + reset
        "y",              // overlay item pointers survive the copy
    ];
    assert_eq!(lines.len(), expected.len(), "unexpected output: {out:?}");
    for (got, want) in lines.iter().zip(expected.iter()) {
        assert_eq!(got, want, "ui v2 portable output mismatch");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// v2 widgets in a real window: textbox, radio, toggle, spin box, list box,
/// combo box, tabs, group box, scroll area and tooltip run for ~60 frames
/// and close themselves. Skips (exit 3) where no window can be created.
#[cfg(windows)]
#[test]
fn ui_window_v2_widgets_smoke() {
    let dir = temp_dir("v2smoke");
    let src = dir.join("ui_v2smoke.ax");
    let exe = dir.join("ui_v2smoke.exe");
    std::fs::write(
        &src,
        format!(
            "import * from \"{}\"\n\n{}",
            abs("stdlib/ui_win.ax"),
            r#"def main() -> int:
    c = ui_init("ui v2 smoke", 640, 480)
    items = ["one", "two", "three"]
    tabs = ["A", "B"]
    txt = "hi"
    n = 0
    while c.open:
        c = ui_frame(c)
        if not c.open:
            break
        n = n + 1
        te = ui_textbox(c, 10, 10, 200, 26, txt)
        txt = te.text
        ui_radio(c, 10, 50, "r1", 1, 0)
        ui_radio(c, 10, 76, "r2", 1, 1)
        ui_toggle(c, 10, 106, 120, 26, "t", True)
        ui_spinbox(c, 10, 142, 100, 1, 0, 5)
        ui_listbox(c, 10, 182, 180, 90, items, 3, 0, 0)
        ui_combobox(c, 10, 284, 180, items, 3, 0)
        ui_tabs(c, 10, 326, 200, tabs, 2, 0)
        ui_groupbox(c, 10, 368, 200, 80, "g")
        sc = ui_scroll_begin(c, 230, 10, 200, 150, 0, 600)
        ui_label(c, 240, 12 - sc, "x")
        ui_scroll_end(c)
        ui_tooltip(c, 230, 200, 80, 24, "tip")
        ui_present(c)
        if n >= 60:
            ui_close(c)
    ui_fini(c)
    if c.w == 0 and c.h == 0:
        print("ui-window: none")
        return 3
    print("ui-v2-ok frames=" + str(n) + " text=" + txt)
    return 0
"#
        ),
    )
    .unwrap();

    let libs: Vec<String> = vec!["user32".to_string(), "gdi32".to_string()];
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &libs, &[])
        .expect("ui v2 smoke driver failed to compile");
    let (code, out) = run_with_timeout(&exe, 60);
    let code = code.expect("v2 smoke driver timed out (killed)");
    if code == 3 {
        assert!(out.contains("ui-window: none"), "skip without report: {out:?}");
        eprintln!("skipped: no window could be created on this host");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    assert_eq!(code, 0, "v2 smoke driver failed: {out:?}");
    assert!(out.contains("ui-v2-ok frames="), "missing ok line: {out:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
