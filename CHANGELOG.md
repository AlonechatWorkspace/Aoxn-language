# Changelog

Notable changes to the Aoxn compiler and language. Aoxn follows semver-ish
minor bumps while pre-1.0: each minor version is a language milestone.

## [Unreleased]

## [0.33.0] - 2026-10-02

Theme: **the IDE learns the package manager** — a Packages view wired to the
real `aoxn pkg` — plus the fix for the CI smoke test that v0.30.0's second
binary silently broke.

### Added
- **Packages panel in the IDE** (`ide/src-tauri/src/pkg.rs` + a sidebar
  view): the workspace's `aoxn.json` (name/version, dependencies and
  devDependencies, `aoxn.lock` presence) alongside what is unpacked in
  `aox_modules/`, with buttons for the whitelisted verbs — Init, Add,
  Install, Update, Outdated, Tree, Audit, Why, Remove. Every command runs
  the real `aoxn pkg` from the opened folder and its output reaches the
  output panel verbatim; mutating subcommands re-read the manifest and the
  explorer when they finish. **The whitelist is the security boundary**:
  publish, yank, cache, trust bootstrap and npm-import are refused in Rust
  (`pkg::ALLOWED_SUBCOMMANDS`), never merely hidden in the UI, and package
  names typed into the panel are validated on both sides (no flag-shaped
  strings, paths, or whitespace reach clap).
- **`aoxn doctor` in the IDE** (`ide_doctor`) — the status bar's "compiler
  not found" (and a new "clang not found") button run the toolchain's own
  self-check into the output panel instead of just pointing at config docs.
- The browser-mode fixture ships an `aoxn.json` + `aoxn.lock`, so the
  packages panel has something real to show in `pnpm dev`.

### Fixed
- **The CI smoke test** ("could not determine which binary to run"):
  v0.30.0 added the `aoxn-setup` bin, after which bare
  `cargo run -- run examples\hello.ax` had no default binary to pick and
  every documented command of that shape failed — the smoke test's
  `.\primes.exe` was only the visible symptom (it had never been built).
  `default-run = "aoxn"` in the root manifest restores the bare form
  everywhere (README, AGENTS.md, `web-bench.yml` included).

### Tests
- IDE Rust 22 (`cargo test --manifest-path ide/src-tauri/Cargo.toml`): six
  new tests pin the pkg surface — the whitelist (publish/yank/cache and
  friends refused), mutating-vs-readonly classification, package-name
  validation (flag/path/whitespace shapes refused), tolerant manifest
  reading (missing, broken, full), and `aox_modules/` listing.
- IDE frontend 26 (`pnpm --dir ide test`): five new tests for
  `lib/pkg.ts` — manifest normalization against junk, dependency
  formatting, and the TS name validator that mirrors the Rust one.
- Root 161, aoxn-pkg 94: unchanged pass.

## [0.32.0] - 2026-10-02

Theme: **the package manager, measured against pip and pnpm**. Aoxn had
a compiler, an IDE, a UI toolkit and a package manager — but the package
manager was the one you could not run a real project through, because
`devDependencies` was not an unsupported feature: it was a parse error.
This is that gap closed, plus five bugs found while measuring against the
tools it is meant to stand next to.

### Package manager — measured against pip and pnpm

The package manager had the right bones (PubGrub, a content-addressed
cache, three registry backends) and a real hole where a day-to-day feature
should have been: `devDependencies` was not merely unsupported, it was a
parse error. That and the bugs found along the way are this half of the
release. Reference layout for the curated registry:
[`docs/trusted-registry.md`](docs/trusted-registry.md).

#### Fixed
- **The HTTP registry backend could not download anything.** It verified
  `sha256(received bytes)` against `checksum` — but `checksum` is the
  *manifest hash* over the unpacked file tree, a completely different
  quantity. Every download failed with a bogus integrity error. The index
  now carries a separate transport digest (`tarball_sha256`, sha256 of the
  tarball bytes) which the backend checks at download; when a registry
  predates it the wire check is skipped rather than failing a good
  transfer. Integrity is not weakened either way — install still re-verifies
  the extracted tree against the manifest hash. The old test passed only
  because it had written the tarball digest into the manifest-hash field,
  which is exactly the bug it should have caught.
- **`aoxn add` silently unbound a dependency from its registry.** Both
  `add` and `update` wrote `registry: None` on the way back to the
  manifest, so a dependency added against a second source quietly moved to
  the default one. `add --registry <name>` now binds one explicitly, and
  an existing binding is preserved.
- **Workspace lockfile fingerprints collided.** The requirements
  fingerprint was keyed by bare dependency name, so two members pinning
  the same package at different ranges overwrote each other. Keys are now
  `"<owner>/<dep>"`. The value no longer embeds the declaring manifest's
  absolute directory either — moving or renaming the project used to force
  a full re-resolve every time.
- **The resolver could attach the wrong checksum.** When finishing a
  resolution it scanned its index cache for "some index whose name
  matches"; a same-named package in two registries produced the wrong
  metadata, silently. Lookups are now keyed by `(registry, name)`.
- **A fresh tarball temp file could collide** between concurrent installs
  of the same package (pid-only naming); the thread id is in the name now.

#### Added
- **`devDependencies`** — `aoxn add -D`, `remove -D`, `update -D`,
  `install --prod` / `--dev-only`. Dev and prod resolve into **one**
  lockfile with a per-package `dev` flag; `--prod` materializes only the
  closure reachable from production roots. A package named in both tables
  is production, so `--prod` cannot break a build that needs it. `lockfile_version`
  stays 1 — every added field defaults, so older lockfiles still load.
- **`overrides`** — `{"overrides": {"http": "1.4.2"}}` replaces every
  requirement the graph places on that package (pnpm `overrides` / pip
  constraints in one line). An unsatisfiable override fails loudly instead
  of falling back. A bare version is a caret range; write `=1.4.2` to pin.
- **Curated registries** — a registry may now carry `trust.json` (a review
  record per package, tiers `unreviewed` / `community` / `audited`) and
  `advisories/*.json`. `aoxn trust bootstrap <url>` wires the default
  registry, the advisory database and the trust index up from one URL;
  `aoxn trust list` shows the records; `aoxn trust check <pkg> --tier`
  is a CI gate. Install warns about packages a curated registry has no
  reviewed record for — and stays silent for registries that make no trust
  claims, because a warning that fires everywhere is one nobody reads.
- **`aoxn list` and `aoxn freeze`** — a table of what is installed
  (name/version/scope/source, `--json`) and pip's `name==version` output for
  CI baselines and diffing.
- **`aoxn audit --json --audit-level <level> --fix`** — machine-readable
  findings, a severity floor (`low`/`medium`/`high`/`critical`), and an
  automatic bump to the advisory's patched version. `--fix` only *tightens*
  a constraint that already admits the patch; it never widens a range
  someone wrote on purpose, and says so when it cannot help.
  Audit now falls back to the default registry's `advisories/` when no
  advisory source is configured — previously, an unconfigured setup made
  the feature unusable rather than merely empty.
- **`aoxn install --json`** — the install report as data (the shape pip
  calls `--report`), for CI to consume.
- **Parallel downloads** — tarballs fetch `jobs` at a time per registry,
  default 8, via `--jobs` or `AOXN_JOBS`; `--jobs 1` is strictly serial.
  Each worker gets its own forked registry handle. Progress reports once
  per fetch phase instead of per package, since the per-package step line
  rewrites one terminal row that several threads would fight over.
- **Minimum compiler versions are enforced.** `IndexVersion.aoxn` has been
  parsed by every registry backend since v0.29.0 and consulted by nobody;
  a publish records the publishing compiler as the package's floor, and
  install now refuses a package that needs a newer one — with every
  offender listed — instead of failing somewhere deep in a compile.
- `aoxn list`, `aoxn freeze` and `aoxn trust` are also direct `aoxn`
  aliases, alongside the existing ones.

#### Tests
- 94 aoxn-pkg tests, up from 48: transport digest on both paths (verified
  when the index publishes one, skipped when it does not) and an explicit
  regression that the two digests differ — the old bug was invisible
  precisely because its test made them equal; prod/dev closure partitioning
  including a dev-only transitive subtree and a cycle; owner-qualified
  fingerprints; engines enforcement; overrides in the resolver; trust tier
  parsing and `freeze`/`list` output. Full suite: 161 workspace + 94
  aoxn-pkg = **255 green**.


## [0.31.1] - 2026-10-02

Theme: **continuing the IDE** — the fixes its own changelog promised, plus
the first round of editor conveniences. Also fixes a version drift: v0.31.0
shipped with the Cargo manifests still saying `0.30.0` (`aoxn version` lied);
both manifests now say `0.31.1`.

### Added
- **`aoxn check <file.ax>`** — runs the pipeline up to codegen and prints
  only the diagnostics (`--json` supported). This is what the IDE's Check
  button drives; `aoxn c` would have printed the whole generated C program
  into the output panel. A good file exits 0 with no stdout, a bad file
  exits 1 with the usual `[type] file:line:col:` lines on stderr.
- **New file / New folder in the IDE explorer** (`ide/src-tauri`
  `ide_new_file` / `ide_new_dir` + a prompt dialog in the workbench). The
  name is relative to the selected folder, nesting works in one step
  (`src/util.ax`), a created file opens immediately, and both the frontend
  (`lib/paths.ts`) and the backend refuse to escape the workspace or clobber
  an existing entry. The command returns the refreshed tree, so the explorer
  is never a round trip behind.
- **Auto-check on save** — saving an `.ax` file in the IDE runs `aoxn check`
  and refreshes the editor markers (skipped while a manual command owns the
  toolchain, when no compiler is found, or for non-Aoxn files).
- **`ide_root` command** — the explorer now learns the open folder from the
  backend instead of inferring it from the first tree row, which made an
  empty-but-open folder read as "no folder open".

### Fixed
- **The Monaco theme race** (v0.31.0's own "first thing to do next"): the
  Aoxn language and theme are registered in the Editor's `beforeMount`,
  strictly between Monaco's load and the first model's creation; the
  page-level registration could lose to the editor's mount and leave the
  light default theme showing.
- **Ctrl+S inside the editor silently did nothing in the native app.** The
  save and cursor callbacks passed `model.uri.toString()` as the file path;
  the browser preview's `/preview/...` paths happen to survive that
  round-trip, but a Windows path does not, and the workbench's document map
  never matched. Both callbacks now pass the document's real path (the same
  identity `onChange` already used).
- **Windows paths in the IDE are now spelled `D:\proj` instead of
  `\\?\D:\proj`** — `fs::canonicalize`'s verbatim prefix is stripped
  (`fsops::pretty`) before a path reaches the tree, the compiler, or the
  log, so echoed diagnostics compare equal to tree paths again.
- Opening a file no longer force-closes the explorer sidebar.
- The output-panel Check verb now reads `aoxn check …`, and a build or run
  refreshes the explorer so the produced executable shows up.

### Tests
- Frontend (`pnpm --dir ide test`): 21 node:test cases — the compiler-output
  parser (10) plus the new prompt-validation (6) and fixture-tree (5) suites.
- IDE Rust (`cargo test --manifest-path ide/src-tauri/Cargo.toml`): 16 tests,
  adding create-file/create-dir round-trips, clobber refusal, workspace
  escape through creation, and the no-verbatim-prefix rule (Windows).
- Root suite: 161 tests, unchanged pass.


## [0.31.0] - 2026-10-02

Theme: **the Aoxn IDE** — an official editor for the language, in `ide/`.
Aoxn has had a compiler, a package manager, a UI toolkit and a web benchmark
harness, but no tool you edit code *in*. This adds one.

