//! The installer window: a small native Win32 wizard (`src/setup/ui.rs`).
//!
//! Aoxn ships as a single executable, so double-clicking it has to feel like a
//! normal Windows installer rather than a console log. This is a hand-rolled
//! window — no dependency, no `.rc` resource — laid out after the Python
//! installer for Windows, which most people already have in their muscle
//! memory for exactly this moment:
//!
//! 1. **Welcome** — a mark, a headline, one large primary button ("Install Now")
//!    over a secondary one ("Customize installation"), and the options as
//!    checkboxes. Nothing touches the disk until "Install Now" is clicked.
//! 2. **Progress** — the stage the installer is on, one blue bar, and a Cancel
//!    button in the same place the primary button was.
//! 3. **Done / Failed** — what happened, where it went, and Close.
//!
//! Everything is owner-drawn: the buttons, the checkboxes and the bar are all
//! painted in GDI. Standard controls cannot produce that look without a theme
//! and a manifest, and a themed progress bar is the one control that never
//! looks like anything else.
//!
//! The installation runs on a worker thread and reports over a channel; the UI
//! thread drains it on a timer, so a slow `winget` LLVM download never freezes
//! the window. `main.rs` hands over a *closure* that starts the worker, so
//! nothing is unpacked until the user says so.
//!
//! The Win32 entry points are declared here and linked against `user32`,
//! `gdi32` and `kernel32` — the same zero-external-crate rule the rest of the
//! compiler follows.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;

use super::{InstallOptions, InstallProgress};

