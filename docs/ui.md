# Aoxn UI (`stdlib/ui.ax` + `stdlib/ui_win.ax`)

An immediate-mode GUI standard library for Aoxn, Qt-flavored. Written 100%
in Aoxn on top of raw FFI — no external crates, no C sources, no resource
files. Status: **v3 (v0.29.3), Windows (Win32 + GDI) backend complete** —
layout managers, text selection + multi-line editing with clipboard, a
focus chain, menu bar, tree/table model+view, signal-slot events, 20+
widgets, floating overlays and 16-role themes; X11/Cocoa backends are
planned (Roadmap at the bottom).

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
aoxn run examples\ui_gallery.ax -l user32 -l gdi32   # the widget gallery
aoxn run examples\ui_demo.ax -l user32 -l gdi32      # getting started
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

For anything beyond a handful of values, collect widget state in one struct
and pass it through the draw functions — the stdlib write-back idiom:

```aoxn
struct Gal:
    dark: bool
    clicks: int
    name: string

def draw(c: UI, gal: Gal) -> Gal:
    gal.dark = ui_toggle(c, 760, 16, 170, 30, "dark theme", gal.dark)
    if ui_button(c, 20, 20, 120, 34, "Click me"):
        gal.clicks = gal.clicks + 1
    te = ui_textbox(c, 20, 70, 200, 28, gal.name)
    gal.name = te.text
    return gal
```

The event loop is poll-based for the same reason: the window procedure is
literally `DefWindowProcW` (its address is obtained via `GetProcAddress`
since Aoxn cannot pass one of its own functions). Every frame the backend
drains the message queue (catching `WM_CHAR` text input and mouse-wheel
deltas), then polls `GetCursorPos` + `ScreenToClient` (mouse),
`GetAsyncKeyState` (buttons + a 256-key snapshot with per-frame edge
detection) and `GetClientRect` (resize). Closing the window destroys it;
`IsWindow()` goes false and the app loop ends. When idle,
`MsgWaitForMultipleObjects` caps the loop at `c.cap_ms` (default 15 ms
≈ 66 fps) so a background window costs ~0% CPU.

## Files

| File | Contents | Portable? |
|---|---|---|
| `stdlib/ui.ax` | text encoding (UTF-8 → UTF-16LE with surrogates), `ui_rgb`, widget ids, `Palette`/`UI`/`TextSize`/`Rect`/`TextEdit`/`ListPick` types, light/dark palettes, heap-state accessors, per-frame bump arena (`utf16f`/`itoa10`), **layout engine**, **focus chain**, disabled mode, **text-editing primitives**, WM_CHAR queue, overlay slots | yes — no platform externs, compiles/links everywhere, covered by tests on all platforms |
| `stdlib/ui_win.ax` | Win32/GDI backend: externs, `ui_init`/`ui_frame`/`ui_present`/`ui_fini`, all widgets and drawing primitives | Windows only |

The backend is a separate file (the same pattern as the web suite's
`web/sock_win.ax` / `web/sock_posix.ax`): it imports `ui.ax` and merges
into one flat namespace, so programs switch backends by changing **one
import line** — widget names are identical. It must also be linked
explicitly: **`-l user32 -l gdi32`**.

## Layout managers (Qt's QV/QH/QGridLayout counterpart)

Boxes and grids live on a stack in the shared heap block (up to 8 nested
levels); widgets keep taking explicit rectangles and the layout hands them
out. Margins and spacing come from `*_begin`; `ui_v_item`/`ui_h_item`
stretch fully across the cross axis; the `*_p` variants size the main axis
in permille (1/1000) of the box, which is how you mix fixed and flexible
rows/columns.

```aoxn
ui_hbox_begin(c, 20, 76, 600, 120, 0, 10)      # x, y, w, h, margin, spacing
r = ui_h_item_p(c, 400)                        # left column: 40%
ui_vbox_begin(c, r.x, r.y, r.w, r.h, 0, 8)     # nest a vbox inside
r2 = ui_v_item(c, 30)                          # full-width row, 30 px
if ui_button(c, r2.x, r2.y, r2.w, r2.h, "OK"): ...
ui_layout_end(c)                               # pop the vbox
r = ui_h_item_p(c, 600)                        # right column: 60%
ui_fill_rect(c, r.x, r.y, r.w, r.h, ui_rgb(90, 150, 220))
ui_layout_end(c)                               # pop the hbox
```

