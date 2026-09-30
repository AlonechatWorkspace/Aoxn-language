# Aoxn UI (`stdlib/ui.ax` + `stdlib/ui_win.ax`)

An immediate-mode GUI standard library for Aoxn, Qt-flavored. Written 100%
in Aoxn on top of raw FFI — no external crates, no C sources, no resource
files. Status: **v1, Windows (Win32 + GDI) backend complete**; X11/Cocoa
backends are planned (see Roadmap at the bottom).

```aoxn
import "../stdlib/ui_win.ax"        # the Windows backend; ui.ax comes with it

def main() -> int:
    c = ui_init("Hello", 640, 480)
    clicks = 0
    while c.open:
        c = ui_frame(c)             # pump messages, sample input, begin frame
        if c.open:
            if ui_button(c, 20, 20, 120, 34, "Click me"):
                clicks = clicks + 1
            ui_label_int(c, 20, 70, "clicks: ", clicks)
            ui_present(c)           # blit the frame
    ui_fini(c)
    return 0
```

```
aoxn run examples\ui_demo.ax -l user32 -l gdi32
```

## Why immediate mode

Aoxn has **no function pointers and no closures**, so the classic
callback/signal-slot architecture of Qt is impossible to express. The
library therefore uses the immediate-mode model (Dear ImGui style), which
fits the language exactly:

- widgets are plain functions called every frame;
- all state is owned by the application and passed in/out
  (`dark = ui_checkbox(c, x, y, "dark", dark)`);
- there is no widget tree, no handles to free, no add/remove calls.

The event loop is poll-based for the same reason: the window procedure is
literally `DefWindowProcW` (its address is obtained via `GetProcAddress`
since Aoxn cannot pass one of its own functions). Every frame the backend
drains the message queue, then polls `GetCursorPos` + `ScreenToClient`
(mouse), `GetAsyncKeyState` (buttons + a 256-key snapshot with per-frame
edge detection) and `GetClientRect` (resize). Closing the window destroys
it; `IsWindow()` goes false and the app loop ends. When idle,
`MsgWaitForMultipleObjects` caps the loop at `c.cap_ms` (default 15 ms
≈ 66 fps) so a background window costs ~0% CPU.

## Files

| File | Contents | Portable? |
|---|---|---|
| `stdlib/ui.ax` | text encoding (UTF-8 → UTF-16LE with surrogates), `ui_rgb`, widget ids, `Palette`/`UI`/`TextSize` types, light/dark palettes, heap-state accessors, per-frame bump arena (`utf16f`/`itoa10`) | yes — no platform externs, compiles/links everywhere, covered by tests on all platforms |
| `stdlib/ui_win.ax` | Win32/GDI backend: externs, `ui_init`/`ui_frame`/`ui_present`/`ui_fini`, all widgets and drawing primitives | Windows only |

The backend is a separate file (the same pattern as the web suite's
`web/sock_win.ax` / `web/sock_posix.ax`): it imports `ui.ax` and merges
into one flat namespace, so programs switch backends by changing **one
import line** — widget names are identical. It must also be linked
explicitly: **`-l user32 -l gdi32`**.

## API reference

### Lifecycle

| Function | Signature | Notes |
|---|---|---|
| `ui_init` | `(title: string, w: int, h: int) -> UI` | DPI-aware window with exactly `w×h` client area; `c.open == false` on failure. Calls `FreeConsole()` (console-subsystem exes would otherwise keep a console window) |
| `ui_frame` | `(c: UI) -> UI` | pump + input sampling + background fill; **returns the updated context** — assign it: `c = ui_frame(c)` |
| `ui_present` | `(c: UI)` | blit the frame (call once, after widgets) |
| `ui_close` | `(c: UI)` | request close (e.g. Esc) |
| `ui_fini` | `(c: UI)` | release window/DC/bitmap/heap state |
| `ui_alert` | `(text: string, caption: string)` | blocking message box |

The canonical loop:

```aoxn
while c.open:
    c = ui_frame(c)
    if c.open:
        ...widgets...
        ui_present(c)
ui_fini(c)
```

### Widgets (call every frame)

| Function | Signature | Behavior |
|---|---|---|
| `ui_button` | `(c, x, y, w, h, s) -> bool` | true on the release frame (press + release inside). Hover = accent border, press = darker face + 1px text shift |
| `ui_checkbox` | `(c, x, y, s, checked) -> bool` | returns the new checked state; the caller stores it |
| `ui_slider` | `(c, x, y, w, value, vmin, vmax) -> int` | horizontal; drag continues outside the track (press-claim); returns the (possibly dragged) value |
| `ui_progress` | `(c, x, y, w, value, vmax)` | groove + accent fill |
| `ui_label` / `ui_label_dim` | `(c, x, y, s)` | text in normal / dim color |
| `ui_label_int` | `(c, x, y, prefix, v)` | `prefix` + decimal value with **zero per-frame allocation** (arena `itoa10`) |
| `ui_title` | `(c, x, y, s)` | 26px bold heading |
| `ui_separator` | `(c, x, y, w)` | 1px line |
| `ui_panel` | `(c, x, y, w, h)` | filled panel |