// ---- Win32 types ------------------------------------------------------------
type HWND = *mut c_void;
type HDC = *mut c_void;
type HBRUSH = *mut c_void;
type HFONT = *mut c_void;
type HGDIOBJ = *mut c_void;
type HBITMAP = *mut c_void;
type HINSTANCE = *mut c_void;
type LPARAM = isize;
type WPARAM = usize;
type LRESULT = isize;
type DWORD = u32;

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
struct POINT {
    x: i32,
    y: i32,
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
struct TRACKMOUSEEVENT {
    cbSize: DWORD,
    dwFlags: DWORD,
    hwndTrack: HWND,
    dwHoverTime: DWORD,
    pt: POINT,
}

#[repr(C)]
struct PAINTSTRUCT {
    hdc: HDC,
    fErase: u32,
    fRestore: u32,
    fIncUpdate: u32,
    rgbReserved: [u8; 32],
}

// ---- window styles ----------------------------------------------------------
const WS_OVERLAPPED: u32 = 0x0000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;

const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;

// ---- messages ---------------------------------------------------------------
const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_CLOSE: u32 = 0x0010;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_KEYDOWN: u32 = 0x0100;
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_SETCURSOR: u32 = 0x0020;

const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;

// ---- control ids ------------------------------------------------------------
const IDC_PRIMARY: usize = 1001;
const IDC_SECONDARY: usize = 1002;
const IDC_PATH: usize = 1003;
const IDC_CLANG: usize = 1004;
const IDC_ADVANCE: usize = 1005;
const N_BUTTONS: usize = 5;

const TIMER_TICK: usize = 1;
const IDC_ARROW: u16 = 32512;
const IDC_HAND: u16 = 32649;

const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const GWL_USERDATA: i32 = -21;

const TRANSPARENT: i32 = 1;
const DT_LEFT: u32 = 0x0000;
const DT_CENTER: u32 = 0x0001;
const DT_WORDBREAK: u32 = 0x0010;
const DT_END_ELLIPSIS: u32 = 0x8000;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_NOPREFIX: u32 = 0x0800;
const TME_LEAVE: DWORD = 0x0000_0002;
const SRCCOPY: DWORD = 0x00CC_0020;
const PS_SOLID: DWORD = 0x0000_0000;

// ---- palette (BGR, as COLORREF wants it) ------------------------------------
const COL_BG: u32 = 0x00FF_FFFF; // white
const COL_TEXT: u32 = 0x0024_201C; // near-black, warm
const COL_MUTED: u32 = 0x0073_6C_67; // secondary text
const COL_ACCENT: u32 = 0x00E8_6A_17; // #176AE8
const COL_ACCENT_HOVER: u32 = 0x00C5_58_10; // #1058C5
const COL_ACCENT_DOWN: u32 = 0x00A5_4A_0D; // #0D4AA5
const COL_SECONDARY_BG: u32 = 0x00F6_F2_EE; // #EEF2F6
const COL_EDGE: u32 = 0x00D6_CEC8; // button border
const COL_TRACK: u32 = 0x00E9_E5_E1; // progress track
const COL_OK: u32 = 0x0088_3B_1C; // #1C3B88
const COL_ERR: u32 = 0x0023_2F_C5; // #C52F23

// ---- layout -----------------------------------------------------------------
const WIN_W: i32 = 640;
const WIN_H: i32 = 470;
const MARGIN: i32 = 40;
const BTN_W: i32 = 180;
const BTN_H: i32 = 38;
const CHECK_H: i32 = 28;

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
    fn SetTimer(hwnd: HWND, id: usize, ms: u32, callback: *const c_void) -> usize;
    fn KillTimer(hwnd: HWND, id: usize) -> bool;
    fn SetFocus(hwnd: HWND) -> HWND;
    fn LoadCursorW(instance: HINSTANCE, name: *const u16) -> HINSTANCE;
    fn CreateFontIndirectW(font: *const LOGFONTW) -> HFONT;
    fn SetProcessDPIAware() -> bool;
    fn GetSystemMetrics(index: i32) -> i32;
    fn AdjustWindowRectEx(rect: *mut RECT, style: DWORD, menu: bool, ex: DWORD) -> bool;
    fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
    fn BeginPaint(hwnd: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    fn EndPaint(hwnd: HWND, ps: *const PAINTSTRUCT) -> bool;
    fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> bool;
    fn TrackMouseEvent(tme: *mut TRACKMOUSEEVENT) -> bool;
    fn SetCursor(cursor: HINSTANCE) -> HINSTANCE;
    fn FillRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32;
    fn FrameRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32;
    fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: bool) -> bool;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> HBRUSH;
    fn SelectObject(dc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    fn DeleteObject(obj: HGDIOBJ) -> bool;
    fn SetTextColor(dc: HDC, color: u32) -> u32;
    fn SetBkMode(dc: HDC, mode: i32) -> i32;
    fn CreateCompatibleDC(dc: HDC) -> HDC;
    fn CreateCompatibleBitmap(dc: HDC, w: i32, h: i32) -> HBITMAP;
    fn DeleteDC(dc: HDC) -> bool;
    fn BitBlt(dst: HDC, x: i32, y: i32, w: i32, h: i32, src: HDC, sx: i32, sy: i32, rop: DWORD) -> bool;
    fn CreateRoundRectRgn(l: i32, t: i32, r: i32, b: i32, w: i32, h: i32) -> HGDIOBJ;
    fn FillRgn(dc: HDC, rgn: HGDIOBJ, brush: HBRUSH) -> i32;
    fn MoveToEx(dc: HDC, x: i32, y: i32, pt: *const POINT) -> bool;
    fn LineTo(dc: HDC, x: i32, y: i32) -> bool;
    fn DrawTextW(dc: HDC, text: *const u16, len: i32, rect: *mut RECT, flags: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
}

// ---- phases -----------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Welcome,
    Progress,
    Done,
    Failed,
}

impl Phase {
    /// The primary button means something different in every phase; the
    /// Python installer likewise has one bottom-right button whose job
    /// changes (Install -> Cancel -> Close).
    fn primary_label(self) -> &'static str {
        match self {
            Phase::Welcome => "Install Now",
            Phase::Progress => "Cancel",
            Phase::Done | Phase::Failed => "Close",
        }
    }
}