| Function | Signature | Behavior |
|---|---|---|
| `ui_vbox_begin` | `(c, x, y, w, h, margin, spacing)` | vertical stack; items are full inner width |
| `ui_hbox_begin` | `(c, x, y, w, h, margin, spacing)` | horizontal stack; items are full inner height |
| `ui_grid_begin` | `(c, x, y, w, h, cols, margin, spacing)` | grid with `cols` equal columns |
| `ui_layout_end` | `(c)` | pops the current box/grid |
| `ui_v_item` | `(c, h) -> Rect` | next vbox row of height `h`; cursor advances by `h + spacing` |
| `ui_h_item` | `(c, w) -> Rect` | next hbox column of width `w` |
| `ui_v_item_p` | `(c, permille) -> Rect` | like `ui_v_item`, height = permille/1000 of inner height |
| `ui_h_item_p` | `(c, permille) -> Rect` | like `ui_h_item`, width = permille/1000 of inner width |
| `ui_spacer` | `(c, px)` | fixed gap along the main axis (no extra spacing) |
| `ui_grid_row` | `(c, h)` | start a grid row of height `h` (wraps a half-finished row first) |
| `ui_grid_cell` | `(c) -> Rect` | next cell; the last column absorbs rounding so rows end exactly at the inner right edge |

## API reference

### Lifecycle

| Function | Signature | Notes |
|---|---|---|
| `ui_init` | `(title: string, w: int, h: int) -> UI` | DPI-aware window with exactly `w×h` client area; `c.open == false` on failure. Calls `FreeConsole()` (console-subsystem exes would otherwise keep a console window) |
| `ui_frame` | `(c: UI) -> UI` | pump + input sampling + background fill; **returns the updated context** — assign it: `c = ui_frame(c)` |
| `ui_present` | `(c: UI)` | draw pending overlays (combo popups, tooltips) then blit the frame (call once, after widgets) |
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

v1:

| Function | Signature | Behavior |
|---|---|---|
| `ui_button` | `(c, x, y, w, h, s) -> bool` | true on the release frame (press + release inside) or Enter/Space when focused. Hover = accent border, press = darker face + 1px text shift |
| `ui_checkbox` | `(c, x, y, s, checked) -> bool` | returns the new checked state |
| `ui_slider` | `(c, x, y, w, value, vmin, vmax) -> int` | horizontal; drag continues outside the track (press-claim); arrows step when focused |
| `ui_progress` | `(c, x, y, w, value, vmax)` | groove + accent fill |
| `ui_label` / `ui_label_dim` | `(c, x, y, s)` | text in normal / dim color |
| `ui_label_int` | `(c, x, y, prefix, v)` | `prefix` + decimal value with **zero per-frame allocation** (arena `itoa10`) |
| `ui_title` | `(c, x, y, s)` | 26px bold heading |
| `ui_separator` | `(c, x, y, w)` | 1px line |
| `ui_panel` | `(c, x, y, w, h)` | filled panel |

v2 (v0.29.2):

| Function | Signature | Behavior |
|---|---|---|
| `ui_textbox` | `(c, x, y, w, h, text) -> TextEdit` | single-line field; `TextEdit{text, changed, enter}`. Click focuses and places the caret, typing inserts (UTF-16 surrogate pairs merged), Backspace/Delete edit, arrows/Home/End move the caret (UTF-8 aware), Enter sets `enter`. Tab moves focus away |
| `ui_toggle` | `(c, x, y, w, h, s, on) -> bool` | checkable push button (accent face + selected text when on) |
| `ui_radio` | `(c, x, y, s, group, item) -> int` | exclusive choice: `group` is the group's current value, `item` this button's value; returns the new group value |
| `ui_spinbox` | `(c, x, y, w, value, vmin, vmax) -> int` | value field + stacked up/down buttons; up/down arrows step when focused |
| `ui_combobox` | `(c, x, y, w, items: [string; N], n, cur) -> int` | drop-down; press opens a floating item list (drawn on top by `ui_present`), press on an item picks it and swallows the click, press elsewhere closes. Wheel scrolls a long list |
| `ui_listbox` | `(c, x, y, w, h, items: [string; N], n, cur, scroll) -> ListPick` | scrollable single-select list; `ListPick{cur, scroll}`. Wheel scrolls; the scrollbar drags |
| `ui_tabs` | `(c, x, y, w, labels: [string; N], n, cur) -> int` | tab strip (accent top bar on the active tab); the caller draws the page content below |
| `ui_groupbox` | `(c, x, y, w, h, title)` | thin frame with the title punched through the top line |
| `ui_scroll_begin` | `(c, x, y, w, h, scroll_y, content_h) -> int` | clipped viewport; returns the new scroll offset (wheel + scrollbar). Draw content at `y - scroll` |
| `ui_scroll_end` | `(c)` | **required** — pops the GDI clip stack |
| `ui_tooltip` | `(c, x, y, w, h, text)` | shows `text` near the cursor after hovering the rect ~0.55 s (drawn on top by `ui_present`) |

