//! The installer window: a small native Win32 wizard (`src/setup/ui.rs`).
//!
//! Aoxn ships as a single executable, so double-clicking it has to feel like a
//! normal Windows installer rather than a console log. This is a hand-rolled
//! window — no dependency, no `.rc` resource: a title, a determinate progress
//! bar, a scrolling status log and Install / Cancel buttons. The installation
//! runs on a worker thread; the UI thread drains a channel on a timer, so a
//! slow `winget` LLVM download never freezes the window.
//!
//! The Win32 entry points are declared here and linked against `user32`,
//! `gdi32`, `comctl32` and `kernel32` — the same zero-external-crate rule the
//! rest of the compiler follows.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use super::InstallProgress;

// ---- Win32 types ------------------------------------------------------------
type HWND = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HFONT = *mut c_void;
type HGDIOBJ = *mut c_void;
type HINSTANCE = *mut c_void;
type LPARAM = isize;
type WPARAM = usize;
type LRESULT = isize;
type DWORD = u32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    time: DWORD,
    pt: POINT,
}

#[repr(C)]
struct WNDCLASSEXW {
    cbSize: u32,
    style: u32,
    lpfnWndProc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    cbClsExtra: i32,
    cbWndExtra: i32,
    hInstance: HINSTANCE,
    hIcon: HINSTANCE,
    hCursor: HINSTANCE,
    hbrBackground: HBRUSH,
    lpszMenuName: *const u16,
    lpszClassName: *const u16,
    hIconSm: HINSTANCE,
}

#[repr(C)]
struct LOGFONTW {
    lfHeight: i32,
    lfWidth: i32,
    lfEscapement: i32,
    lfOrientation: i32,
    lfWeight: i32,
    lfItalic: u8,
    lfUnderline: u8,
    lfStrikeOut: u8,
    lfCharSet: u8,
    lfOutPrecision: u8,
    lfClipPrecision: u8,
    lfQuality: u8,
    lfPitchAndFamily: u8,
    lfFaceName: [u16; 32],
}

#[repr(C)]
struct INITCOMMONCONTROLSEX {
    dwSize: u32,
    dwICC: u32,
}

// ---- constants --------------------------------------------------------------
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0080;
const ES_READONLY: u32 = 0x0800;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const SS_LEFT: u32 = 0x0000;
const SW_SHOWNORMAL: i32 = 1;
const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_SETFONT: u32 = 0x0030;
// PBM_SETPOS carries the position in wParam. We deliberately keep the bar on
// its default 0..100 range instead of issuing PBM_SETRANGE: without a comctl32
// v6 manifest the control is the v5 one, whose range message differs, and a
// percentage needs no range change at all.
const PBM_SETPOS: u32 = 0x0402;
const EM_SETSEL: u32 = 0x00B1;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_SCROLLCARET: u32 = 0x00B7;
const BN_CLICKED: u16 = 0;
const IDC_INSTALL: usize = 1001;
const IDC_CANCEL: usize = 1002;
const IDC_LOG: usize = 1003;
const IDC_BAR: usize = 1004;
const IDC_TITLE: usize = 1005;
const IDC_SUBTITLE: usize = 1006;
const TIMER_TICK: usize = 1;
const ICC_PROGRESS_CLASS: u32 = 0x0000_0020;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const GWL_ID: i32 = -12;
const IDC_ARROW: u16 = 32512; // IDC_ARROW, MAKEINTRESOURCE(32512)