/// Visual state of one owner-drawn child. Kept in a fixed-size array indexed by
/// control id so a child window can reach its own slot without a back-pointer.
#[derive(Clone, Copy)]
struct Btn {
    hwnd: HWND,
    hover: bool,
    down: bool,
    focus: bool,
    checked: bool,
}

impl Btn {
    const fn empty() -> Btn {
        Btn { hwnd: std::ptr::null_mut(), hover: false, down: false, focus: false, checked: false }
    }
}

struct Fonts {
    head: HFONT,
    body: HFONT,
    small: HFONT,
    btn: HFONT,
}

struct UiState {
    hwnd: HWND,
    phase: Phase,
    version: String,
    options: InstallOptions,
    events: Receiver<InstallProgress>,
    cancel: &'static AtomicBool,
    start: Option<Box<dyn FnOnce(InstallOptions) -> JoinHandle<()>>>,
    worker: Option<JoinHandle<()>>,
    failed: bool,

    // welcome-page choices
    add_to_path: bool,
    install_clang: bool,
    show_advanced: bool,

    // progress-page state
    stage: String,
    percent: u32,
    log: Vec<String>,

    btns: [Btn; N_BUTTONS],
    fonts: Fonts,
}

fn slot(id: usize) -> usize {
    id - IDC_PRIMARY
}

static mut STATE: *mut UiState = std::ptr::null_mut();

fn state() -> &'static mut UiState {
    unsafe { &mut *STATE }
}

fn btn(id: usize) -> &'static mut Btn {
    let s = state();
    &mut s.btns[slot(id)]
}

/// Show the installer window. `start` performs the installation when the user
/// asks for it; returns the process exit code.
pub fn wizard(
    version: &str,
    options: &InstallOptions,
    events: Receiver<InstallProgress>,
    cancel: &'static AtomicBool,
    start: Box<dyn FnOnce(InstallOptions) -> JoinHandle<()>>,
) -> i32 {
    unsafe {
        STATE = std::ptr::null_mut();
        let _ = SetProcessDPIAware();

        STATE = Box::into_raw(Box::new(UiState {
            hwnd: std::ptr::null_mut(),
            phase: Phase::Welcome,
            version: version.to_string(),
            options: options.clone(),
            events,
            cancel,
            start: Some(start),
            worker: None,
            failed: false,
            add_to_path: true,
            install_clang: options.install_clang,
            show_advanced: false,
            stage: String::new(),
            percent: 0,
            log: Vec::new(),
            btns: [Btn::empty(); N_BUTTONS],
            fonts: Fonts {
                head: make_font(-34, 700),
                body: make_font(-15, 400),
                small: make_font(-14, 400),
                btn: make_font(-16, 600),
            },
        }));

        let instance = GetModuleHandleW(std::ptr::null());
        register_classes(instance);

        let mut rect = RECT { left: 0, top: 0, right: WIN_W, bottom: WIN_H };
        AdjustWindowRectEx(&mut rect, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, false, 0);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        let title = wide(&format!("Install Aoxn {version}"));
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            wide("AoxnSetupWindow").as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
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
            // No window means nobody can ask for an install. Fail loudly
            // rather than exiting 0 on a silent no-op.
            let s = Box::from_raw(STATE);
            STATE = std::ptr::null_mut();
            if let Some(start) = s.start {
                let _ = start(options.clone()).join();
            }
            return 2;
        }
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let s = Box::from_raw(STATE);
        STATE = std::ptr::null_mut();
        let failed = s.failed;
        if let Some(w) = s.worker {
            let _ = w.join();
        }
        if failed {
            1
        } else {
            0
        }
    }
}