Widgets never allocate Aoxn strings per frame: text goes through a 64 KiB
bump arena that resets each frame (`utf16f`), numbers render through
`itoa10`, and the caret position is cached on (id, caret, length) so an
idle focused text box costs zero allocations. A steady-state UI leaks
nothing (Aoxn string concat leaks by design; the arena is the discipline
that keeps frame paths clean — same idea as the web server's byte buffers).
Text *editing* allocates a fresh string per keystroke, the same cost model
as `+`.

### Keyboard focus

Interactive widgets register their id every frame — **call order is tab
order**. The backend rotates the focus ring with Tab / Shift+Tab against
the previous frame's registry (Qt's focus chain model), draws an accent
ring around the focused widget, and Enter/Space activates buttons, toggles,
checkboxes and radios. Clicking a widget focuses it. Slider arrows and
spin-box arrows work on the focused widget. Disabled widgets drop out of
the chain.

### Disabled mode (Qt's setEnabled)

```aoxn
if not enable:
    ui_begin_disabled(c)
ui_button(c, x, y, w, h, "runs either way")   # drawn dimmed, ignores input
if not enable:
    ui_end_disabled(c)
```

### Theming

Colors are `COLORREF`s built with `ui_rgb(r, g, b)`. The palette
(`ui_palette_get` / `ui_palette_set`, `light_palette()` / `dark_palette()`)
has 16 named roles: `bg panel text text_dim widget widget_hover
widget_down border accent groove sel sel_text disabled_face disabled_text
tooltip_bg tooltip_text` — set it at the top of the loop to switch themes
(see `examples/ui_gallery.ax`). Custom themes are ordinary `Palette`
values; fill every field (construction requires all of them).

### Drawing primitives

`ui_fill_rect(c,x,y,w,h,color)` · `ui_frame_rect(...)` (border) ·
`ui_draw_line(c,x1,y1,x2,y2,color)` · `ui_draw_text(c,x,y,s,color)` ·
`ui_draw_text_big(...)` · `ui_measure(c,s) -> TextSize{w,h}`.

### Input

- mouse: `c.mx`, `c.my` (client px), `c.dn` / `c.dn2` (left/right button
  this frame), `ui_mouse_in(c, x, y, w, h)`, `c.wheel` / `ui_wheel(c)`
  (accumulated wheel delta this frame; 120 per notch).
- keyboard: `ui_key_down(c, vk)`, `ui_key_pressed(c, vk)` (true only on
  the frame the key went down). `vk` is a Windows virtual key — 8 =
  Backspace, 9 = Tab, 13 = Enter, 27 = Esc, 32 = Space, 35..40 =
  Home/End/arrows (no hex literals in the language, so VKs are decimal).
- text input: `ui_char_count(c)` / `ui_char_str(c)` expose the UTF-16
  units queued from `WM_CHAR` this frame as a UTF-8 string (you normally
  use `ui_textbox` instead).
- `c.w`, `c.h` = client size (live), `c.frames` = frame counter,
  `c.cap_ms` = frame-cap milliseconds (assign before the loop to change).

### Overlays

Drop-down popups and tooltips are recorded during widget calls and drawn
by `ui_present` **after** every widget, so they float above the layout
regardless of call order. While a combo popup is open, presses anywhere
(even on widgets drawn earlier in the frame) are swallowed — standard menu
behavior. A tooltip re-arms every frame while its rect is hovered.

## v3 (v0.29.3): selection, multi-line editing, menus, model/view, signals

### Text selection & the clipboard

A selection is a `Sel{anchor, caret}` pair of byte offsets (equal = just a
caret). `ui_textbox` and `ui_textedit` share the same editing core, all
pure functions in `ui.ax` (tested on every platform):

| Function | Behavior |
|---|---|
| `sel_make(caret)` / `sel_active(s)` / `sel_lo(s)` / `sel_hi(s)` | selection math |
| `edit_type(s, anchor, caret, ins) -> MEdit` | insert, or replace the selection if one exists |
| `edit_backspace` / `edit_delete` | delete the selection, or one codepoint before/after the caret |
| `edit_left/right/up/down` / `edit_home/end` | UTF-8-aware movement; `shift` extends, otherwise the selection collapses; `whole` (Ctrl) jumps to doc start/end |