### Added
- **The Aoxn IDE (`ide/`)** — a native workbench built on Tauri 2 with a
  Next.js + Monaco frontend: project explorer, tabbed editing with an Aoxn
  syntax grammar, an output panel, a status bar, and one-key Check /
  Build / Run. Diagnostics from the compiler become editor markers, and a
  diagnostic line in the output panel is clickable: it opens the file it
  names and puts the caret on the offending line.
  - **It drives the real compiler.** `ide_check` / `ide_build` / `ide_run`
    shell out to the same `aoxn` a user would type and capture its output
    verbatim; nothing about the build is reimplemented, so a build inside
    the IDE cannot disagree with a build in a terminal. `AOXN_IDE_CC`
    overrides the compiler; `AOXN_CLANG` is forwarded to it.
  - **Monaco is bundled, not fetched.** `@monaco-editor/react` loads Monaco
    from a CDN by default, which is fine for a website and wrong for a
    desktop app — an IDE has to work on a machine with no network.
  - **The workspace is a boundary, not a suggestion.** Every path crossing
    from the webview is canonicalised and refused if it resolves outside
    the folder the user opened, including the sibling-directory case a
    string-prefix check lets through (`C:\proj` vs `C:\proj-evil`). There is
    no general read-any-path command and no shell plugin; see
    `ide/src-tauri/capabilities/default.json`.
  - **Browser mode.** `pnpm dev` runs the whole workbench against an
    in-memory fixture folder, so layout, theme and interaction can be
    worked on without a native rebuild. The build/run buttons say plainly
    that there is no toolchain behind them rather than pretending.
- **`ide/src-tauri` is its own Cargo workspace** (`exclude` in the root
  `Cargo.toml`): a plain `cargo test` at the repo root must not compile a
  Tauri application.

### Known limitations
- No language server, so no IntelliSense or type-aware completion. The
  Monaco features that would call one are switched OFF rather than left
  spinning — a half-configured service would report a wall of red
  squiggles for perfectly valid Aoxn.
- A program is launched with `aoxn run`, so its output goes to the output
  panel rather than to an attached console; an interactive program that
  reads stdin will not see it.
- The project tree is capped at 8000 nodes and 8 levels deep.
- The Monaco colour theme is registered in `app/page.tsx`, which races the
  editor's own mount. In practice the editor shows Monaco's light default
  until that effect lands; `registerTheme` in the Editor's `onMount` is the
  fix and is the first thing to do next.

### Tests
- 12 Rust tests (`cargo test --manifest-path ide/src-tauri/Cargo.toml`):
  workspace escape (including the sibling-directory prefix trap), sorted
  tree with dependency directories skipped, depth reporting, read/write
  round-trip, `which` PATH resolution, exit-code and output capture.
- 10 tests for the compiler-output parser (`pnpm --dir ide test`), led by
  the two cases that actually break naive parsers: a Windows drive letter
  (`C:\src\main.ax:12:9:`) must not be read as the filename, and a
  successful run must produce zero markers.

## [0.30.0] - 2026-10-02

Theme: **one file installs everything, and that file is for Windows**. The
release artifact is a single `Aoxn-<version>-Setup.exe` carrying the compiler,
the standard library, the UI toolkit and the examples; double-clicking it
installs them through a native window and proves the result. In the same
release Aoxn became a **Windows-only** language: one platform, one installer,
one CI job, one UI backend.

### Added
- **`Aoxn-<version>-Setup.exe`** — the single-file installer. The
  `aoxn-setup` stub (`src/setup/main.rs`) with a stored archive of the
  toolchain appended after its PE image (`[image][payload][u64 len]["AOXNSFX\0"]`);
  `dist/package.ps1` builds it. No dependencies, no decompressor: the archive
  is stored, so the zero-external-crate rule holds.
- **A native installer window** (`src/setup/ui.rs`) — hand-rolled Win32, no
  `.rc` resource and no GUI framework: title, determinate progress bar, a
  scrolling log, Install→Finish button. The installation runs on a worker
  thread and the UI drains a channel on a timer, so a slow winget download
  never freezes the window. `-Console` / `-Quiet` runs it headless for
  scripts and CI; `-Prefix`, `-NoClang`, `-Uninstall`, `-?` are the rest.
- **`aoxn doctor`** (`src/doctor.rs`) — reports the install root, the stdlib
  location and its files, the resolved clang and its version, the build
  cache, then compiles and runs a one-line program that imports the stdlib.
  Exit code 0 means `aoxn run` works. `--json` for scripts and agents,
  `--no-smoke` to skip the compile step.
- **`aoxn version` / `aoxn --version`** — the manifest version, without
  starting the compile pipeline.