// COLORREF is 0x00BBGGRR
const COL_TEXT: u32 = 0x002A_2622;
const COL_MUTED: u32 = 0x0070_6A_64;
const COL_LOG_BG: u32 = 0x00FF_FF_FF;
const COL_WINDOW_BG: u32 = 0x00FA_F8F5;
const TRANSPARENT: i32 = 1;

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(classes: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex: DWORD,
        class: *const u16,
        title: *const u16,
        style: DWORD,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HINSTANCE,
        instance: HINSTANCE,
        param: LPARAM,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
    fn DestroyWindow(hwnd: HWND) -> bool;
    fn PostQuitMessage(code: i32);
    fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG) -> bool;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn ShowWindow(hwnd: HWND, cmd: i32) -> bool;
    fn SetForegroundWindow(hwnd: HWND) -> bool;
    fn SendMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
    fn SetWindowTextW(hwnd: HWND, text: *const u16) -> bool;
    fn SetTimer(hwnd: HWND, id: usize, ms: u32, callback: *const c_void) -> usize;
    fn KillTimer(hwnd: HWND, id: usize) -> bool;
    fn EnableWindow(hwnd: HWND, enable: bool) -> bool;
    fn LoadCursorW(instance: HINSTANCE, name: *const u16) -> HINSTANCE;
    fn GetDC(hwnd: HWND) -> HDC;
    fn ReleaseDC(hwnd: HWND, dc: HDC) -> i32;
    fn CreateFontIndirectW(font: *const LOGFONTW) -> HFONT;
    fn SetProcessDPIAware() -> bool;
    fn GetSystemMetrics(index: i32) -> i32;
    fn AdjustWindowRectEx(rect: *mut RECT, style: DWORD, menu: bool, ex: DWORD) -> bool;
    fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> HBRUSH;
    fn SelectObject(dc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    fn SetTextColor(dc: HDC, color: u32) -> u32;
    fn SetBkColor(dc: HDC, color: u32) -> u32;
    fn SetBkMode(dc: HDC, mode: i32) -> i32;
}

#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(icc: *const INITCOMMONCONTROLSEX) -> bool;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
}

// ---- state ------------------------------------------------------------------
struct UiState {
    log: HWND,
    bar: HWND,
    install_btn: HWND,
    cancel_btn: HWND,
    events: Receiver<InstallProgress>,
    cancel: &'static AtomicBool,
    finished: bool,
    failed: bool,
    _fonts: Vec<HFONT>,
}

/// The state exists before the window does (it owns the channel), so it is
/// built by `wizard` and handed to `WM_CREATE` through a global.
static mut STATE: *mut UiState = std::ptr::null_mut();

