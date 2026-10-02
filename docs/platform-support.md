# Platform support

**Aoxn targets Windows x86_64. That is the whole platform list.**

Decided in v0.30.0 (2026-10-02): one platform, one installer, one CI job,
one windowing backend. Anything that existed only to serve macOS or Linux was
removed rather than left to rot.

## What ships

| | |
|---|---|
| Compiler | `aoxn.exe` (MSVC host, `x86_64-pc-windows-msvc`) |
| Installer | `Aoxn-<version>-Setup.exe` — one file, installs everything |
| Standard library | `stdlib/stdlib.ax` + the UI toolkit (`ui.ax`, `ui_draw.ax`, `ui_win.ax`) |
| UI backend | Win32/GDI — link `-l user32 -l gdi32` |
| CI | `windows-latest`, on every push |
| External tools | clang (LLVM) for the C backend; MSVC Build Tools for linking |

## What was removed in v0.30.0

| Removed | Why |
|---|---|
| `stdlib/ui_x11.ax`, `examples/ui_probe_x11.ax` | the X11 backend existed for Linux + macOS (XQuartz) |
| `web/server_posix.ax`, `web/sock_posix.ax` | POSIX sockets/server entry points |
| `dist/install.sh`, the POSIX branch of `dist/package.sh` | one installer, Windows only |
| Linux and macOS CI jobs; the cross-platform release matrix | one artifact to verify |
| POSIX link flags (`-Wl,-rpath`, `-lm`) from `src/lib.rs` | Windows folds C math into the CRT |

## What stays, and why

- **`target_os()` is still a builtin.** User programs can fold it, so the
  language keeps the feature; on a supported build it is always `"windows"`.
  The self-hosted compiler (`selfhost/codegen.ax`) must fold it identically,
  which the fixed-point test checks.
- **`src/platform.rs` keeps its helpers** (`exe_ext`, `obj_ext`,
  `stack_link_flag`, `target_os_name`). They now have exactly one answer each,
  but call sites read as intent rather than as `cfg!` noise.
- **`clang` is still external.** The compiler emits C and shells out; the
  installer provisions it through winget and `aoxn doctor` verifies it.
- **The wiki is frozen** (`wiki/`), so pages there still describe the old
  multi-platform matrix. `docs/` is the living documentation.

## Porting back

Nothing structurally blocks it: there is no `winapi`/`os::windows` use in the
compiler core, the C emitter is plain text, and the `plat_*` primitive
contract means a second windowing backend would slot in beside `ui_win.ax`.
What a port would need: the POSIX link flags, a socket backend for the web
suite, a CI job, and an installer for that platform — roughly what v0.29.x
had.