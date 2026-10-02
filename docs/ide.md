# The Aoxn IDE

The official editor for Aoxn, in `ide/`. A native desktop app: a Tauri 2
shell around a Next.js + Monaco workbench, with the filesystem and process
work done by a small Rust command layer.

```
ide/
├─ app/            Next.js App Router entry (layout, page, global styles)
├─ components/     Editor.tsx (Monaco), icons.tsx
├─ lib/            bridge.ts (backend calls), diagnostics.ts, monaco.ts
├─ test/           node:test suite for the output parser
├─ out/            the static export Tauri serves (build output, gitignored)
└─ src-tauri/      Rust: fsops.rs, toolchain.rs, model.rs, lib.rs
```

## Building and running

```bash
cd ide
pnpm install
pnpm ide:dev        # development, with hot reload
pnpm ide:build      # a release binary + installers
```

`pnpm build` alone produces `out/`, the static bundle. `pnpm ide:build`
compiles the Rust half on top of it (about 12 minutes cold, with LTO).

### Working on the UI without a native rebuild

```bash
pnpm dev            # http://localhost:3000
```

The workbench runs in a plain browser against an in-memory fixture folder
(`lib/bridge.ts`). Everything except the compiler is real — the explorer,
tabs, editor, output panel and quick-open all work — which makes the
visual design loop a browser refresh instead of a 12-minute Rust compile.
The Build/Run buttons answer honestly that there is no toolchain behind
them.

### Tests

```bash
pnpm test                  # the compiler-output parser (node:test)
pnpm test:rust             # the Rust command layer
pnpm typecheck             # tsc --noEmit
```

## What it does

- **Explorer** — a project tree, directories first, with `target`,
  `node_modules`, `.git` and friends skipped. Click a file to open it.
- **Editor** — Monaco with an Aoxn grammar (`lib/monaco.ts`: keywords,
  types, builtins, `f""` interpolation, `#` comments). One model per open
  file, so undo is per-file and tab switching keeps position.
- **Diagnostics** — the compiler's output becomes editor markers. Clicking
  a diagnostic line in the output panel opens the file it names and jumps
  to the line.
- **Check / Build / Run** — F7, F6, F5. They shell out to `aoxn` and show
  stdout and stderr in the output panel.
- **Quick open** — Ctrl+P for files, Ctrl+Shift+P for commands.

## Configuration

| Variable | Meaning |
|---|---|
| `AOXN_IDE_ROOT` | folder to open at launch |
| `AOXN_IDE_CC` | the `aoxn` to drive (default: `aoxn` on PATH) |
| `AOXN_CLANG` | forwarded to the compiler; the compiler needs clang |

A folder may also be passed as a command-line argument. The status bar
reports when no compiler is found, which is the single most useful thing
the IDE can say to a new user.

## Why it is built this way

**It drives the real compiler.** No build logic is reimplemented. A build
inside the IDE runs the same binary with the same arguments a user would
type, so the two cannot disagree — and anything the compiler prints,
including clang's own output, reaches the panel unchanged.

**Monaco is bundled, not fetched.** `@monaco-editor/react` defaults to a
CDN. That is correct for a website and wrong for a desktop app: the IDE has
to work offline, which is the normal condition on a locked-down machine
that compiles code. `app/page.tsx` points the loader at the bundled copy.

**The workspace is a security boundary.** Every path arriving from the
webview goes through `Workspace::resolve`, which canonicalises it and
refuses anything resolving outside the opened folder — comparing path
*components*, because a string-prefix test lets `C:\proj-evil` through a
`C:\proj` check. There is no general read-any-path command and no shell
plugin; `capabilities/default.json` grants only the window and the folder
picker. The rules are in `fsops.rs` and covered by tests.

**One Cargo workspace.** `ide/src-tauri` is excluded from the root
workspace so `cargo test` at the repo root does not compile a Tauri app.