/// Show the installer window. `worker` performs the installation and reports
/// on the channel behind `events`; returns the process exit code.
pub fn wizard(
    version: &str,
    install_dir: &str,
    events: Receiver<InstallProgress>,
    cancel: &'static AtomicBool,
    worker: std::thread::JoinHandle<()>,
) -> i32 {
    unsafe {
        STATE = std::ptr::null_mut();
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_PROGRESS_CLASS,
        });
        let _ = SetProcessDPIAware();

        let state = Box::new(UiState {
            log: std::ptr::null_mut(),
            bar: std::ptr::null_mut(),
            install_btn: std::ptr::null_mut(),
            cancel_btn: std::ptr::null_mut(),
            events,
            cancel,
            finished: false,
            failed: false,
            _fonts: Vec::new(),
        });
        STATE = Box::into_raw(state);

        let class_name = wide("AoxnSetupWindow");
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0x0008 | 0x0002 | 0x0001, // CS_DBLCLKS | CS_HREDRAW | CS_VREDRAW
            lpfnWndProc: wnd_proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), &IDC_ARROW as *const u16),
            hbrBackground: window_brush(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&class);

        let mut rect = RECT { left: 0, top: 0, right: 640, bottom: 500 };
        AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, false, 0);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        let title = wide(&format!("Aoxn Setup {version} — {install_dir}"));
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            0,
        );
        if hwnd.is_null() {
            // no window: drop the channel (the worker stops reporting), wait
            // for it, and fail loudly
            drop(Box::from_raw(STATE));
            STATE = std::ptr::null_mut();
            let _ = worker.join();
            return 2;
        }
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let state = Box::from_raw(STATE);
        STATE = std::ptr::null_mut();
        let failed = state.failed;
        // dropping the state closes the channel; the worker is about to exit
        drop(state);
        let _ = worker.join();
        let cancelled = cancel.load(Ordering::SeqCst);
        if cancelled && !failed {
            0
        } else if failed {
            1
        } else {
            0
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                create_controls(hwnd);
                SetTimer(hwnd, TIMER_TICK, 100, std::ptr::null());
                drain_events();
                0
            }
            WM_TIMER => {
                drain_events();
                0
            }
            WM_COMMAND => {
                let id = wparam & 0xFFFF;
                let code = ((wparam >> 16) & 0xFFFF) as u16;
                if code == BN_CLICKED && !STATE.is_null() {
                    match id {
                        IDC_INSTALL => {
                            // the installation starts by itself; the button is
                            // disabled until the worker says "done", at which
                            // point it reads "Finish" and closes the window
                            let done = (*STATE).finished;
                            if done {
                DestroyWindow(hwnd);
                            }
                        }
                        IDC_CANCEL => {
                            (*STATE).cancel.store(true, Ordering::SeqCst);
                            KillTimer(hwnd, TIMER_TICK);
                            DestroyWindow(hwnd);
                        }
                        _ => {}
                    }
                }
                0
            }
            WM_CLOSE => {
                if !STATE.is_null() {
                    (*STATE).cancel.store(true, Ordering::SeqCst);
                }
                KillTimer(hwnd, TIMER_TICK);
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, TIMER_TICK);
                PostQuitMessage(0);
                0
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
                let control = lparam as HWND;
                let dc = GetDC(hwnd);
                let id = GetWindowLongPtrW(control, GWL_ID) as usize;
                match id {
                    // the log pane keeps its own background; everything else
                    // is transparent so the window's background shows through
                    IDC_LOG => {
                        SetTextColor(dc, COL_TEXT);
                        SetBkColor(dc, COL_LOG_BG);
                        let brush = log_brush();
                        SelectObject(dc, brush as HGDIOBJ);
                        ReleaseDC(hwnd, dc);
                        brush as LRESULT
                    }
                    IDC_TITLE => {
                        SetTextColor(dc, COL_TEXT);
                        SetBkMode(dc, TRANSPARENT);
                        ReleaseDC(hwnd, dc);
                        window_brush() as LRESULT
                    }
                    IDC_SUBTITLE => {
                        SetTextColor(dc, COL_MUTED);
                        SetBkMode(dc, TRANSPARENT);
                        ReleaseDC(hwnd, dc);
                        window_brush() as LRESULT
                    }
                    _ => {
                        SetTextColor(dc, COL_TEXT);
                        ReleaseDC(hwnd, dc);
                        window_brush() as LRESULT
                    }
                }
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn create_controls(parent: HWND) {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let state = &mut *STATE;
        let mut fonts: Vec<HFONT> = Vec::new();
        let mut make_font = |height: i32, weight: i32| {
            let f = CreateFontIndirectW(&logfont(height, weight));
            fonts.push(f);
            f
        };
        let title_font = make_font(-32, 700);
        let subtitle_font = make_font(-15, 400);
        let body_font = make_font(-15, 400);

        let title = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("Aoxn").as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            32,
            26,
            560,
            46,
            parent,
            IDC_TITLE as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(title, WM_SETFONT, title_font as WPARAM, 1);

        let subtitle = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("Python-style language — compiler, standard library, UI toolkit").as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            32,
            74,
            560,
            24,
            parent,
            IDC_SUBTITLE as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(subtitle, WM_SETFONT, subtitle_font as WPARAM, 1);

        let bar = CreateWindowExW(
            0,
            wide("msctls_progress32").as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE,
            32,
            110,
            576,
            22,
            parent,
            IDC_BAR as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(bar, PBM_SETPOS, 0, 0);

        let log = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            wide("EDIT").as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_BORDER | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
            32,
            146,
            576,
            262,
            parent,
            IDC_LOG as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(log, WM_SETFONT, body_font as WPARAM, 1);

        let button_font = make_font(-15, 400);
        let install_btn = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Installing…").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
            424,
            428,
            104,
            34,
            parent,
            IDC_INSTALL as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(install_btn, WM_SETFONT, button_font as WPARAM, 1);
        EnableWindow(install_btn, false);

        let cancel_btn = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide("Cancel").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
            536,
            428,
            72,
            34,
            parent,
            IDC_CANCEL as HINSTANCE,
            instance,
            0,
        );
        SendMessageW(cancel_btn, WM_SETFONT, button_font as WPARAM, 1);

        state.log = log;
        state.bar = bar;
        state.install_btn = install_btn;
        state.cancel_btn = cancel_btn;
        state._fonts = fonts;
    }
}