unsafe fn register_classes(instance: HINSTANCE) {
    unsafe {
        // Win32 keeps `lpszClassName` for the lifetime of the registration —
        // long past this call — so the buffers must outlive the statement. A
        // `Vec` freed at end of scope leaves the class pointing at recycled
        // memory; these are three tiny allocations for the whole process.
        let name_main = wide_leaked("AoxnSetupWindow");
        let name_flat = wide_leaked("AoxnFlat");
        let name_check = wide_leaked("AoxnCheck");

        let main = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: wnd_proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), &IDC_ARROW as *const u16),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name_main,
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&main);

        let flat = WNDCLASSEXW {
            lpfnWndProc: button_proc,
            hCursor: std::ptr::null_mut(), // hover sets a hand cursor per message
            hbrBackground: std::ptr::null_mut(),
            lpszClassName: name_flat,
            ..main
        };
        RegisterClassExW(&flat);

        let check = WNDCLASSEXW {
            lpfnWndProc: check_proc,
            lpszClassName: name_check,
            ..main
        };
        RegisterClassExW(&check);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                state().hwnd = hwnd;
                create_controls(hwnd);
                SetTimer(hwnd, TIMER_TICK, 80, std::ptr::null());
                apply_phase();
                0
            }
            WM_TIMER => {
                drain_events();
                0
            }
            WM_PAINT => {
                paint(hwnd);
                0
            }
            WM_ERASEBKGND => 1, // WM_PAINT covers the whole client area
            WM_COMMAND => {
                on_command((wparam & 0xFFFF) as usize);
                0
            }
            WM_CLOSE => {
                if state().phase == Phase::Progress {
                    // Closing mid-install cancels rather than orphaning a
                    // thread that is still unpacking into the user's disk.
                    state().cancel.store(true, Ordering::SeqCst);
                }
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, TIMER_TICK);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn create_controls(parent: HWND) {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let right = WIN_W - MARGIN;
        let mk = |id: usize, class: &str, x: i32, y: i32, w: i32, h: i32| -> HWND {
            let hwnd = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                x,
                y,
                w,
                h,
                parent,
                id as HINSTANCE,
                instance,
                0,
            );
            SetWindowLongPtrW(hwnd, GWL_USERDATA, id as isize);
            hwnd
        };
        state().btns[slot(IDC_PRIMARY)].hwnd = mk(IDC_PRIMARY, "AoxnFlat", right - BTN_W, WIN_H - 64, BTN_W, BTN_H);
        state().btns[slot(IDC_SECONDARY)].hwnd = mk(IDC_SECONDARY, "AoxnFlat", right - BTN_W, WIN_H - 110, BTN_W, 32);
        state().btns[slot(IDC_PATH)].hwnd = mk(IDC_PATH, "AoxnCheck", MARGIN, WIN_H - 186, 340, CHECK_H);
        state().btns[slot(IDC_CLANG)].hwnd = mk(IDC_CLANG, "AoxnCheck", MARGIN, WIN_H - 152, 420, CHECK_H);
        state().btns[slot(IDC_ADVANCE)].hwnd = mk(IDC_ADVANCE, "AoxnCheck", MARGIN, WIN_H - 116, 200, CHECK_H);
        state().btns[slot(IDC_PATH)].checked = true;
        state().btns[slot(IDC_CLANG)].checked = state().install_clang;
    }
}

// ---- painting ---------------------------------------------------------------
unsafe fn paint(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        if dc.is_null() {
            return;
        }
        let mut rc: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);

        // Double buffered: a progress tick repaints the whole window, and
        // painting that straight to the DC flickers.
        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, w, h);
        let old_bmp = SelectObject(mem, bmp);
        let bg = CreateSolidBrush(COL_BG);
        let old_brush = SelectObject(mem, bg);
        fill_rect(mem, 0, 0, w, h, COL_BG);

        match state().phase {
            Phase::Welcome => paint_welcome(mem, w, h),
            Phase::Progress => paint_progress(mem, w, h),
            Phase::Done => paint_finished(mem, w, h, true),
            Phase::Failed => paint_finished(mem, w, h, false),
        }

        BitBlt(dc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        SelectObject(mem, old_brush);
        DeleteObject(bg);
        DeleteObject(bmp);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}