Widgets never allocate Aoxn strings per frame: text goes through a 64 KiB
bump arena that resets each frame (`utf16f`), numbers render through
`itoa10`. A steady-state UI leaks nothing (Aoxn string concat leaks by
design; the arena is the discipline that keeps frame paths clean — same
idea as the web server's byte buffers).

### Drawing primitives

`ui_fill_rect(c,x,y,w,h,color)` · `ui_frame_rect(...)` (border) ·
`ui_draw_line(c,x1,y1,x2,y2,color)` · `ui_draw_text(c,x,y,s,color)` ·
`ui_draw_text_big(...)` · `ui_measure(c,s) -> TextSize{w,h}`.

Colors are `COLORREF`s built with `ui_rgb(r, g, b)`. The palette
(`ui_palette_get` / `ui_palette_set`, `light_palette()` / `dark_palette()`)
has 10 named colors: `bg panel text text_dim widget widget_hover
widget_down border accent groove` — set it at the top of the loop to
switch themes (see `examples/ui_demo.ax`).

### Input

- mouse: `c.mx`, `c.my` (client px), `c.dn` / `c.dn2` (left/right button
  this frame), `ui_mouse_in(c, x, y, w, h)`.
- keyboard: `ui_key_down(c, vk)`, `ui_key_pressed(c, vk)` (true only on
  the frame the key went down). `vk` is a Windows virtual key — 27 = Esc,
  13 = Enter, 32 = Space, 37..40 = arrows (no hex literals in the
  language, so VKs are used in decimal).
- `c.w`, `c.h` = client size (live), `c.frames` = frame counter,
  `c.cap_ms` = frame-cap milliseconds (assign before the loop to change).

## Design notes / implementation facts

- **Rendering**: GDI into a memory DC (double buffered). The window class
  uses `CS_OWNDC` and a `NULL` background brush, so nothing erases the
  window except us — no flicker. `ui_present` is one `BitBlt(SRCCOPY)`.
- **Fonts**: `CreateFontW`, Segoe UI 16px regular + 26px bold,
  `CLEARTYPE_QUALITY`, `SetBkMode(TRANSPARENT)`, `SetTextAlign(TA_TOP)`.
  Text is measured with `GetTextExtentPoint32W` (button centering).
- **UTF-16 everywhere**: all W-APIs receive runtime-converted UTF-16LE
  (emoji/astral planes included — the demo draws 😀). Surrogate pairs are
  encoded arithmetically (the language has no bitwise operators; every
  Win32 constant is a hand-summed decimal literal, e.g.
  `WS_OVERLAPPEDWINDOW = 13565952`, `SRCCOPY = 13369376`).
- **Resize**: detected per frame via `GetClientRect`; the memory bitmap is
  recreated on change.
- **Input robustness**: `GetAsyncKeyState` bit 15 (down now) is OR-ed with
  bit 0 (pressed since the previous call) so presses shorter than one
  frame gap are still seen; exactly one `GetAsyncKeyState` call per key
  per frame (a second call clears the latch). Mouse-button state is
  sampled only while the window is foreground.
- **Click semantics**: press edge claims the single "active" slot in the
  heap block; release fires the click only if the pointer is back inside
  the widget (standard IMGUI behavior, one drag at a time).
- **FreeConsole**: the compiler emits console-subsystem exes; `ui_init`
  detaches from the console so a GUI app doesn't keep a terminal open.
  Piped stdout (tests) keeps working. `print` output is invisible after
  that — log to a file if you need it.

## Platform matrix

| Backend | File | Status |
|---|---|---|
| Windows (Win32 + GDI) | `stdlib/ui_win.ax` | complete, tested (`tests/ui.rs`, Windows) |
| X11 | planned `stdlib/ui_x11.ax` | not started |
| Cocoa | planned | not started |

On non-Windows hosts the portable half (`ui.ax`) still compiles and its
pure logic is tested everywhere; importing the backend is a Windows-only
operation today (uncalled backend functions still reference user32/gdi32,
so a POSIX link of `ui_win.ax` fails by design until the X11 backend
lands).

## Tests

- `ui_pure_helpers_utf16_rgb_ids` (all platforms): UTF-8 → UTF-16 incl.
  surrogates, COLORREF packing, i32 assembly, widget-id injectivity,
  palettes — via a driver that imports only `stdlib/ui.ax`.
- `ui_window_selfclose_smoke` (Windows): builds a real window, runs ~90
  frames, closes itself; skips with exit code 3 where no window can be
  created. Bounded by a 60s kill-timeout in the harness.

## Known limits (v1) / roadmap

- single window per process; no menus, MDI, or child windows
- no text input widget (needs per-key unicode synthesis; roadmap), no
  scroll areas, no images
- full-window repaint each frame — fine at widget scale, not optimized
  for huge canvases
- no closing animation/timing APIs; `cap_ms` is the only pacing control
- POSIX backends (X11 first) — the `UI`/`Palette` types, palettes and
  encoding already live in the portable half so a backend only supplies
  `ui_init/ui_frame/ui_present/ui_fini` + the widget draw calls
- once the new module system lands (W1), `ui`/`ui_win` should be
  migrated like the rest of the stdlib