/// Move every queued worker message into the controls.
fn drain_events() {
    unsafe {
        if STATE.is_null() {
            return;
        }
        let state = &mut *STATE;
        loop {
            match state.events.try_recv() {
                Ok(progress) => apply(progress),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }
}

unsafe fn apply(progress: InstallProgress) {
    unsafe {
        let state = &mut *STATE;
        match progress {
            InstallProgress::Log(line) => append_line(state.log, &line),
            InstallProgress::Step { done, total, text } => {
                let pct = (done as WPARAM) * 100 / total.max(1) as WPARAM;
                SendMessageW(state.bar, PBM_SETPOS, pct, 0);
                append_line(state.log, &text);
            }
            InstallProgress::Done { ok, message } => {
                state.finished = true;
                state.failed = !ok;
                SendMessageW(state.bar, PBM_SETPOS, 100, 0); // full
                append_line(state.log, &message);
                EnableWindow(state.cancel_btn, false);
                let label = wide("Finish");
                SetWindowTextW(state.install_btn, label.as_ptr());
                EnableWindow(state.install_btn, true);
            }
        }
    }
}

/// Append one line to the read-only log control (caret to end, insert, scroll).
unsafe fn append_line(hwnd: HWND, line: &str) {
    unsafe {
        // put the caret at the end, then INSERT (not append) a line WITH its
        // newline: EM_REPLACESEL with the caret at the end silently concatenates
        // when the caller already stripped the line terminator
        SendMessageW(hwnd, EM_SETSEL, usize::MAX, LPARAM::MAX);
        let mut text: Vec<u16> = line.encode_utf16().collect();
        text.extend([13u16, 10]); // CR LF — what a Win32 EDIT expects
        text.push(0);
        SendMessageW(hwnd, EM_REPLACESEL, 1, text.as_ptr() as LPARAM);
        SendMessageW(hwnd, EM_SCROLLCARET, 0, 0);
    }
}

/// Cached GDI brushes: a Win32 brush must stay valid while Win32 paints with
/// it, and allocating one per WM_CTLCOLOR would leak on every repaint.
fn window_brush() -> HBRUSH {
    static mut BRUSH: HBRUSH = std::ptr::null_mut();
    unsafe {
        if BRUSH.is_null() {
            BRUSH = CreateSolidBrush(COL_WINDOW_BG);
        }
        BRUSH
    }
}

fn log_brush() -> HBRUSH {
    static mut BRUSH: HBRUSH = std::ptr::null_mut();
    unsafe {
        if BRUSH.is_null() {
            BRUSH = CreateSolidBrush(COL_LOG_BG);
        }
        BRUSH
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn logfont(height: i32, weight: i32) -> LOGFONTW {
    // SAFETY: LOGFONTW is a plain C struct of integers and a char array; all
    // zero is a valid value for every field.
    let mut font = LOGFONTW {
        // SAFETY: see above

        lfHeight: height,
        lfWeight: weight,
        lfCharSet: 1,  // DEFAULT_CHARSET
        lfQuality: 5,  // CLEARTYPE_QUALITY
        ..unsafe { std::mem::zeroed() }
    };
    for (i, c) in "Segoe UI".encode_utf16().take(31).enumerate() {
        font.lfFaceName[i] = c;
    }
    font
}