unsafe fn fill_rect(dc: HDC, l: i32, t: i32, r: i32, b: i32, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old = SelectObject(dc, brush);
        let rc = RECT { left: l, top: t, right: r, bottom: b };
        FillRect(dc, &rc, brush);
        SelectObject(dc, old);
        DeleteObject(brush);
    }
}

unsafe fn frame_rect(dc: HDC, l: i32, t: i32, r: i32, b: i32, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let rc = RECT { left: l, top: t, right: r, bottom: b };
        FrameRect(dc, &rc, brush);
        DeleteObject(brush);
    }
}

unsafe fn text(dc: HDC, font: HFONT, x: i32, y: i32, w: i32, h: i32, s: &str, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(dc, font);
        SetTextColor(dc, color);
        SetBkMode(dc, TRANSPARENT);
        let mut rc = RECT { left: x, top: y, right: x + w, bottom: y + h };
        let buf: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(dc, buf.as_ptr(), buf.len() as i32, &mut rc, flags | DT_NOPREFIX);
        SelectObject(dc, old);
    }
}

/// The mark: a rounded blue tile carrying a white "A". Drawn rather than shipped
/// as a resource so the installer stays one file with no bitmap to decode.
unsafe fn draw_logo(dc: HDC, x: i32, y: i32, size: i32) {
    unsafe {
        let brush = CreateSolidBrush(COL_ACCENT);
        let old = SelectObject(dc, brush);
        let rgn = CreateRoundRectRgn(x, y, x + size, y + size, 12, 12);
        FillRgn(dc, rgn, brush);
        DeleteObject(rgn);
        SelectObject(dc, old);
        DeleteObject(brush);

        let font = make_font(-(size * 52 / 100), 700);
        text(dc, font, x, y + size / 14, size, size - size / 7, "A", COL_BG, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        DeleteObject(font);
    }
}

unsafe fn paint_welcome(dc: HDC, w: i32, h: i32) {
    unsafe {
        let s = state();
        draw_logo(dc, MARGIN, MARGIN, 52);
        text(dc, s.fonts.head, MARGIN + 72, MARGIN + 2, 300, 40, &s.version.clone(), COL_MUTED, DT_LEFT | DT_SINGLELINE);
        text(dc, s.fonts.small, MARGIN + 74, MARGIN + 38, 300, 20, "AI-native compiled language", COL_MUTED, DT_LEFT | DT_SINGLELINE);

        let headline = format!("Install Aoxn {}", s.version);
        text(dc, s.fonts.head, MARGIN, MARGIN + 96, w - 2 * MARGIN, 44, &headline, COL_TEXT, DT_LEFT | DT_SINGLELINE);

        let body = "The compiler, the standard library, the UI toolkit and the examples, in one file.";
        text(dc, s.fonts.body, MARGIN, MARGIN + 146, w - 2 * MARGIN - 190, 48, body, COL_MUTED, DT_LEFT | DT_WORDBREAK);

        if s.show_advanced {
            let dest = format!("Destination: {}", s.options.prefix.display());
            text(dc, s.fonts.small, MARGIN, WIN_H - 250, w - 2 * MARGIN - 190, 20, &dest, COL_TEXT, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);
            let hint = "Re-run this file with -Prefix <dir> to install elsewhere.";
            text(dc, s.fonts.small, MARGIN, WIN_H - 228, w - 2 * MARGIN - 190, 36, hint, COL_MUTED, DT_LEFT | DT_WORDBREAK);
        }
        let _ = h;
    }
}

unsafe fn paint_progress(dc: HDC, w: i32, _h: i32) {
    unsafe {
        let s = state();
        draw_logo(dc, MARGIN, MARGIN + 18, 46);
        let headline = format!("Installing Aoxn {}", s.version);
        text(dc, s.fonts.head, MARGIN + 66, MARGIN + 24, w - 2 * MARGIN, 40, &headline, COL_TEXT, DT_LEFT | DT_SINGLELINE);

        let stage = if s.stage.is_empty() { String::from("Starting...") } else { s.stage.clone() };
        text(dc, s.fonts.body, MARGIN + 66, MARGIN + 66, w - 2 * MARGIN - 80, 24, &stage, COL_MUTED, DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS);

        let bx = MARGIN + 66;
        let by = MARGIN + 102;
        let bw = w - 2 * MARGIN - 106;
        draw_bar(dc, bx, by, bw, 6, s.percent);
        let pct = format!("{}%", s.percent);
        text(dc, s.fonts.small, bx, by + 16, bw, 20, &pct, COL_MUTED, DT_LEFT | DT_SINGLELINE);
    }
}

unsafe fn draw_bar(dc: HDC, x: i32, y: i32, w: i32, h: i32, percent: u32) {
    unsafe {
        fill_rect(dc, x, y, x + w, y + h, COL_TRACK);
        let filled = (w * percent.min(100) as i32) / 100;
        if filled > 0 {
            fill_rect(dc, x, y, x + filled, y + h, COL_ACCENT);
        }
    }
}

unsafe fn paint_finished(dc: HDC, w: i32, _h: i32, ok: bool) {
    unsafe {
        let s = state();
        draw_logo(dc, MARGIN, MARGIN + 14, 50);
        let headline = if ok {
            format!("Successfully installed Aoxn {}", s.version)
        } else {
            String::from("Installation failed")
        };
        let color = if ok { COL_OK } else { COL_ERR };
        text(dc, s.fonts.head, MARGIN + 70, MARGIN + 20, w - 2 * MARGIN - 70, 44, &headline, color, DT_LEFT | DT_WORDBREAK);

        let detail = if ok {
            format!(
                "Installed in {}\n\nOpen a NEW terminal so it picks up the updated PATH, then run\n    aoxn doctor",
                s.options.prefix.display()
            )
        } else {
            let last = s.log.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_default();
            format!("The install did not finish.\n\n{last}")
        };
        text(dc, s.fonts.body, MARGIN + 70, MARGIN + 84, w - 2 * MARGIN - 70, 140, &detail, COL_TEXT, DT_LEFT | DT_WORDBREAK);
    }
}

// ---- owner-drawn children ---------------------------------------------------
unsafe extern "system" fn button_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let id = GetWindowLongPtrW(hwnd, GWL_USERDATA) as usize;
        match msg {
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                paint_button(dc, id, &rc);
                EndPaint(hwnd, &ps);
                0
            }
            WM_MOUSEMOVE => {
                let b = btn(id);
                if !b.hover {
                    b.hover = true;
                    track_leave(hwnd);
                    InvalidateRect(hwnd, std::ptr::null(), false);
                }
                0
            }
            WM_MOUSELEAVE => {
                let b = btn(id);
                b.hover = false;
                b.down = false;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_LBUTTONDOWN => {
                SetFocus(hwnd);
                btn(id).down = true;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_LBUTTONUP => {
                let was = btn(id).down;
                btn(id).down = false;
                if was && inside(hwnd, lparam) {
                    on_command(id);
                }
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_KEYDOWN => {
                let key = wparam as i32;
                if key == 0x0D || key == 0x20 {
                    on_command(id);
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_SETFOCUS => {
                btn(id).focus = true;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_KILLFOCUS => {
                btn(id).focus = false;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_SETCURSOR => {
                SetCursor(LoadCursorW(std::ptr::null_mut(), &IDC_HAND as *const u16));
                1
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe extern "system" fn check_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let id = GetWindowLongPtrW(hwnd, GWL_USERDATA) as usize;
        match msg {
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                paint_check(dc, id, &rc);
                EndPaint(hwnd, &ps);
                0
            }
            WM_MOUSEMOVE => {
                btn(id).hover = true;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_MOUSELEAVE => {
                btn(id).hover = false;
                InvalidateRect(hwnd, std::ptr::null(), false);
                0
            }
            WM_LBUTTONDOWN => {
                on_command(id);
                0
            }
            WM_KEYDOWN => {
                let key = wparam as i32;
                if key == 0x0D || key == 0x20 {
                    on_command(id);
                    0
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_SETCURSOR => {
                SetCursor(LoadCursorW(std::ptr::null_mut(), &IDC_HAND as *const u16));
                1
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn paint_button(dc: HDC, id: usize, rc: &RECT) {
    unsafe {
        let s = state();
        let b = &s.btns[slot(id)];
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let primary = id == IDC_PRIMARY;
        let (bg, edge, fg) = if primary {
            let c = if b.down { COL_ACCENT_DOWN } else if b.hover { COL_ACCENT_HOVER } else { COL_ACCENT };
            (c, c, COL_BG)
        } else {
            let bg = if b.down { 0x00E2_DC_D5 } else if b.hover { COL_SECONDARY_BG } else { COL_BG };
            (bg, COL_EDGE, COL_TEXT)
        };
        fill_rect(dc, 0, 0, w, h, bg);
        frame_rect(dc, 0, 0, w, h, edge);
        if b.focus {
            frame_rect(dc, 3, 3, w - 3, h - 3, COL_ACCENT_HOVER);
        }
        let label = if primary { s.phase.primary_label() } else { "Customize installation" };
        let font = if primary { s.fonts.btn } else { s.fonts.body };
        text(dc, font, 0, 0, w, h, label, fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
}

unsafe fn paint_check(dc: HDC, id: usize, rc: &RECT) {
    unsafe {
        let s = state();
        let link = id == IDC_ADVANCE;
        let label = match id {
            IDC_PATH => "Add aoxn to PATH",
            IDC_CLANG => "Download LLVM with winget if clang is missing (slow, off by default)",
            _ => "Advanced options",
        };
        let b = &s.btns[slot(id)];
        let color = if link { COL_ACCENT } else { COL_TEXT };

        if !link {
            let side = 20;
            let top = (rc.bottom - rc.top - side) / 2;
            let edge = if b.hover { COL_ACCENT } else { COL_EDGE };
            if b.checked {
                fill_rect(dc, MARGIN, top, MARGIN + side, top + side, COL_ACCENT);
                // the tick, drawn rather than typeset: no font dependency
                let pen = CreateSolidBrush(COL_BG);
                let old = SelectObject(dc, pen);
                MoveToEx(dc, MARGIN + 5, top + 10, std::ptr::null());
                LineTo(dc, MARGIN + 9, top + 14);
                LineTo(dc, MARGIN + 15, top + 6);
                SelectObject(dc, old);
                DeleteObject(pen);
            } else {
                fill_rect(dc, MARGIN, top, MARGIN + side, top + side, COL_BG);
                frame_rect(dc, MARGIN, top, MARGIN + side, top + side, edge);
            }
            text(dc, s.fonts.body, MARGIN + side + 12, 0, rc.right - MARGIN - side - 12, rc.bottom - rc.top, label, color, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
        } else {
            let arrow = if s.show_advanced { String::from("- ") } else { String::from("+ ") };
            let full = format!("{arrow}{label}");
            text(dc, s.fonts.small, MARGIN, 0, rc.right - MARGIN, rc.bottom - rc.top, &full, color, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
        }
    }
}

// ---- behaviour --------------------------------------------------------------
fn on_command(id: usize) {
    unsafe {
        let s = state();
        match id {
            IDC_PRIMARY => match s.phase {
                Phase::Welcome => {
                    s.options.add_to_path = s.add_to_path;
                    s.options.install_clang = s.install_clang;
                    if let Some(start) = s.start.take() {
                        let opts = s.options.clone();
                        s.worker = Some(start(opts));
                    }
                    s.stage = String::from("Starting...");
                    set_phase(Phase::Progress);
                }
                Phase::Progress => {
                    s.cancel.store(true, Ordering::SeqCst);
                    s.stage = String::from("Cancelling...");
                    repaint();
                }
                Phase::Done | Phase::Failed => {
                    DestroyWindow(s.hwnd);
                }
            },
            IDC_SECONDARY => {
                s.show_advanced = !s.show_advanced;
                repaint();
            }
            IDC_PATH => {
                s.add_to_path = !s.add_to_path;
                s.btns[slot(IDC_PATH)].checked = s.add_to_path;
                repaint();
            }
            IDC_CLANG => {
                s.install_clang = !s.install_clang;
                s.btns[slot(IDC_CLANG)].checked = s.install_clang;
                repaint();
            }
            IDC_ADVANCE => {
                s.show_advanced = !s.show_advanced;
                repaint();
            }
            _ => {}
        }
    }
}

/// Show exactly the controls the current phase uses. The primary button is
/// reused across phases rather than swapped, so the user's eye stays in one
/// place.
fn set_phase(phase: Phase) {
    let s = state();
    s.phase = phase;
    let welcome = phase == Phase::Welcome;
    show(s.btns[slot(IDC_PRIMARY)].hwnd, true);
    show(s.btns[slot(IDC_SECONDARY)].hwnd, welcome);
    show(s.btns[slot(IDC_PATH)].hwnd, welcome);
    show(s.btns[slot(IDC_CLANG)].hwnd, welcome);
    show(s.btns[slot(IDC_ADVANCE)].hwnd, welcome);
    repaint();
}

fn apply_phase() {
    set_phase(state().phase);
}

fn show(hwnd: HWND, visible: bool) {
    unsafe {
        if !hwnd.is_null() {
            ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
    }
}

fn repaint() {
    unsafe {
        let hwnd = state().hwnd;
        if !hwnd.is_null() {
            InvalidateRect(hwnd, std::ptr::null(), false);
        }
    }
}

fn drain_events() {
    unsafe {
        if STATE.is_null() {
            return;
        }
        loop {
            match state().events.try_recv() {
                Ok(p) => apply(p),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }
}

fn apply(progress: InstallProgress) {
    unsafe {
        if STATE.is_null() {
            return;
        }
        let s = state();
        match progress {
            InstallProgress::Log(line) => {
                s.log.push(line);
                if s.log.len() > 400 {
                    s.log.drain(0..100);
                }
            }
            InstallProgress::Step { done, total, text } => {
                s.percent = (done * 100) / total.max(1);
                s.stage = text;
                repaint();
            }
            InstallProgress::Done { ok, message } => {
                s.failed = !ok;
                if !ok && !message.is_empty() {
                    s.stage = message;
                }
                set_phase(if ok { Phase::Done } else { Phase::Failed });
            }
        }
    }
}

// ---- helpers ----------------------------------------------------------------
fn inside(hwnd: HWND, lparam: LPARAM) -> bool {
    let x = (lparam & 0xFFFF) as i16 as i32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(hwnd, &mut rc);
    }
    x >= 0 && y >= 0 && x < rc.right && y < rc.bottom
}

unsafe fn track_leave(hwnd: HWND) {
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as DWORD,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
        pt: POINT { x: 0, y: 0 },
    };
    TrackMouseEvent(&mut tme);
}

unsafe fn make_font(height: i32, weight: i32) -> HFONT {
    let mut f = LOGFONTW {
        lfHeight: height,
        lfWeight: weight,
        lfQuality: 5, // CLEARTYPE_QUALITY
        lfOutPrecision: PS_SOLID as u8,
        ..std::mem::zeroed()
    };
    let face: Vec<u16> = "Segoe UI".encode_utf16().collect();
    for (i, c) in face.iter().take(31).enumerate() {
        f.lfFaceName[i] = *c;
    }
    CreateFontIndirectW(&f)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A NUL-terminated UTF-16 string that lives for the rest of the process, for
/// the few pointers Win32 stores rather than copies.
fn wide_leaked(s: &str) -> *const u16 {
    let mut v = wide(s).into_boxed_slice();
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}