Both widgets handle **Shift+arrows / mouse drag** to select, **Ctrl+A**
select all, **Ctrl+C/X/V** copy/cut/paste via the Win32 clipboard
(`ui_clip_get`/`ui_clip_set`, CF_UNICODETEXT). Rendering a selection uses
`ui_measure_sub` / `ui_draw_text_sub`, which operate on a byte range
through the per-frame arena — no substring is built, so a selected field
still allocates nothing per frame.

### Multi-line editor

`ui_textedit(c, x, y, w, h, text, anchor, caret, scroll) -> EditView{text,
anchor, caret, changed, scroll}` — Enter inserts a newline, arrows move by
line (up/down), Home/End hit the line ends, the wheel + scrollbar scroll
lines. `scroll` is the first visible line (app-owned, like `ListPick`).
Line boundaries come from a cached line-start table in the heap block
(`lines_sync` rebuilds it only when the text length or owner changes).

### Menus (Qt's QMenuBar)

```aoxn
bar = ui_menubar(c, 0, 0, c.w, ["File", "Edit", "Help"], 3, open)
if bar.open == 0:
    mp = ui_menu(c, bar, ["New", "Open", "Save", "Exit"], 4)
    open = mp.open
    if mp.pick == 3: ui_close(c)
```

`ui_menubar` returns the open title's rect + state (hovering a title while
a menu is open switches menus; outside click / Esc closes). `ui_menu`
draws the open menu's items as a floating overlay (via `ui_present`) and
returns `MenuPick{open, pick}`.

### Model/view without interfaces (tree & table)

Qt's model/view shape adapted to a language with no interfaces: a model is
a **heap block behind a one-field struct**, so views can mutate it in place
(no value-copy write-back). Getters for unset cells return `""`.

| Model | Construction / access | View |
|---|---|---|
| `TableModel` | `table_model_new(rows, cols)`, `tm_set/tm_get`, `tm_set_header/tm_header`, `tm_set_colw/tm_colw`, `tm_rows/tm_cols` | `ui_table(c, x, y, w, h, m, sel_row, scroll) -> TableRet` |
| `TreeModel` | `tree_model_new(n)`, `tree_set_label/tree_label`, `tree_set_parent/tree_parent`, `tree_set_expanded/tree_expanded`, `tree_depth/tree_visible` | `ui_tree(c, x, y, w, h, m, sel, scroll) -> TreeRet` |

`ui_tree` toggles a node's expansion **directly on the model** when the
+/- is clicked; `ui_table` draws header + rows with per-column widths
(explicit via `tm_set_colw`, else equal shares). Both are focusable and
support wheel/scrollbar + arrow-key selection.

### Signal-slot without function pointers

The language has no callbacks, so signals are **integer channels**: emitters
name a signal id, a connection table picks a slot id, and the app drains one
queue with a single `switch` on `ev.slot` — the decoupling Qt gets from
signals/slots, minus the callables.

```aoxn
ui_connect(c, SIG_SAVE, SLOT_FILE)         # rewire at runtime by re-calling
if ui_button(c, x, y, w, h, "Save"):
    ui_emit(c, SIG_SAVE, EV_CLICK, 0, 0)   # emitter names a signal, not a handler
...
while i < ui_event_count(c):               # ONE dispatch site
    ev = ui_event(c, i)
    if ev.slot == SLOT_FILE: ...
```

`Ev{slot, sig, kind, a, b}`; the queue resets every `ui_frame` and holds 32
events (overflow drops). `ui_slot_of(signal)` reports the current binding.

## Design notes / implementation facts

- **Rendering**: GDI into a memory DC (double buffered). The window class
  uses `CS_OWNDC` and a `NULL` background brush, so nothing erases the
  window except us — no flicker. `ui_present` draws overlays, then one
  `BitBlt(SRCCOPY)`.
- **Fonts**: `CreateFontW`, Segoe UI 16px regular + 26px bold,
  `CLEARTYPE_QUALITY`, `SetBkMode(TRANSPARENT)`, `SetTextAlign(TA_TOP)`.
  Text is measured with `GetTextExtentPoint32W` (button centering, caret
  placement).
- **Stock pens**: `DC_PEN = 19`, `DC_BRUSH = 18`, `NULL_BRUSH = 5`,
  `NULL_PEN = 8`. **Do not "fix" 19 to 20** — `GetStockObject(20)` is out
  of range, `SelectObject` fails silently, and every outline disappears
  (this shipped broken in v1; v0.29.2 found it by pixel-inspecting
  rendered frames).
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
  sampled only while the window is foreground. `WM_CHAR` units below 32
  are dropped (Enter/Tab/Backspace stay key events).