- **`src/paths.rs`** — install-layout discovery: the toolchain root
  (`$AOXN_HOME` or the parent of the executable's `bin/`), the stdlib
  directory (`$AOXN_STDLIB`, `<root>/lib/stdlib`, `<root>/stdlib`, or the
  checkout in dev builds) and a bundled `toolchain/bin/clang`.
- **`.github/workflows/release.yml`** — a tagged `v*` builds the installer
  on windows-latest, installs it into a scratch prefix, runs `aoxn doctor`
  and a stdlib program, and attaches the exe to the release.

### Changed
- **Aoxn is Windows-only.** Removed, not deprecated: `stdlib/ui_x11.ax` and
  `examples/ui_probe_x11.ax` (the X11 backend that served Linux and macOS),
  `web/server_posix.ax` and `web/sock_posix.ax`, the POSIX link flags
  (`-Wl,-rpath`, `-lm`) in `src/lib.rs`, the Linux and macOS CI jobs, the
  cross-platform release matrix, and `dist/install.sh` /
  `dist/package.sh`. `src/platform.rs` keeps its helpers — each now has one
  answer. See `docs/platform-support.md` for what a port would need.
- **The standard library resolves by name.** `import * from "stdlib"` and
  `import * from "stdlib/ui_win"` resolve against the installed stdlib when
  no local package of that name matches. A project-local `aox_modules/<name>`
  package still wins; relative (`./`, `../`) and absolute paths still bypass
  the search entirely.
- **`examples/` import the stdlib by name** (`"stdlib"`, `"stdlib/ui_win"`),
  so the examples inside the installer run from wherever it was unpacked.
- The Windows CI job now also packages the single-file installer, installs it
  into a scratch prefix and runs `aoxn doctor` plus a stdlib program — a
  broken installer fails the build, not the next release.
- `find_clang()` also looks in `<root>/toolchain/bin` and `<root>/LLVM/bin`
  after `AOXN_CLANG` and `PATH`: a portable LLVM dropped into the install
  makes the toolchain self-contained without touching the environment.

### Docs
- **`docs/install.md`** rewritten around the single exe (English + 中文).
- **`docs/platform-support.md`** replaced: what "Windows only" means, what
  was removed and why, and what porting back would take.
- README, SECURITY, spec, UI reference, CONTRIBUTING and AGENTS regenerated
  in both languages. The historical reports (`docs/llvm-independence-report.md`,
  `docs/web-benchmark.md`) keep their old numbers with a note marking the
  parts that are no longer reproducible.

### Tests
- **`tests/install.rs`** (6): stdlib-by-name resolution from an unrelated
  directory, `AOXN_STDLIB` redirection (and that the same import fails
  without it), the bundled examples resolving by name, `doctor` in text and
  JSON form, `--no-smoke`, and `version`.
- `tests/ui.rs`: the X11 backend tests are gone with the backend; the
  neutrality and `plat_*` coverage tests now pin the single Win32 backend.
- 211 green (163 workspace + 48 aoxn-pkg).

### Notes
- The compiler is still not statically self-contained — it emits C and shells
  out to clang. On Windows the linker additionally needs the MSVC Build
  Tools, which is exactly what the installer's smoke test detects.
- The self-hosted loader (`selfhost/load.ax`) stays repo-bound and resolves
  relative paths only, so `selfhost_frontend_handles_imports` now feeds it a
  fixture whose stdlib import is rewritten to a relative path.
- The wiki is frozen since v0.29.3 and still describes the multi-platform
  matrix.

## [0.29.7] - 2026-10-02

Theme: **surface-syntax parity with Python**, batch 1. Four Python forms that
Aoxn was missing, all implemented as *parse-time desugarings* so that the type
checker, the C backend, and the self-hosted compiler keep seeing exactly the
AST they already knew. `src/ast.rs` is untouched by this release.

### Added
- **Augmented assignment: `+= -= *= /= %=`.** On every target Python allows -
  a plain name (`x += 1`), an array slot (`arr[0] += 10`), a struct field
  (`p.x += 1`). Each desugars to the plain assignment `x = x + 1`, so the
  operand rules are unchanged: `+=` on a string concatenates, and an
  augmented form on a never-bound name is an error because the expansion
  reads the name first. `x //= n` deliberately does not exist (`//` is a
  division, not an assignment).
- **`//` integer division.** Python's spelling; a synonym of `/` on two
  `int`s, which already truncates. Documented deviation: both truncate toward
  zero, so `-7 // 2` is `-3` where Python floors to `-4`.
- **Unary `+`.** The identity on a numeric operand, as in Python.
- **Chained comparison: `a < b <= c`.** Means `(a < b) and (b <= c)` and
  short-circuits like a plain `and`; each link is type-checked on its own.
  Middle operands are evaluated twice by the desugaring - observable only if a
  middle operand is a side-effecting call.

### Changed
- **`docs/spec.md` is no longer the v0.9 document.** It had drifted from the
  implementation on exactly the points that matter when checking parity: it
  still claimed `while` was "the only loop (no `for` yet)", described the
  LLVM-era pass pipeline (`AOXN_PASSES`, `AOXN_DUMP_IR`, O0 fast-isel) that
  has been dead since v0.29.0, said `extern def` could not take `string`,
  described the UI toolkit as two Windows-only files, and its grammar omitted
  arrays, indexing, field access, f-strings, and struct construction. All
  corrected; the grammar now carries `//`, unary `+` and the augmented
  assignment forms, and every intentional deviation from Python (`//`
  truncation, no implicit int/float mixing, value semantics) is labelled as a
  deviation rather than left implicit.
- The roadmap is reordered by Python-parity cost, and the entries that are
  real language design rather than syntax (`None`, `try`/`except`, `with`,
  generators, closures, classes, dict/set literals) are collected at the end,
  so the real gap to Python is visible instead of implied.
- **The self-hosted compiler learned the same four forms** (`selfhost/
  lexer.ax`, `selfhost/parser.ax`): the new tokens `//` and `+= -= *= /= %=`,
  a `dup_expr` helper (the arena AST has no shared nodes, so a chain or an
  augmented assignment must copy an operand it uses twice), and the matching
  desugarings. The byte-identical fixed point still holds.

### Tests
- Six new tests in `tests/pipeline.rs` covering the four new forms: augmented
  assignment across all three target kinds (plus its undeclared-name error),
  `//`, unary `+`, chained comparison (including that the chain really
  short-circuits past a call, and that a badly typed link is still caught),
  and a clang-free check that a chain folds to `&&` in the emitted C.

## [0.29.6] - 2026-10-02

### Added
- **X11 UI backend (`stdlib/ui_x11.ax`) — the toolkit now runs on Linux and
  macOS.** Programs switch OS with one import line: `ui_win.ax` (Win32/GDI)
  or `ui_x11.ax` (X11 + Xft). Link with `-l X11 -l Xft`; on macOS run under
  XQuartz. Every v3 widget (layout managers, text selection + multi-line
  editing, clipboard, focus chain, menus, tree/table model+view, signal-slot
  events) works unchanged on both.
- `examples/ui_probe_x11.ax`: a self-closing X11 smoke probe (used by CI).

### Changed
- **The UI toolkit is now three files instead of one.** The 1,900-line
  widget layer moved out of `ui_win.ax` into a new platform-neutral
  `stdlib/ui_draw.ax`; `ui.ax` stays the portable core. The widget layer
  declares no platform externs and never tests `target_os()` — it talks to
  the windowing system only through a documented `plat_*` primitive
  contract that each backend implements. Windows rendering and behaviour
  are unchanged (the emitted C differs only by dead `target_os()` guards
  that clang already folded away, and the `plat_*` indirection).
- `ui_clip_get`/`ui_clip_set` take the `UI` context (`ui_clip_get(c)`), which
  X11 needs to reach the display connection.
- `ui_alert` is routed through `plat_alert` (on X11 it prints, since a modal
  dialog would need a nested event loop).

### Tests
- Five new tests in `tests/ui.rs`, all of which run with **no clang and no
  display**: both backends typecheck and emit C, the widget layer is proven
  free of Win32/Xlib symbols and of `target_os()`, both backends implement
  the same `plat_*` set, and every primitive the widget layer calls exists in
  both. The first three real defects found (a stale `plat_font`, two
  private helpers wrongly named `plat_*`) came straight out of these.
- The six pre-existing runnable UI tests now skip cleanly when clang is
  absent instead of failing (matches the self-hosting tests in
  `tests/pipeline.rs`).
- CI: the Linux job installs `libx11-dev libxft-dev xvfb`, links the gallery
  against `-l X11 -l Xft` and runs the probe under `xvfb-run`; the macOS job
  installs XQuartz and links the same gallery.

## [0.29.5] - 2026-10-02

**Package manager W2 step 3: the npm bridge — a one-shot import tool**
(`aoxn npm-import`). The roadmap's open decision ("一次性导入工具 vs 注册表
代理层") is resolved for the import-tool shape: the **npm CLI is the
transport** (`npm view` / `npm pack`), so auth, https and private
registries come from the user's `.npmrc` while the crate stays TLS-free
(the zero-build-script dependency constraint forbids an HTTPS client
here — a registry proxy would need one; revisit if that ever changes).

### Added
- **`aoxn npm-import <spec>…` / `--from package.json`**
  (`crates/aoxn-pkg/src/npm.rs`): imports an Aoxn package published to any
  npm-compatible registry (npm, Verdaccio, GitHub Packages). The npm
  tarball's sha512 (`dist.integrity`) is verified before unpacking; only
  packages with an `aoxn.json` in their root are accepted — plain
  JavaScript packages are rejected with an explanation (Aoxn cannot link
  JavaScript). Imported packages land in `vendor/<name>/` and are recorded
  in `aoxn.json` as path dependencies, reusing the whole install pipeline
  (materialization shims, manifest-hash integrity). Re-import overwrites
  the vendored tree. `--from` enumerates `dependencies` +
  `devDependencies` of an npm `package.json` (ranges resolved via
  `npm view`, newest match wins). `AOXN_NPM` overrides the npm binary.
- **`docs/pkg-manager.md`** — the package manager's topical document
  (quick start, manifest/lockfile/materialization, resolution, all three
  registry backends, npm bridge, cache, security model).

### Changed
- **`vendor/` is excluded from package tarballs** like `aox_modules/` —
  vendored npm imports are local working state, not publishable content.
- Obsolete pre-beta stashes `wip-pkg-all` / `wip-pkg-2` dropped.

### Tests
- 8 new tests (`npm::`): spec parsing (incl. scoped `@scope/pkg@^1`),
  base64 sha512 vectors, vendor + path-dep recording, re-import
  overwrite, plain-JS rejection, dry-run isolation, integrity mismatch,
  `package.json` enumeration — all against in-process npm-layout
  tarballs, no network. Full suite 48/48.

## [0.29.4] - 2026-10-02

**Package manager W2 step 2: HTTP registry backend (read-only).** The
registry abstraction gains a third backend: the same
`packages/<name>/…` tree the git and dir backends use, served over plain
HTTP/1.1 — a static file server or mirror is enough to host a registry.

### Added
- **`HttpRegistry`** (`crates/aoxn-pkg/src/registry/http.rs`): `index` /
  `all_names` / `fetch_tarball` over GET; `publish` / `yank` are hard
  read-only errors (publish through the git or dir backend that owns the
  tree). Registered automatically for `http://` URLs and explicitly via
  `{"kind": "http"}` in a `aoxn.json` `registries` entry.
- **Zero-dependency HTTP/1.1 client** on `std::net::TcpStream` (the
  crate's dependency set must stay free of build scripts, which rules out
  every TLS-capable HTTP crate): `Content-Length` and `Transfer-Encoding:
  chunked` bodies, 3xx redirect following (absolute / root-relative
  `Location`), 10 s connect / 30 s IO timeouts, `Connection: close`
  request style.
- **Checksum verified at transport**: a downloaded tarball's sha256 is
  checked against the index checksum before it enters the cache — a
  truncated or tampered transfer fails at download, not at extraction.
- `names.json` (`GET {base}/packages/names.json`) replaces the directory
  listing the git/dir backends enumerate; a static registry mirror must
  generate it (it feeds the typosquat guard).
- `https://` registry URLs fail early with a message explaining the
  TLS-free design (put a local reverse proxy in front of a remote
  registry, or use the git backend).

### Tests
- 8 new tests (`registry::http`): an in-process static HTTP server
  (`std::net::TcpListener`) exercises index/names/tarball round-trips +
  cache reuse, `PackageNotFound` on 404, integrity failure on a tampered
  tarball, offline refusal, read-only publish/yank, chunked decoding,
  https rejection, URL-parse errors. Full suite 40/40.

## [0.29.3] - 2026-10-01

**UI toolkit v3: text selection, multi-line editing, menus, model/view and
signal-slot events.** Fills the v2 gap list toward Qt: the editors now do
real selection (Shift+arrows / drag / Ctrl+A/C/X/V) with clipboard, a
multi-line editor joins the single-line field, the app gets a menu bar,
tree and table views backed by heap `TableModel`/`TreeModel` objects, and
the language's missing function pointers are answered with an integer-channel
signal-slot event bus. Also: `AGENTS.md` is now a published repo file and
the wiki is frozen (no more per-session wiki updates).

### Added
- **Text selection** (`ui.ax`, portable): `Sel{anchor, caret}` byte-offset
  model; `edit_type`/`edit_backspace`/`edit_delete`/`edit_left/right/up/
  down/home/end` are pure functions (UTF-8 aware, shift extends). Both
  widgets handle Shift+arrows, mouse drag, Ctrl+A, and Ctrl+C/X/V copy/cut/
  paste via `ui_clip_get`/`ui_clip_set` (Win32 CF_UNICODETEXT).
- **Multi-line editor** `ui_textedit` (→ `EditView{text, anchor, caret,
  changed, scroll}`): Enter inserts a newline, up/down move by line, wheel +
  scrollbar scroll, caret blink; line boundaries come from a cached
  line-start table (`lines_sync`) in the heap block.
- **Menus** `ui_menubar` + `ui_menu` (→ `MenuBar`/`MenuPick`): Qt's QMenuBar
  shape — hover switches open menus, outside click / Esc closes, items draw
  as a floating overlay on top via `ui_present`.
- **Model/view without interfaces**: heap-backed `TableModel` (rows×cols +
  headers + per-column widths) and `TreeModel` (parent/expanded/label)
  feed `ui_table` / `ui_tree`. Models are pointer structs so views mutate
  them in place (tree expander toggles the model directly); unset cells
  return `""`.
- **Signal-slot without function pointers**: `ui_connect(c, signal, slot)` +
  `ui_emit(c, signal, kind, a, b)` queue `Ev{slot, sig, kind, a, b}`; the
  app drains one `switch` on `ev.slot` per frame (32-event ring, reset each
  `ui_frame`). `ui_slot_of` reports the current binding.
- **Zero-alloc selection rendering**: `ui_measure_sub` / `ui_draw_text_sub`
  measure/draw a byte range through the per-frame arena — no substring is
  built, so a focused/selected editor allocates nothing per frame.
- `examples/ui_gallery.ax` gains an Editor page, a Tree & Table page and a
  menu bar with a signal-slot dispatch demo (5 tabs total).

### Fixed
- **NULL model cells crashed the tree/table views** — a `TableModel` cell or
  `TreeModel` label never set held a NULL string pointer, and drawing it
  (`len` on NULL) segfaulted. Getters now return `""` for unset slots.

### Changed
- **`AGENTS.md` is now a published, tracked repo file** (it was gitignored
  as local session context). The commit-policy section reflects this.
- **The wiki is frozen** (user instruction): sessions no longer update
  `wiki/` pages; `docs/` remains living documentation.

### Notes
- UI tests 4 → 6 (`ui_v3_portable`, `ui_window_v3_widgets_smoke`).
- No language or compiler changes; the self-hosting fixed point is
  unaffected (the UI toolkit is not on the selfhost path).

**UI toolkit v2: the Qt-grade widget library.** `stdlib/ui.ax` +
`stdlib/ui_win.ax` grow from 10 widgets with absolute coordinates into a
Qt-flavored toolkit: layout managers, real text input, keyboard focus, 20+
widgets, floating overlays and 16-role themes. The whole change lives in
the stdlib + tests + docs — no language or compiler changes, and the
self-hosting fixed point stays byte-identical.

### Added
- **Layout engine** (`ui.ax`, portable): `ui_vbox_begin`/`ui_hbox_begin`/
  `ui_grid_begin` + `ui_v_item`/`ui_h_item`/`ui_v_item_p`/`ui_h_item_p`
  (permille stretch) + `ui_grid_row`/`ui_grid_cell`/`ui_spacer`, margins
  and spacing, up to 8 nested boxes — the Qt `QVBoxLayout`/`QHBoxLayout`/
  `QGridLayout` counterpart for immediate mode. Handed out as `Rect`s;
  widgets keep taking explicit coordinates.
- **Text input**: `ui_textbox` (single-line field with caret, click-to-
  place, Backspace/Delete, UTF-8-aware arrow/Home/End movement, Enter
  reporting via `TextEdit{text, changed, enter}`) fed by a real `WM_CHAR`
  queue (surrogate pairs merged to UTF-8). Editing primitives
  (`str_sub`/`str_insert`/`str_remove`/`caret_left`/`caret_right`) live in
  the portable half and are tested on every platform.
- **Keyboard focus chain** (Qt's focus model): widgets register per frame
  in call order, Tab/Shift+Tab rotates the chain, an accent ring marks the
  focused widget, Enter/Space activates buttons/toggles/checkboxes/radios,
  arrows step sliders and spin boxes.
- **New widgets**: `ui_toggle`, `ui_radio` (exclusive groups),
  `ui_spinbox`, `ui_combobox` (floating drop-down list), `ui_listbox`
  (scrollable single-select, `ListPick{cur, scroll}`), `ui_tabs`,
  `ui_groupbox`, `ui_scroll_begin`/`ui_scroll_end` (clipped viewport +
  wheel + draggable scrollbar), `ui_tooltip` (~0.55 s hover delay).
- **Disabled mode**: `ui_begin_disabled`/`ui_end_disabled`/`ui_disabled` —
  Qt's `setEnabled` pattern; groups draw dimmed and ignore input.
- **Wheel input**: `WM_MOUSEWHEEL` deltas are accumulated per frame
  (`c.wheel` / `ui_wheel`) and scroll lists, drop-downs and scroll areas.
- **Overlays drawn on top**: combo popups and tooltips are recorded during
  the frame and rendered by `ui_present` after every widget; while a popup
  is open the click is swallowed (menu behavior) via an active-slot
  sentinel.
- **16-role palettes**: `sel sel_text disabled_face disabled_text
  tooltip_bg tooltip_text` join the original 10 (light + dark).
- **Caret cache** (`ui_textbox`): caret x is memoized on
  (widget id, caret, length) so an idle focused field allocates nothing
  per frame (the steady-state-no-leak discipline holds).
- `examples/ui_gallery.ax` — a three-page Qt-style widget gallery built on
  the layout engine (controls / input / containers).
- Tests: `ui_layout_text_focus_portable` (all platforms: layout rect math,
  editing primitives, UTF-8 caret boundaries, char queue, tab order,
  disabled counter, palette, overlay slots, hover timing) and
  `ui_window_v2_widgets_smoke` (Windows: every v2 widget in a real window,
  bounded + self-closing). UI tests 2 → 4; suite total 175 (pipeline 99 +
  lib 6 + TS 34 + UI 4 + aoxn-pkg 32).

### Fixed
- **`DC_PEN` is stock object 19, not 20** — `GetStockObject(20)` is out of
  range so `SelectObject` failed silently and pen-only drawing
  (`ui_frame_rect`, checkbox ticks, button/slider borders, focus rings)
  never reached the bitmap (it shipped this way in v0.27.0; found by
  pixel-inspecting rendered frames in v0.29.2). All widget outlines now
  render.
- The scroll-area scrollbar drew inside its own content clip region and
  was clipped away; it is now drawn before the clip is pushed.

### Removed
- **macOS Intel (x86_64) support** — the `macos-x86_64` / `macos-13` CI job is
  dropped and Intel Macs are removed from the platform tables, the issue
  template and the docs. This restores the v0.27.1 decision (the job had
  crept back in with the v0.28.0–v0.29.0 work). Tier 2 is Linux x86_64 +
  macOS arm64; the CI matrix is three jobs.

## [0.29.1] - 2026-10-01

**Package manager W2 step 1: manifest entry resolution lands on the compiler
side.** A bare package import (`import * from "http"`) now resolves its
entry through the package's `aox_modules/<name>/aoxn.json` manifest —
`main`, `exports` (incl. `pkg/sub` subpaths), and `types` — so an installed
package whose entry is not literally `index.ax` is finally importable. This
is the compiler-side half of the W2 package-management milestone (spec
§4); `aoxn pkg` itself gained the matching `exports`/`types` manifest
fields. No language or backend change; the C-emitting pipeline is
untouched. 173 tests (pipeline 99 + lib 6 + TS 34 + UI 2 + aoxn-pkg 32).

### Added
- **`src/pkg_manifest.rs`** — a zero-dependency JSON value parser
  (hand-rolled; `src/` keeps its zero-external-crate security property) that
  reads `aox_modules/<pkg>/aoxn.json` and resolves the entry source file
  for a (sub)path. `exports` values may be a plain path string or a
  conditional object (`{ "default": "...", "types": "..." }`), mirroring
  the npm convention closely enough for Aoxn's needs.
- **`exports` and `types` fields** on the `aoxn-pkg` `Manifest`
  (`crates/aoxn-pkg/src/manifest.rs`); `entry_file()` now prefers
  `exports["."]` then `main` then the legacy probe order.
- **Subpath package imports**: `import * from "pkg/client"` resolves via
  `exports["./client"]`.
- **BOM tolerance** in the manifest reader (a PowerShell-written
  `aoxn.json` with a UTF-8 BOM no longer breaks resolution).

### Changed
- **`resolve_import`** (`src/lib.rs`): bare identifiers consult the
  package manifest first; a package without `aoxn.json` (or one whose
  manifest has no resolvable entry for the subpath) falls back to the
  legacy `aox_modules/<name>` directory probe, keeping pre-manifest
  packages working.
- **`complete_module_path`**: switched `base.exists()` → `base.is_file()`
  (and the per-extension probes likewise). A directory at `base` no longer
  short-circuits the probe, so `index.<ext>` inside a directory-named
  package/module is actually reached — a latent bug that was masked because
  every call site passed paths with an explicit extension.

### Tests
- `tests/pipeline.rs`: `pkg_manifest_main_entry_resolution`,
  `pkg_manifest_subpath_exports`, `pkg_no_manifest_falls_back_to_probe`.
- `src/pkg_manifest.rs`: 6 unit tests (main, exports subpath, conditional
  object, missing-subpath-None, no-manifest-None, BOM).
- `crates/aoxn-pkg/src/manifest.rs`: `parses_exports_and_types`,
  `exports_roundtrip_omits_empty`, `deny_unknown_fields_still_holds`.

## [0.29.0] - 2026-10-01

**LLVM independence Phase 2 complete: the LLVM dependency is gone.** The
C-emitting backend (v0.27.1's `--backend c`) is now the compiler's only
backend. `src/llvm.rs` (the hand-written LLVM-C FFI), `src/codegen.rs` (the
LLVM-IR codegen) and `build.rs` (LLVM probing and linking) are deleted, and
`cargo build` no longer searches for, links, or ships any LLVM library. What
remains is clang — the C toolchain that compiles the generated C and performs
the final link; it was always required.

### Removed
- **The LLVM backend** (`src/llvm.rs`, `src/codegen.rs`) and the LLVM
  probing/linking in `build.rs` (the build script is gone entirely;
  `Cargo.toml` no longer declares one). Building the compiler now needs only
  the Rust toolchain; compiling and running programs needs only clang.
- **`aoxn ir`** (the optimized-IR dump) — there is no IR anymore. The new
  **`aoxn c`** prints the generated C text; the old `ir` spelling is kept as
  an alias that forwards to it.
- **`AOXN_PASSES`, `AOXN_BACKEND` and `--backend llvm`** — the pass-pipeline
  override and the backend switch have nothing left to select. `--backend c`
  is still accepted (it is the default and only backend); any other value is
  rejected with a pointer at v0.29.0. `AOXN_DUMP_IR` became `AOXN_DUMP_C`.
- **`platform::llvm_dir_candidates` / `platform::llvm_link_name[_in]`** and
  the self-host tests' `-l LLVM-C -L ...` link flags.

### Changed
- **The self-hosted compiler emits C text** (`selfhost/codegen.ax` rewritten
  from an LLVM-C driver into a C emitter mirroring `codegen_c.rs`): structs
  are C structs, arrays are wrapped in single-field structs
  (`typedef struct { T data[N]; }`) for value semantics, and the self-hosted
  driver writes the C next to the object and shells out to
  `clang -O3 -w -c`. `emit_ir` became `emit_c` (writing `stdlib_use.c`
  instead of `stdlib_use.ir`), `gen_ir_text` became `gen_c_text`, and
  `gen_dispose` is gone (no handles left to dispose).
- **The self-hosting fixed point is now C-text based** and therefore no
  longer Windows-only: the stage-1 (Rust-built) and stage-2 (Aoxn-built)
  compilers must emit byte-identical C for the same program — and, because
  clang is deterministic for identical input, byte-identical objects too.
  The old skip (`C:/Program Files/LLVM/lib/LLVM-C.lib` must exist) is gone;
  the self-host tests now skip only when clang itself cannot be found.
- **Optimization levels** now select the clang `-O` level used to compile the
  generated C; the C text itself is level-independent (asserted by the new
  `c_text_is_opt_level_independent` test, which replaces the distinct-IR
  test). `--O0` no longer means "fast-isel": it means `clang -O0`.
- **CI** installs clang only: Windows keeps the winget LLVM package purely
  for its clang, Linux apt-installs `clang`, and macOS uses the preinstalled
  Apple clang — no `llvm-18-dev`, no `brew install llvm@18`, no
  `AOXN_LLVM_DIR` anywhere.

### Fixed
- **Byte-exact self-hosting off Windows** (the last open item in
  docs/platform-support.md §7): without the LLVM-C library requirement, the
  fixed-point test no longer depends on a Windows-only library path and runs
  on every Tier-1/Tier-2 platform with clang.
- **The fixed-point object comparison masks the COFF `TimeDateStamp`**
  (bytes 4–8 of every Windows object): clang stamps each object with the
  wall-clock time of its run, so two compiles of identical C never matched
  byte-for-byte and `selfhost_driver_self_compiles` failed on Windows despite
  a correct compiler. The generated-C comparison is still exact.


## [0.28.0] - 2026-09-29

### Fixed
- **POSIX link line passes `-lm`** — the TS `%` lowering calls C `fmod`
  (`__ts_mod`), and on Linux/macOS C math lives in a separate libm while the
  Windows CRT link covers it; Linux CI failed with "undefined reference to
  `fmod'". Both link paths get the flag: `src/lib.rs` `link_opts` (after the
  user `-l` list so `--as-needed` toolchains still resolve) and the
  self-hosted driver's clang command.


**TS-M1 W1 complete** — the TypeScript front end lands its type layer (S2b)
and the module system (S3), and the whole repository migrates off the legacy
`import "path"` syntax in one switch. Self-hosting fixed point (byte-identical
IR + COFF) re-verified unchanged.

### Added (S2b type layer)
- **f64 numeric tower**: `number` is double everywhere; numeric literals are
  JS numbers (f64), integer positions (indexes, lengths) convert via the new
  internal `Expr::Cast`; mixed int/float arithmetic auto-shapes literals to
  the other side's width. New **conversion builtins** `to_int(x)` /
  `to_float(x)` (Aoxn language, spec.md updated) back the tower.
- **JS bit semantics**: `& | ^ ~ << >> >>>` lower to `__ts_*` runtime helpers
  (injected Aoxn source: ToInt32 patterns, floor-shift, 32-bit wrap); `%` is
  fmod. `console.log` and template substitutions print numbers in JS form
  (`__ts_num`: integral values without `.000000`, trailing zeros trimmed) and
  `console.log(a, b)` joins space-separated like JS.
- **Unions + null narrowing**: `T | null | undefined` erases to `T` with
  sentinel null values (`""` / `0` / `false`); `x == null` / `x != null`
  compare against the sentinel. Known M1 deviation (documented): the sentinel
  is indistinguishable from a legitimate zero-ish value.
- **`any`/`unknown`** (64-bit int boxes, narrowed with `as`), `never`,
  **optional/default parameters** (`x?: T`, `x: T = e` — omitted call sites
  fill the sentinel/default through an AST post-pass, hoisting-safe),
  **tuple types** (`[A, B]` → synthesized value struct with `_0..` fields,
  read via `t[0]`), **`as`/`!` assertions** (checked scalar casts; `!` is
  identity in M1's value model).

### Added (S3 modules)
- **Module forms**: `import * from "p"` (whole-module merge), `import { a, b }
  from "p"`, `import d from "p"`, side-effect `import "./p"`; `export
  function`/`export interface` pass through as ordinary declarations. The TS
  front end now emits real `ImportDecl`s into the shared pipeline.
- **Loader resolution**: `./`/`../` specifiers complete with `.ax`/`.ts`/
  `.tsx` and `index.<ext>`; bare names probe `aox_modules/<pkg>` (full
  manifest resolution is W2's `aoxn pkg`). `scan_imports` (build-cache
  dependency walk) understands every form.
- **The bare `import "path"` form is removed** (diagnostic points at the new
  syntax). All of `stdlib/`, `selfhost/`, `examples/`, `web/` and the
  embedded test fixtures migrated in the same change; the self-hosted
  `parser.ax` accepts the new forms and rejects the old one, `load.ax`
  gained `.`/`..` path normalization so its include-once/cycle keys stay
  stable — the byte-identical fixed point passes unchanged.

### Known limits (tracked in docs/ts-m1-spec.md §0.1)
- multiple independent array-length parameters per function (the
  monomorphizer still carries one length parameter);
- `import * as ns`, `export default`, re-exports, top-level module
  statements — later slices; `typeof`/classes/closures — TS-M2.

## [0.27.1] - 2026-09-30

**Experimental C-emitting backend** (`--backend c` / `AOXN_BACKEND=c`) — the
first deliverable of the
[LLVM-independence investigation](docs/llvm-independence-report.md): instead
of building LLVM IR, codegen emits ISO C99 text and hands it to the same clang
toolchain that already does the final link. Everything else is unchanged: the
typechecked AST, the clang link step, the `run`/`build` cache, and the
self-hosted compiler are untouched. The default backend stays LLVM; the C
backend is opt-in.

### Added
- **`src/codegen_c.rs`** (~900 lines) — walks the same AST as `src/codegen.rs`
  and emits C. Mapping highlights: structs map to C structs; arrays wrap in
  single-field structs (`typedef struct { T data[N]; }`) for C value
  semantics matching the language spec; `nsw`/`inbounds` UB maps to plain C
  signed arithmetic and indexing; raw-memory builtins go through `memcpy`
  helpers (strict-aliasing safe); no C standard header is included — the
  runtime surface uses `__builtin_*` forms and every other C function is an
  Aoxn `extern def` declaration in the same shape the LLVM backend uses.
- **`--backend <llvm|c>` CLI flag** plus the `AOXN_BACKEND` env var (what the
  test suite uses to run everything through one backend); the build cache key
  includes the backend.
- **`c_backend_matches_llvm_backend` test** — runs three multi-feature
  examples through both backends via the real CLI and asserts byte-identical
  stdout and exit codes (suite: 127 → 128).

### Measured (2026-09-30, this machine, interleaved min of 5–7 runs)
- Output parity: all 11 runnable `examples/` byte-identical between
  backends; the full suite (pipeline 97 + TS 28 + UI 2) passes under
  `AOXN_BACKEND=c` as well.
- Runtime: within ±10% of the LLVM backend on the example benchmarks
  (both are `clang -O3`-optimized native code).
- Compile time end-to-end: `examples/hello.ax` 836 → 884 ms; the ~7k-line
  self-hosting input 4000 → 4226 ms (+6%). The feared C-front-end regression
  did not materialize at O3.

### Notes
- The C backend is experimental: `aoxn ir` still prints LLVM IR (the C
  backend has no IR stage), `AOXN_PASSES` does not apply, and C's unspecified
  evaluation order between operands means expressions with multiple calls
  may order side effects differently than the LLVM backend (the spec does not
  pin an order either).
- No language, semantic, ABI, or self-hosting changes; the byte-exact
  self-hosting fixed point is unaffected.

## [0.27.0] - 2026-09-29

**UI standard library** — a Qt-flavored, immediate-mode GUI toolkit written
100% in Aoxn on top of raw Win32/GDI FFI (`stdlib/ui.ax` + `stdlib/ui_win.ax`,
`docs/ui.md`, `examples/ui_demo.ax`). No compiler, language or codegen
changes: the byte-exact self-hosting fixed point and the full suite pass
unchanged.

### Added
- **`stdlib/ui.ax` — the portable half** (compiles and links on every
  platform, no platform externs): UTF-8 → UTF-16LE conversion with
  surrogate pairs (`utf16_write`/`ui_utf16`), COLORREF packing (`ui_rgb`),
  position-derived widget ids (`ui_wid_id`), little-endian i32 assembly
  (`i32_at`), the `UI`/`Palette`/`TextSize` value types, light + dark
  palettes, heap-state accessors (`st_get`/`st_set`, documented slot
  layout) and the per-frame bump arena (`utf16f`/`itoa10`) that keeps
  steady-state frames allocation-free — string concat leaks by design, so
  per-frame UI text is the discipline that needs it most.
- **`stdlib/ui_win.ax` — the Windows backend** (the web suite's
  `sock_win.ax` pattern: the backend is its own file; switching backends
  later = changing one import line):
  - lifecycle `ui_init` / `ui_frame` / `ui_present` / `ui_close` /
    `ui_fini` / `ui_alert` — DPI-aware window with an exact client size
    (`AdjustWindowRect`), `FreeConsole()` to drop the console-subsystem
    terminal, CS_OWNDC + NULL background brush double-buffered GDI
    rendering, per-frame resize detection, `MsgWaitForMultipleObjects`
    frame cap (default 15 ms) so idle windows cost ~0% CPU;
  - immediate-mode widgets: `ui_button` (press+release click, hover/press
    faces, centered label), `ui_checkbox` and `ui_slider` (caller owns the
    value; drag continues outside the track via a single press-claim
    slot), `ui_progress`, `ui_label`/`ui_label_dim`/`ui_label_int` (int
    rendering without per-frame string building), `ui_title`,
    `ui_separator`, `ui_panel`;
  - primitives `ui_fill_rect` / `ui_frame_rect` / `ui_draw_line` /
    `ui_draw_text(_big)` / `ui_measure`, polled input
    (`c.mx/c.my/c.dn/c.dn2`, `ui_mouse_in`, `ui_key_down`,
    `ui_key_pressed` with a 256-key snapshot and per-frame edge
    detection);
  - **no callbacks anywhere**: the window procedure IS DefWindowProcW
    (address via GetProcAddress — the language has no function pointers),
    everything is polled per frame; `GetAsyncKeyState` bit 15 is OR-ed
    with the bit-0 "pressed since last call" latch so sub-frame presses
    are never lost, with exactly one call per key per frame;
  - all Win32 constants are hand-summed decimal literals (the language
    has no hex literals and no bitwise operators); a real W-suffix trap
    is recorded inline: `TranslateMessage` has no `W` export (its `MSG*`
    is charset-neutral) — `TranslateMessageW` does not exist in
    user32.lib.
- `examples/ui_demo.ax` — every v1 widget, light/dark palette switch,
  drawing primitives, UTF-8/emoji text, Esc-to-close. Run:
  `aoxn run examples\ui_demo.ax -l user32 -l gdi32`.
- `tests/ui.rs` — `ui_pure_helpers_utf16_rgb_ids` (all platforms: encoding
  incl. surrogates, rgb packing, ids, palettes, negative i32 reads) and
  `ui_window_selfclose_smoke` (Windows: builds a real window, ~90 bounded
  frames, self-closes; skips with exit code 3 on headless sessions).
- `docs/ui.md` — design rationale, full API reference, platform matrix,
  limits and roadmap.

### Docs & wiki
- **`README.md` rewritten** against the current tree — it still claimed
  "v0.7 · 70/70 tests" and pre-fixed-point self-hosting. Now: v0.27.0 status,
  127 tests, the self-hosting fixed point, the UI toolkit quickstart, the TS
  front end in the layout table. Bilingual (English first, 中文 second).
- **`SECURITY.md` rewritten bilingually** (English first, 中文 second);
  the supported-version table now covers 0.27.x, the cache-poisoning entry
  covers `build` as well as `run`, and the UI toolkit's raw FFI is called out
  in the out-of-scope section.
- `wiki/Standard-Library.md` gains the UI section in both languages;
  `wiki/Home.md`/`wiki/Roadmap.md` move the verified version to 0.27.0;
  `wiki/Testing-and-CI.md` test counts refreshed (127 total).

## [0.26.3] - 2026-09-27

**Compile-speed follow-through** — `aoxn build` joins the `run` content-hash
cache, and the front-end hot spots found by a fresh audit are fixed. No
language or codegen changes: the byte-exact self-hosting fixed point
(`selfhost_driver_self_compiles`, IR + COFF object) passes unchanged.

### Added
- **`aoxn build` reuses the content-hash cache** (shared key with `run`, so a
  `run` followed by a `build` of the same program shares one entry): a
  repeated build of unchanged sources copies the cached executable instead of
  recompiling and re-linking — measured `aoxn build examples/hello.ax -o
  out.exe` ~0.8–1.6s → ~0.2s on the dev machine (every `build` previously
  always paid full compile + link). Same invalidation semantics as `run`:
  any source, option, `-l`/`-L`, or compiler-binary change is a miss;
  `AOXN_NO_CACHE=1` / `AOXN_CACHE_DIR` apply unchanged; concurrent
  invocations publish through the same `<key>.<pid>` temp + rename.

### Changed (performance)
- Front-end micro-optimizations from a fresh hot-spot audit (all IR-invariant,
  verified by the fixed-point test; effects are individually below the dev
  machine's noise floor since the front end is <1% of a large build, but they
  remove real algorithmic and allocation costs):
  - the monomorphization instance queue is a `VecDeque` — the old
    `Vec::remove(0)` drain was O(m²) in the instance count;
  - typecheck struct-field lookup is O(1): `StructTable` values are now a
    `StructInfo { fields, index }` (mirroring codegen's `struct_fields` map),
    covering struct literals, `Point(...)` construction and field access;
    duplicate/missing-field diagnostics keep byte-identical messages and
    error positions;
  - `type_size` memoizes per `Type` — thousands of aggregate copies no longer
    re-walk the same LLVM types through `LLVMStoreSizeOfType`;
  - direct call sites no longer clone the callee's whole `params: Vec<Type>`;
    only the per-parameter compound flag survives the borrow;
  - generic call sites no longer clone the callee signature pieces before the
    dedup check (cloning happens only when a new instance is created);
  - `AOXN_TC_TRACE` / `AOXN_CG_TRACE` are read once per compile instead of
    once per function; `printf` declaration goes through the shared `externs`
    cache like every other C helper.

### Verified not actionable
- **F5 (link-layer probe caching) re-examined with data and closed**: the
  report's premise was that clang's ~136ms MSVC detection is skippable.
  Measured on clang 23.1.0 / Windows: `clang --version` (pure driver startup
  floor) costs 156–189ms **with and without** `INCLUDE`/`LIB` set — there is
  no environment fast path, and link timings with/without env are
  indistinguishable within the machine's noise. Combined with the report's
  earlier finding that spawning `lld-link` directly is a wash (§四), F5 stays
  skipped — now with evidence instead of an open question. The remaining
  link/startup costs are DLL-load floors (LLVM-C.dll, clang's own DLLs).

### Docs/community (e99a3b0, landed with this cycle)
- CODE_OF_CONDUCT.md, CONTRIBUTING.md, SECURITY.md and GitHub issue/PR
  templates added; CONTRIBUTING.md now states the aggregate-ABI codegen
  invariants inline (they previously lived only in gitignored AGENTS.md).

## [0.26.2] - 2026-09-27

**Compile-time work from `docs/optimization-report.md` + Tier-2 CI fixes** —
the front end was already thin (<1% of a 7k-line build); these four items
attack the fixed costs around it. Default codegen is unchanged (still
`default<O3>`), so the "parity with `clang -O3`" promise and the self-hosting
fixed point are untouched (F1/F2 only add opt-in paths; F3 sits outside the
compiler). The same release makes the Linux/macOS CI matrix green for the
first time (see **Fixed**).

### Added
- **`--O1` fast-compile level (F1)**: runs the `default<O1>` pipeline. On the
  7k-line self-host input the pass pipeline drops from ~2.4s to ~1.1s
  (≈-50% compile time); recommended for iteration and compile-time-sensitive
  CI. Measured cost: recursion/inlining-heavy code (fib) runs ~38% slower,
  loop-shaped code (primes) is unaffected — which is why O3 stays the default.
  `--O2`/`--O3` are also accepted for symmetry, and at most one level flag may
  be given (otherwise exit code 2). `AOXN_PASSES=<pipeline>` still overrides
  the pipeline text at any level > 0.
- **`Aoxn run` build cache (F3)**: the executable is cached in
  `target/cache/` keyed on the content hash of the entry file *and all
  transitive imports*, plus the compiler binary's identity (size + mtime) and
  every codegen-affecting option (level, `AOXN_CPU`, `AOXN_PASSES`, `-l`/`-L`,
  resolved clang path). An unchanged re-run skips compile + link: measured
  `Aoxn run examples/hello.ax` 1113ms cold → ~85ms warm. `AOXN_CACHE_DIR`
  relocates the cache, `AOXN_NO_CACHE=1` disables it, and the cache is bounded
  to 64 entries (oldest-first, hits refresh the mtime).
- **`aoxn::dependency_files`**: public helper returning the entry file plus
  every transitively imported source file (`None` when a file is unreadable),
  so the cache key covers the whole program instead of just the entry.
- **Tests**: `optimization_levels_agree_on_program_output` (O0..O3 produce
  identical program behavior), `optimization_levels_produce_distinct_ir`
  (O0/O1/O2/O3 really select different pipelines),
  `dependency_files_follows_import_chain` (transitive imports are found, a
  missing file disables the cache), and
  `llvm_link_name_probe_covers_platform_layouts` (Windows/macOS/Ubuntu LLVM
  library layouts). Suite: 93 → 97 integration tests.

### Changed
- **`--O0` now really is O0 (F2)**: `--O0` used to skip only the IR pipeline
  while the target machine still ran at `CodeGenOptLevel` 2. It now creates
  the target machine with `LLVMCodeGenLevelNone`, so instruction selection
  takes LLVM's fast-isel path. (`CODEGEN_LEVEL_NONE`/`CODEGEN_LEVEL_LESS`
  added to `src/llvm.rs`.)
- **Optimization level plumbed through the API**: `opt: bool` became an
  `opt_level: u8` internally; every existing `bool` entry point is retained as
  a forwarding wrapper (`true` = O3, `false` = O0), so `tests/pipeline.rs` and
  the self-host drivers are unaffected. New `*_lvl` entry points
  (`build_paths_opts_lvl`, `compile_paths_to_ir_lvl`, …) take the level.
- **Dev profile compiles optimized (F4)**: `[profile.dev] opt-level = 1` in
  `Cargo.toml`. The compiler is ~8x slower at codegen when built unoptimized,
  which made every `cargo run` iteration pay for it. Benchmark with
  `--release` as before.

### Fixed
- **Tier-2 CI (Linux / macOS) had never actually run green** — the v0.26.1
  platform matrix failed on `linux` and `macos-arm64` with 5 failures each;
  all of them were on the *test / self-hosted* side (the Rust compiler itself
  was already platformized). See `docs/platform-support.md` §7:
  - **`-lLLVM-C` does not exist on Linux**: Debian/Ubuntu put the C API inside
    a versioned `libLLVM-<N>.so`, so the self-host tests died with
    `cannot find -lLLVM-C`. New `platform::llvm_link_name()` probes the install
    (`LLVM-C` → newest `libLLVM-<N>` → unversioned `libLLVM`); `AOXN_LLVM_LIB`
    overrides it. Covered by `llvm_link_name_probe_covers_platform_layouts`.
  - **Apple Silicon**: the self-hosted code generator registered only the X86
    backend, so arm64 hosts failed with `no available targets are compatible
    with triple arm64-apple-darwin`. `selfhost/codegen.ax` now registers
    AArch64 too (the self-host counterpart of the Rust-side B3 fix).
  - **PIE on Linux**: `selfhost/codegen.ax` created its target machine with
    `RELOC_DEFAULT`; it now uses `CG_RELOC()` (0 on Windows, 2 = PIC
    elsewhere), mirroring `platform::is_windows()`.
  - **POSIX PATH separator**: the self-host tests built `PATH` as
    `"{llvm_bin};{PATH}"`, which on POSIX collapses to one nonexistent
    directory — the drivers then could not find `clang`. They now use
    `std::env::join_paths`.
  - **Loader path for linked libraries**: `link_opts` passed `-L` but no
    rpath, so an executable linked against a non-default LLVM dir (Homebrew's
    `libLLVM-C.dylib` re-exports `@rpath/libLLVM.dylib`) could not start; each
    `-L` dir now also gets `-Wl,-rpath,<dir>` on POSIX (Windows linkers reject
    `-rpath`, and `build.rs` already did this for the compiler itself).
  - **`system()` semantics**: the stdlib exposed the raw C `system()`, whose
    POSIX return is a wait status (`exit 7` → 1792) while Windows returns the
    exit code. New `system_exit_code(cmd)` normalizes both (a signal-killed
    process is reported shell-style as 128 + signal); `stdlib_system_spawn`
    uses it and asserts `7` on every platform.
- Local verification: 97/97 integration tests green on Windows, including the
  self-hosting fixed point and the self-hosted codegen/loader parity tests.

### Notes / follow-ups
- Not done (deliberately, from the report): link-layer micro-tuning (already at
  the lld-link floor, ≤130ms/call) and delay-loading the 73MB `LLVM-C.dll`
  (F5/F6 — low value, Windows-specific).
- `docs/spec.md` "Tooling contract" documents the level flags, `AOXN_PASSES`,
  and the run cache.

## [0.26.1] - 2026-09-27

**Cross-platform migration (Linux x86_64, macOS x86_64/arm64)** — Aoxn is no
longer Windows-only. The compiler builds and passes the full suite on four
platforms; CI runs the matrix.

### Added
- **`target_os() -> string` builtin**: compile-time platform query returning
  `"windows" | "linux" | "macos" | "other"`, folded to a module-internal
  string constant (same value from both the Rust and self-hosted compilers on
  the same host). This is the minimal platform-awareness facility Aoxn
  programs need to branch on OS-specific code.
- **`src/platform.rs`**: central platform abstraction (exe/obj extensions,
  stack-link flag, target-OS name, LLVM lib candidates). `lib.rs`,
  `main.rs`, `codegen.rs`, `build.rs` all route through it.
- **`build.rs` platform portability** (B1): probes for `libLLVM-C` /
  `libLLVM-XX` / `libLLVM` (Linux), `libLLVM.dylib` (Homebrew), or
  `LLVM-C.lib` (Windows); emits an rpath so `aoxn` finds libLLVM at runtime;
  accepts `AOXN_LLVM_DIR` (with a `AXON_LLVM_DIR` legacy alias).
- **AArch64 backend registration** (B3): `LLVMInitializeAArch64{TargetInfo,
  Target,TargetMC,AsmPrinter}` registered alongside X86 (they don't conflict;
  `LLVMGetTargetFromTriple` selects by triple). Apple Silicon is now a
  first-class target.
- **PIC relocation on non-Windows** (B4): the target machine is created with
  `RELOC_PIC` on Linux/macOS (PIE is the default there) and `RELOC_DEFAULT` on
  Windows.
- **CI matrix** (T1.5/M1-M3): `windows-latest`, `ubuntu-latest`, `macos-13`
  (Intel), `macos-14` (arm64) — each runs build + 93 tests + smoke.

### Changed
- **Self-hosted codegen (`selfhost/codegen.ax`)**: the `_setmode(1, 0x8000)`
  entry-wrapper call is now emitted only when `target_os() == "windows"`
  (S2). POSIX stdout is already binary-safe.
- **Self-hosted driver (`selfhost/driver.ax`)**: the `-Wl,/STACK:8388608`
  link flag is Windows-only (S1); POSIX main-thread stacks are 8MB already.
  New `exe_suffix()` helper (`.exe` on Windows, empty elsewhere).
- **`selfhost/driver_self_demo.ax`**: the LLVM lib dir and artifact suffix are
  chosen by `target_os()` (S3) instead of hardcoded `C:/Program Files/...`.
  Other `driver_*_demo.ax` use `exe_suffix()` for portable output names.
- **`tests/pipeline.rs`**: the ~30 hardcoded `.exe` suffixes are replaced by a
  platform-aware `EXE` const (S3's Rust half).
- **`docs/spec.md`**: `target_os()` documented under "Platform query"; new
  "Platform support" section declaring Tier 1 = Windows x86_64, Tier 2 =
  Linux x86_64 / macOS x86_64 / macOS arm64, with per-platform toolchain notes.

### Notes / follow-ups
- Linux/macOS behavior is CI-verified; local verification on Windows used the
  same code paths (`platform::is_*` are `cfg!`-based, so the Windows build is
  byte-identical to v0.26.0 apart from the new builtin).
- The fixed-point test (`selfhost_driver_self_compiles`) compares ELF/Mach-O
  objects on non-Windows instead of COFF; the comparison is still byte-identical
  (same platform, same reloc model on both sides).
- `AOXN_PASSES=<pipeline>` (from v0.26.0) remains available for pass-pipeline
  experiments.

## [0.26.0] - 2026-09-26

Compile times collapse. The code generator no longer materializes whole
aggregates as SSA values and aggregates cross function boundaries by pointer.
On the self-hosting compiler (~7k lines of Aoxn) codegen drops from **32.0s to
~3.3s** (O3 passes 10.9s → 1.8s, instruction selection 21.0s → 1.5s); the
integration suite runs roughly twice as fast. Generated-code speed is
unchanged (struct-copy and array benchmarks stay within noise of their
documented values).

### Changed
- **Aggregate ABI: structs and arrays are passed by pointer.** The callee
  copies the pointee into its own slot (value semantics preserved) and
  aggregate returns use an sret out-pointer — the same convention the
  self-hosted codegen has used since v0.19. `extern def` keeps the plain C
  ABI, since that is the FFI boundary. By-value aggregates forced every call
  site to build and every callee to extract a whole SSA aggregate (the
  self-hosting compiler passes ~500-byte, 40-field state structs), which
  multiplied IR size and made the optimizer and the backend superlinear.
- **Aggregate values are represented by their address** throughout codegen:
  struct literals, array literals, replication temps and aggregate-returning
  calls no longer `load` the whole aggregate as an SSA value; copying stays
  an explicit `memcpy`. Those giant loads/stores were the second half of the
  pathology (instcombine alone: 15s; ISel 14-44s depending on how much the
  pipeline had simplified first).
- Aggregate sizes in `memcpy` are plain integer constants now
  (`LLVMStoreSizeOfType`, with the module data layout established *before* IR
  emission) instead of `LLVMSizeOf`'s `ptrtoint(gep)` constant expressions,
  which every pass had to re-fold.

### Fixed
- **Temp-cache aliasing across functions** (`module verification failed:
  Referring to an instruction in another function!`): struct construction
  cloned its field expressions before emitting them, but the literal/temp
  caches are keyed by AST node address — a temporary clone's address gets
  recycled, so sites in different functions could share one hoisted temp.
  Field expressions are now borrowed.

### Added
- `AOXN_TIME=1` additionally reports codegen sub-phases (`cg.build`,
  `cg.verify`, `cg.target`, `cg.passes`, `cg.isel`); this is what localized
  the pathology.
- `AOXN_PASSES=<pipeline>` overrides the LLVM pass pipeline (compile-time
  experiments and pathological inputs); the default remains `default<O3>`.

## [0.25.0] - 2026-09-26

The self-hosting fixed point is now verified down to the object file, and the
self-hosted codegen's nested-aggregate coverage is pinned by the shared
fixture.

### Added
- **Object-level fixed point**: `selfhost_driver_self_compiles` now also
  compares the COFF object files emitted by the Rust-built compiler and by
  the Aoxn-built (stage-2) compiler for the same program — byte-identical,
  alongside the v0.24 IR-byte comparison.
- **Nested-aggregate coverage in the self-hosting fixture**: the shared
  `STDLIB_USE_PROG` target now exercises 2D arrays (`[[int; 3]; 2]` literals,
  indexing, element assignment, `len`), structs with array-of-array fields,
  sub-array call arguments (`sum_row(g.cells[1])`), array literals passed
  straight to an array parameter (`sum_row([1, 2, 3])`), generic calls on
  literals (`sort([5, 3, 8, 1])[0]`), 2D `for` iteration and value-semantics
  copies of compound structs — verified through both the Rust-built and the
  Aoxn-built driver.

### Changed
- `STDLIB_USE_PROG` is written as a raw string literal (the
  escaped-continuation form had become unreadable at fixture size).

## [0.24.0] - 2026-09-26

Self-hosting reaches the artifact level: the Aoxn-written compiler gains an
IR dump (the self-hosted `aoxn ir`), and the fixed point is now verified on
IR bytes rather than only on program behavior. Also completes the
in-progress B1 `Diag::at` conversion from the v0.23 P2 work.

### Added
- **Self-hosted `aoxn ir`**: `selfhost/codegen.ax` gains `gen_ir_text`
  (`LLVMPrintModuleToString`) and `selfhost/driver.ax` gains
  `emit_ir(src, out)` — the Aoxn-written compiler can now dump a program's
  unoptimized module IR, matching the Rust CLI's `ir` subcommand.
- **Artifact-level self-hosting fixed point**: `selfhost_driver_self_compiles`
  now also compares the IR produced by the compiler built by the Rust
  compiler against the IR produced by the compiler built by the Aoxn compiler
  for the same stdlib program. The two dumps are byte-identical — the
  compiler reproduces itself at the artifact level, not only behaviorally
  (the v0.22 fixed point checked stdout + exit codes only).

### Changed
- `selfhost/driver_stdlib_demo.ax` additionally dumps `stdlib_use.ir` next to
  its product (fixed-point comparison material).

### Fixed
- Completed the in-progress B1 `Diag::at` conversion
  (docs/p2-compiler-performance.md): call sites carried a stray `Diag` prefix
  (`Err(Diag self.err(...))`, `Diag Diag::at(...)`), and 29 sites passed
  `"msg".into()` into `impl Into<String>` parameters, which is ambiguous.

## [0.23.0] - 2026-09-26

P2 "compiler performance & code quality" (docs/p2-compiler-performance.md),
part 1: measurement groundwork, the A-group hot paths, and the lexer
restructure. Batches 3-5 (Diag helpers, giant-function splits, table-driven
builtins, robustness) follow.

### Added
- **`AOXN_TIME=1` stage timing**: per-file `lex`/`parse` and per-phase
  `typecheck`/`codegen`/`link` wall-clock lines on stderr, in the style of
  `AOXN_TC_TRACE`/`AOXN_CG_TRACE` — the measurement base for all further
  compiler-performance work.
- `src/hashing.rs`: FxHash-style hasher (zero dependencies) for the
  compiler's internal lookup tables, whose iteration order is never
  observable; std's SipHash dominated small-map lookups.

### Changed (hot paths)
- **A1 monomorphization clones**: struct layouts are borrowed instead of
  cloned at construction sites (`structs` is a shared reference with the
  checker's lifetime, so the borrow outlives `&mut self` calls); call
  signatures are read piecewise from `sigs` (params/ret no longer deep-copied
  per concrete call); `field_type` returns `&Type`.
- **A2 parser `bump()` takes tokens by move** (`mem::replace`) instead of
  cloning every consumed token — payload strings and f-string token vecs are
  moved, and the f-string token vec is no longer cloned at all; the one error
  path that read a consumed token now peeks before bumping.
- **A3 O(1) lookups**: function-scoped binding tables switched from a
  reversed-linear-scan `Vec` to a `HashMap` (semantics preserved — Aoxn
  bindings are function-scoped and unique); codegen `struct_fields` became a
  per-struct field map, making field access O(1) instead of a linear scan
  per read/assignment; hot tables (scopes, sigs, generics, struct layouts,
  codegen locals/fns/fields) use the fast hasher.
- **A4 f-string interpolations lex in place** over the main char buffer: no
  padded string copy, no per-interpolation char re-collection. A virtual
  open paren disables indent tracking and the `brace_col + 1` column offset
  keeps every interpolation token at its exact source column.
- **A5**: `Gen` borrows `call_map` with a lifetime (the whole-map clone per
  compilation is gone); generic-call routing no longer clones the instance
  name (reading the `&'a` field copies the reference out of `self`); `elif`
  folding moves branches by value instead of cloning conditions/bodies.

### Changed (lexer restructure)
- `lex()` is now a `Lexer` struct with per-category scanners (`scan_word`/
  `scan_number`/`scan_string`/`scan_punct`/`scan_fstring`) and a single
  shared `adv!`/escape-table definition (previously duplicated between the
  main scan and f-string scanning). Interpolation sub-scans share the main
  buffer and scan bound. The hot advance/peek helpers stay macros so debug
  builds keep their speed.

### Fixed
- The `unclosed bracket` lex error now reports both `'('` and `'['` (it
  previously always said `'('`).
- Dead code removed: a constant-empty condition in the `load_u8`/`store_u8`
  arity diagnostic (whose suffix was malformed), and a redundant
  `cur_len_params` clear on the parse-error path.
- Mojibake (`鈥`/`鈫`/`路`) in comments replaced with proper `—`/`→`/`·`
  across `ast.rs`, `lib.rs`, `parser.rs`, `typecheck.rs`, `codegen.rs`.

## [0.22.0] - 2026-09-26

### Added
- **Self-hosting fixed point**: `selfhost/driver_self_demo.ax` — the
  Aoxn-written driver compiles the ENTIRE self-hosting compiler (driver +
  codegen + typecheck + parser + lexer + loader + stdlib, ~7k lines of Aoxn)
  into `target/selfhost_stage2.exe`. The stage-2 compiler then compiles a
  stdlib-importing program and its product's stdout + exit code match the
  Rust compiler's — the Aoxn compiler compiles itself and the product
  behaves identically. Regression test `selfhost_driver_self_compiles`.
- **The self-hosted driver compiles its own front end**:
  `selfhost/driver_frontend_demo.ax` builds `selfhost/lex_demo.ax` and
  `selfhost/parse_demo.ax` (pulling in the Aoxn-written lexer and parser)
  through the Aoxn pipeline; the products' output matches the Rust-compiled
  demos byte for byte. Regression test
  `selfhost_driver_compiles_selfhost_frontend`.
- The Aoxn-written driver compiles the **entire `examples/` suite** (11
  programs: hello, fib, primes, vectors, strings, benchmarks, stdlib_demo)
  with zero failures — arrays of structs, `[e] * N` struct replication,
  string arrays and generic instances all covered.
- `selfhost/driver.ax`: `compile_file_libs(...)` forwards `-l`/`-L` to clang
  (`compile_file` delegates with empty flags); needed to link programs that
  drive LLVM-C through `extern def`.

## [0.21.0] - 2026-09-26

### Added
- **Self-hosted codegen: arrays + raw memory (stdlib bootstrap).** The Aoxn
  code generator now covers the whole `stdlib/stdlib.ax` surface: array
  types/literals, `[e] * N` replication (runtime fill loop), indexing
  read/write, `len(array)`, `for x in arr` (hidden index + element copy),
  array params/returns using the same pointer + sret ABI as structs, arrays
  in struct fields, and the raw memory builtins (`load_i64`/`load_f64`/
  `load_u8`/`store_i64`/`store_f64`/`store_u8`/`as_ptr`/`as_string`).
  Short-circuit `and`/`or` lower to branch + phi like the Rust compiler.
- **The self-hosted driver compiles stdlib programs**: new
  `selfhost/driver_stdlib_demo.ax` + `selfhost_driver_compiles_stdlib` — the
  Aoxn-written pipeline (load -> check -> codegen -> clang) compiles a program
  importing the real `stdlib/stdlib.ax` (generic `sort`/`binary_search`/
  `sum_int`, `Vec`/raw memory, char classes) and the produced exe's stdout +
  exit code match the Rust compiler's.

### Fixed
- **Self-hosted `range(n)` loops emitted a null operand** (module
  verification failure): the single-argument form set `start` and then
  overwrote it with 0, leaving `end` null — only `range(a, b)` had ever been
  exercised by tests.
- **Self-hosted `if`/`elif`/`else` merge blocks** could be left unterminated
  (and a stray branch appended after a terminator) when a branch body ended
  in a nested compound statement; the merge now branches from the real end
  block of each path (`LLVMGetInsertBlock`). Regression covered by the
  stdlib-program test (`binary_search` has exactly this shape).

## [0.20.0] - 2026-09-25

### Added
- **`--cpu <name>` / `AOXN_CPU`**: select the LLVM target CPU (`native`
  enables host-specific SIMD, e.g. AVX2); the default stays generic so
  compiled output remains reproducible across machines.

### Changed
- **Codegen optimization pass.** `int` arithmetic now emits `nsw` and
  array/field GEPs emit `inbounds` — signed overflow and out-of-bounds
  indexing were already undefined by spec (matching C), and the spec text now
  says so explicitly. String lengths are cached: literals, `str()` and
  `as_string()` results record their byte length, and string bindings keep a
  tracked length in a side slot updated at every assignment. `s = s + piece`
  accumulator loops and f-string chains therefore run in O(total bytes)
  instead of rescanning the accumulated string on every `+` (the old O(n²)).
  Cached lengths are dropped when a raw store or an unknown C function could
  mutate string bytes (`invalidate_str_lens`).

### Fixed
- **Nested generic calls failed at codegen** (`internal error: unknown
  callable`): monomorphized instances are now checked as the same AST objects
  that codegen emits, so `call_map` routing by node address also works for
  generic calls *inside* generic instance bodies. Regression test
  `nested_generic_calls`. Side effect: generic bodies are no longer deep
  cloned three times per instance (faster typecheck on generic-heavy code).
- Codegen panic paths (empty array literal, callable lookup) surface as
  `internal` diagnostics instead of panicking.
- `--json` diagnostics escape newlines and control characters (multi-line
  messages previously produced invalid JSON).

## [0.19.0] - 2026-09-12

### Added
- **Self-hosted codegen: structs (value semantics).** Two-phase named LLVM
  struct declarations, field GEP read/write, struct literals filled into
  entry-hoisted temps, `memcpy` copies on binding/assignment, and field
  assignment. Struct parameters are passed as pointers that the callee copies
  into its own local slot; struct returns use an sret out-pointer — this
  avoids by-value aggregate function signatures (which hung LLVM) and scales
  to large structs. `LLVMVoidTypeInContext` is now used for void returns
  (previously a null type ref was passed to `LLVMFunctionType`).
- Codegen demo/test coverage: `dist2(Point, Point)`, a struct-returning
  `origin()`, field assignment, and copy-on-assignment (`s = p` leaves `p`
  untouched) — stdout parity with the Rust compiler.

## [0.18.0] - 2026-09-12

### Added
- **Self-hosted codegen: floats (f64).** The Aoxn code generator now handles
  float literals (`strtod` + `LLVMConstReal`), float locals/params/returns,
  `+ - * /` via `fadd/fsub/fmul/fdiv`, unary `-` via `fneg`, all six ordered
  comparisons (`fcmp` OEQ/UNE/OLT/OLE/OGT/OGE), `print(float)` (`"%f\n"`),
  and `str(float)` via `snprintf("%f")`. The codegen test now covers
  `4.000000`, `2.000000`, `-1.500000`, `f=1.500000`, `half f = 0.750000`
  with exact stdout parity against the Rust compiler.

## [0.17.0] - 2026-09-12

### Added
- **Self-hosting: the driver closes the loop in Aoxn.** `selfhost/driver.ax`
  orchestrates the Aoxn-written stages — import-aware `load_program` -> strict
  `check_all` -> LLVM-C codegen -> object emission -> `system("clang ...")`
  link — producing a native executable from a real `.ax` file. No Rust
  compiler involvement at runtime.
- `selfhost/driver_demo.ax` compiles `hello.ax` end to end; the
  `selfhost_driver_links_hello` test builds the demo, runs it, then executes
  the produced exe and compares its stdout with the Rust compiler's build
  (`hello, Aoxn`). 89 tests green.

## [0.16.0] - 2026-09-12

### Added
- **Self-hosted codegen: strings.** The Aoxn code generator now handles
  string literals/params/returns/locals (opaque pointers), `print(string)`,
  the `len` and `str` builtins (`str(int)` via `snprintf("%lld")`,
  `str(bool)` via a branch + phi over `true`/`false`), `+` concatenation
  (`malloc` + `memcpy` + NUL, never freed), and all six string comparisons
  via `strcmp`. f-strings work end-to-end because the parser already
  desugars them to `"lit" + str(expr) + ...`.
- C runtime declarations (`strlen`, `strcmp`, `malloc`, `memcpy`,
  `snprintf`) are emitted into the module on demand, so a checked program
  need not declare them.
- `selfhost_codegen_int_slice` now covers strings: `hello, aoxn!`, `12`,
  `n=5`, `n squared = 25` — stdout and exit code match the Rust compiler.

### Fixed
- Self-hosted codegen built pointer arithmetic with `LLVMBuildAdd` (invalid
  IR: "Invalid operator", verifier crash); mid/end pointers now use GEP
  byte offsets. Also fixed string bindings being tagged as ints.

## [0.15.0] - 2026-09-12

### Added
- **Self-hosted codegen: `bool` support and `print`.** The Aoxn code
  generator (`selfhost/codegen.ax`) now types locals/params/returns as
  `i64` or `i1` (arena tag -> LLVM type), so `bool` bindings, comparisons
  held in variables, and bool-returning functions work end-to-end.
  `print(int)` calls `printf("%lld\n")`; `print(bool)` branches over the
  static `true`/`false` strings like the Rust codegen. The C entry wrapper
  now sets stdout to binary mode (`_setmode`) for byte-identical newlines.
- `selfhost_codegen_int_slice` now compares **stdout** between the
  self-hosted object and the Rust compiler's build of the same program
  (`41\n24\ntrue\ntrue\n-41\n`, exit code 65) in addition to exit codes.

### Fixed
- Self-hosted codegen passed type-arena *tags* where `ty_ll` expects arena
  *indices*, allocating garbage element types for locals/params (crash).

## [0.14.0] - 2026-09-12

### Added
- **Self-hosting: multi-file import resolution in the Aoxn front end**
  (`selfhost/load.ax`). `load_program(path)` reads a real `.ax` file, resolves
  `import "..."` recursively (paths relative to the importing file), includes
  each file once, rejects cycles with an import stack, and parses all files
  into one shared arena so node indices stay valid across files. Top-level
  nodes are spliced into a single program root (careful `next`-chain
  detaching); `checker_state()` + `check_all` now accept an already-parsed
  state. Verified by `selfhost/load_demo.ax` +
  `selfhost_frontend_handles_imports` (diamond include-once, cycle rejection,
  missing-file rejection, and `examples/stdlib_demo.ax` typechecking as a
  real 2-file program with 10 monomorphized instances).

## [0.13.0] - 2026-09-12

### Added
- **Self-hosting stage 4, first slice: the Aoxn code generator written in
  Aoxn** (`selfhost/codegen.ax`). Drives the LLVM-C API through `extern def`
  and emits a native object file from the self-hosted checker's output:
  int/void functions, int locals, arithmetic and comparisons, `if`/`else`,
  `while`, `for`-range, `break`/`continue`, direct calls and **monomorphized
  generic instances** (routed through the checker's `call_node`/`call_fni`).
  Target setup, `default<O3>`, verification and object emission included.
- `selfhost/codegen_demo.ax`: parses, checks and code-generates a sample
  program (generic `twice`, `fact` recursion, loops) to `selfhost_out.obj`.
- `selfhost_codegen_int_slice` test: builds the demo with `-l LLVM-C`,
  links its object with clang, runs it and compares the exit code with the
  Rust compiler's output for the same source (both 65).

### Fixed
- **Rust codegen literal-temp collision across files** (latent since v0.8
  imports): per-file parse-time literal ids restart at 0, but `lit_temps`
  cached hoisted allocas globally by id — an aggregate literal in one file
  reused an alloca from another function ("Referring to an instruction in
  another function", LLVM abort). Temps/replication iterators are now keyed
  by AST node address (as generic call routing already was). Regression test
  `import_aggregate_literals_across_files`.
- Codegen type hints for unannotated `load_i64`/`load_u8` (int) and
  `load_f64` (float) bindings.

## [0.12.0] - 2026-09-12

### Added
- **Self-hosting stage 3: monomorphization in the Aoxn type checker**
  (`selfhost/typecheck.ax`). Generic declarations are collected with `TY_VAR`
  type parameters and length-`N` arrays; every generic call unifies argument
  types against the declared parameter types, builds a deterministic mangled
  instance name (`id.i`, `first.i.?.3` — same scheme as the Rust compiler),
  clones the declaration's AST with the type parameters and `N` substituted,
  registers the instance as a normal signature, and queues its body for
  checking. Instances are deduplicated (recursive generics terminate) and
  each cloned call site is routed to its instance via `call_node`/`call_fni`.
- Self-hosted front-end exit test: the Aoxn lexer + parser + checker now
  process the **entire `stdlib.ax`** in one program
  (`selfhost_frontend_handles_stdlib`), plus generic accept/reject cases in
  `tycheck_demo.ax`.

### Fixed
- Self-hosted lexer keyword parity: `and` / `or` / `not` now map to `&&` /
  `||` / `!` (as in the Rust lexer) instead of distinct tokens the parser
  did not understand — this blocked parsing any real program using them.
- Self-hosted checker argument indexing for multi-argument builtins
  (`load_u8`, `store_i64`, `store_f64`, `store_u8`): it followed the sibling
  chain of the first argument's *value* instead of the argument list,
  producing bogus "missing expression" errors.
- Self-hosted checker signature collection: extern declarations were
  reported as "malformed function" because the return type was assumed to
  sit before a body block; externs return the last child.
- Self-hosted parser records the length-parameter name on `[T; N]` nodes so
  the checker can substitute `N` inside cloned bodies.

## [0.11.0] - 2026-09-12

### Added
- **Self-hosting stage 2: the Aoxn parser written in Aoxn**
  (`selfhost/parser.ax`, ~1100 lines) — arena AST (tag / sval / ival / child /
  next in parallel Vecs, first-child + next-sibling), full grammar port:
  declarations (import / struct / def+extern, generic headers, array-length
  params), statements (let / annotated let / assignment / if-elif-else folding
  / while / for-range / for-array / break / continue / return / pass),
  precedence-climbing expressions, call arguments, indexing, field access,
  array literals + `[e] * N` replication, and f-string desugaring via
  sub-lexing. Verified by `selfhost/parse_demo.ax` plus an exact AST-dump
  regression test (`selfhost_parser_ast_dump`).
- **Self-hosting stage 3, first slice: the Aoxn type checker written in Aoxn**
  (`selfhost/typecheck.ax`, ~1000 lines) — strict rules ported: struct
  collection (duplicate + cycle detection), signature collection, scope and
  struct tables, all-paths-return / unreachable-code analysis, expression
  typing, builtins, struct literals. Generic functions are reported as
  unsupported for now. Verified by `selfhost/tycheck_demo.ax` (accepts a
  well-typed program, rejects an ill-typed one) plus
  `selfhost_typechecker_accepts_and_rejects`.
- Self-hosted lexer: `;` token for array types, and f-strings now emit the
  raw literal source in the FSTR token so the parser can re-lex
  interpolations (matching the Rust lexer's literal/expr split).

### Fixed
- **Self-hosted parser segfault (the v0.11 WIP known issue).** Helpers such as
  `new_node` mutated a pass-by-value `PState` copy and discarded the write-back
  (`p.n_tag = vec_push(...)`), so the caller's arena Vecs stayed at `data=0`
  and the first `vec_set` stored through NULL. Rewritten in write-back style:
  every mutating function returns the updated `PState`, and multi-value
  results travel through `p.res_node` / `p.res_vec`.
- Self-hosted type checker: same write-back bug in `alloc_ty`; additionally
  the type arena's `t_elem` / `t_len` / `t_sname` Vecs were index-misaligned
  (seeded with a dummy slot while `t_tag` was not), making `ty_sname` return
  the wrong struct names. Fixed; struct-field counting no longer includes the
  reservation rows.
- Codegen: an unannotated binding of `as_string(...)` / `as_ptr(...)`
  (`x = as_string(p)`) failed with "unknown call in type hint"; both builtins
  now carry type hints (`string` / `int`).

## [0.10.0] - 2026-09-06

### Changed
- **Project renamed: Axon → Aoxn** (crate, binary, docs, examples).

### Added
- **Self-hosting stage 1: the Aoxn lexer written in Aoxn**
  (`selfhost/lexer.ax`, ~770 lines) — a faithful port of the Rust lexer:
  Python-style layout (NEWLINE/INDENT/DEDENT with an indent stack), comments,
  paren continuation, all operators, string escapes, floats, f-string raw
  tokens. Verified by an exact token-stream test (`selfhost/lex_demo.ax`).
- stdlib: `vec_pop`.

### Fixed
- Struct cycle detection: a struct appearing in multiple sibling fields was
  falsely reported as recursive; replaced with proper gray/black DFS.
- Runtime crash in the demo pipeline: an empty indent stack (seed value
  missing) made `vec_get(stack, -1)` dereference NULL.

## [0.9.0] - 2026-09-06

### Added
- **Raw memory builtins** (self-hosting foundation): `load_i64`/`store_i64`,
  `load_f64`/`store_f64`, `load_u8`/`store_u8` (byte access on `int`
  addresses and on `string` bytes), and pointer reinterpretation
  `as_string`/`as_ptr`. Addresses are plain `int`; unsafe by design.
- stdlib: growable `Vec` (8-byte slots, write-back style: `v = vec_push(v, x)`,
  `vec_get`/`vec_set`/`vec_free`), byte buffers (`buf_new`, `fill_zero`),
  string byte access (`str_get`), char classification (`is_digit`, `is_alpha`,
  `is_space`), file IO (`read_file`, `write_file` via FFI), and `system(cmd)`
  for process spawning.

### Fixed
- Lexer: `>=` was lexed as `>` (a latent bug no earlier test caught 鈥?  `4 >= 5` is false under both). Added boundary-equality regression tests.

## [0.8.1] - 2026-09-06

### Added
- `-l NAME` / `-L DIR` link flags: user programs can link arbitrary C
  libraries (LLVM-C, crypto, ...).
- Self-hosting proof of concept: `examples/ffi_llvm.ax` drives the LLVM-C
  API from Aoxn (pointers pass as `int`, ABI-identical on x86-64) and emits
  real IR 鈥?`define i64 @answer() { ret i64 42 }`.
- Self-hosting feasibility assessment: `docs/selfhost.md` (capability
  matrix, gap analysis, staged bootstrap plan, verdict: feasible).

## [0.8.0] - 2026-09-06

### Added
- **Import / module system**: `import "../stdlib/stdlib.ax"` 鈥?paths resolve
  relative to the importing file; include-once per canonical path; circular
  imports rejected with the full cycle chain.
- **File-aware diagnostics**: every error now names its source file
  (`[type] stdlib/stdlib.ax:130:20: ...`, also in `--json` output).
- `Aoxn build / run / ir` are now import-aware 鈥?`Aoxn run examples\stdlib_demo.ax`
  alone pulls in the standard library.

## [0.7.0] - 2026-09-06

### Added
- **Generic functions**: `def sort[T, N](arr: [T; N]) -> [T; N]` 鈥?type
  parameters `T` and array-length parameters `N`, monomorphized at every call
  site (deterministic mangled instances like `sort.i.8`).
- The length parameter is usable as an `int` constant inside generic bodies
  (`range(N)`, `N - 1`).
- Standard library rewritten with generics: `sort`, `linear_search`,
  `binary_search`, `max_of`, `min_of`, `reverse`, `sum_int`, `sum_float` 鈥?  replacing the fixed-size `_8` functions.

### Fixed
- Generic declarations no longer reach codegen (a `[T; usize::MAX]` type
  crashed LLVM's array-type construction).

## [0.6.0] - 2026-09-06

### Added
- `extern def` 鈥?C FFI declarations (bodyless, resolved at link time).
- Multi-file compilation: `Aoxn build main.ax stdlib/stdlib.ax` merges all
  inputs into one namespace (build / run / ir).
- `stdlib/stdlib.ax` 鈥?the standard library written in Aoxn itself: math
  (`abs/min/max/clamp/pow_i/gcd/lcm/isqrt/is_prime/hypot` + `sqrt/floor/ceil`
  via FFI), search, sort.
- `examples/stdlib_demo.ax`.

## [0.5.0] - 2026-09-06

### Added
- `for` loops: `range(n)`, `range(a, b)`, `range(a, b, step)` (negative step
  supported), and array iteration (`for x in arr`). start/end/step evaluated
  once at loop entry (Python semantics).
- `break` / `continue`.
- f-strings: `f"hello {name}, {1 + 2}"` with `{{`/`}}` escapes and arbitrary
  expressions; desugars to `"lit" + str(expr) + ...`.
- `str()` builtin (int 鈫?decimal, float 鈫?`%f`, bool 鈫?`true`/`false`).

### Added (CI)
- GitHub Actions: windows-latest, winget LLVM, full test suite + smoke test.

## [0.4.0] - 2026-09-05

### Added
- String operations: `+` concatenation (runtime malloc + memcpy + NUL), all
  six comparison operators (byte-wise `strcmp`), `len(str)` (bytes).
- Strings in struct fields, array elements, parameters, and returns.
- C runtime functions (malloc/strlen/strcmp/snprintf) declared lazily.

### Design
- Strings are immutable; concatenation results are heap-allocated and never
  freed (no GC yet 鈥?documented behavior).

## [0.3.0] - 2026-09-05

### Added
- Fixed-size arrays `[T; N]`: literals, unchecked indexing, `[e] * N`
  replication (element evaluated once, runtime fill loop).
- Structs: Python-style indented fields, named-field construction
  (`Point(x=1, y=2)`), field access/assignment, nesting, forward references,
  recursion rejected.
- Value semantics: assignment/params/returns copy whole aggregates (memcpy).

### Fixed
- Struct GEP field indices must be i32 constants (LangRef rule).
- Large aggregates are never materialized as SSA values 鈥?`load [50000 x i64]`
  made SROA/O3 hang; assignment now goes through memcpy.

## [0.2.0] - 2026-09-05

### Changed
- Python-style surface syntax: indentation-delimited blocks, `def` / `elif` /
  `pass`, `#` comments, no braces or semicolons, `//` is integer division.
- Lexer emits NEWLINE / INDENT / DEDENT; blank and comment-only lines produce
  no tokens; newlines inside parentheses ignored (implicit line joining).
- `and` / `or` / `not` and `True` / `False` as aliases of the symbolic forms.
- Bindings: `x = 5` infers, `x: int = 5` checks, re-assignment keeps the type.

### Removed
- `fn` / `let` keywords, braces, semicolons, block comments.

## [0.1.0] - 2026-09-05

### Added
- Initial compiler in Rust with zero external crates: hand-written LLVM-C FFI
  (no inkwell/llvm-sys), pipeline lexer 鈫?parser 鈫?strict typecheck 鈫?LLVM O3
  鈫?object file 鈫?clang link.
- Primitives (int/float/bool/string), functions (mutual recursion), control
  flow, `print` builtin.
- `Aoxn build / run / ir` CLI with `--json` diagnostics for AI agents.
- Benchmarks: parity with `clang -O3` on identical algorithms.