- **Click semantics**: press edge claims the single "active" slot in the
  heap block; release fires the click only if the pointer is back inside
  the widget (standard IMGUI behavior, one drag at a time). Overlays claim
  the slot with a sentinel that swallows the whole click.
- **FreeConsole**: the compiler emits console-subsystem exes; `ui_init`
  detaches from the console so a GUI app doesn't keep a terminal open.
  Piped stdout (tests) keeps working. `print` output is invisible after
  that — log to a file if you need it.

## Heap block layout (`ui.ax` header is authoritative)

The context's `st` pointer addresses 1024 i64 slots: key snapshots
(4..515), palette (516..531), focus chain (536..538, registry 720..783),
caret (539, cache 532..535), disabled counter (540), WM_CHAR queue
(541..543), wheel (544), overlay record (545..555), tooltip hover
(556..557), layout stack (558, frames 560..719), popup item pointers
(784..815), textbox caret owner/anchor (816/819), and two v3 heap blocks:
the event bus at 817 (connect table + 32-event ring) and the line cache at
818 (line starts for multi-line editing). `ui_state_init` / `ui_state_free`
allocate and release it — the platform backend calls them at init/fini,
headless tests can too.

## Platform matrix

| Backend | File | Status |
|---|---|---|
| Windows (Win32 + GDI) | `stdlib/ui_win.ax` | complete, tested (`tests/ui.rs`, Windows) |
| X11 | planned `stdlib/ui_x11.ax` | not started |
| Cocoa | planned | not started |

On non-Windows hosts the portable half (`ui.ax`) still compiles and its
pure logic (layout, text editing, focus, palette) is tested everywhere;
importing the backend is a Windows-only operation today (uncalled backend
functions still reference user32/gdi32, so a POSIX link of `ui_win.ax`
fails by design until the X11 backend lands).

## Tests

- `ui_pure_helpers_utf16_rgb_ids` (all platforms): UTF-8 → UTF-16 incl.
  surrogates, COLORREF packing, i32 assembly, widget-id injectivity,
  palettes — via a driver that imports only `stdlib/ui.ax`.
- `ui_layout_text_focus_portable` (all platforms): layout engine rect math
  (vbox/hbox/grid, permille, spacers, nesting), text-editing primitives,
  UTF-8 caret boundaries, WM_CHAR queue → UTF-8, tab-order rotation,
  disabled counter, 16-role palette, overlay slots, tooltip hover timing.
- `ui_window_selfclose_smoke` (Windows): builds a real window, runs ~90
  frames, closes itself; skips with exit code 3 where no window can be
  created. Bounded by a 60s kill-timeout in the harness.
- `ui_window_v2_widgets_smoke` (Windows): textbox, radio, toggle, spin
  box, list box, combo box, tabs, group box, scroll area and tooltip in a
  real window for ~60 frames; same skip/timeout protocol.
- `ui_v3_portable` (all platforms): text selection, the line cache, edit
  operations (type-over-selection, backspace join, UTF-8 up/down with
  shift), the signal-slot event bus (connect/emit/queue/clear), and the
  heap-backed table/tree models.
- `ui_window_v3_widgets_smoke` (Windows): multi-line editor, menu bar +
  menu, tree, table and a signal-slot dispatch loop in a real window for
  ~60 frames; same skip/timeout protocol.

## Known limits (v3) / roadmap

- single window per process; no MDI or child windows
- `ui_textbox`/`ui_textedit` have no rich text; the editor has no
  undo/redo or word-wrap (long lines clip)
- `ui_menubar` supports one menu level (no nested submenus) and no
  keyboard arrow navigation inside an open menu (Esc closes)
- `ui_table` has no column resize/drag-reorder or cell editing;
  `ui_listbox`/`ui_combobox`/`ui_menu` take up to 32 items in a floating
  popup (`overlay_items_store` clamp)
- full-window repaint each frame — fine at widget scale, not optimized
  for huge canvases
- no animations/timing APIs; `cap_ms` and `GetTickCount64` (tooltip delay,
  caret blink) are the only pacing controls
- POSIX backends (X11 first) — the `UI`/`Palette`/`Rect`/`Sel`/`MEdit`
  types, palettes, layout engine, focus chain, editing primitives, event
  bus and models already live in the portable half, so a backend only
  supplies `ui_init/ui_frame/ui_present/ui_fini` + the widget draw calls
- once the new module system settles, `ui`/`ui_win` should be migrated
  like the rest of the stdlib
