# AGENTS.md — Aoxn compiler

Aoxn: AI-native compiled language with Python-style syntax (indentation
blocks, `def`, `elif`, `#` comments). Programs lower to ISO C
(`src/codegen_c.rs`) and clang compiles/links them to native code — measured
at parity with `clang -O3`. **v0.29.0 (2026-10-01): the LLVM dependency is
gone**; the C-emitting backend is the only backend (history:
`docs/llvm-independence-report.md`, `CHANGELOG.md`). This repo IS the
compiler (Rust workspace: root `aoxn` crate + `crates/aoxn-pkg`).
Language sources use the `.ax` extension. Language rules live in
`docs/spec.md` — keep it in sync with `src/parser.rs` + `src/typecheck.rs`
when the grammar changes. Update `CHANGELOG.md` on every version bump.
**The wiki (`wiki/`) is FROZEN since v0.29.3 (user instruction)** — do not
update wiki pages anymore; `docs/` is the living documentation and stays in
sync. AGENTS.md is a published repo file (tracked since v0.29.3) — update it
in the session bench like any other file.

## Commit policy (user instruction, standing)

**每对话独立工作树 / per-session worktree bench (v2, 2026-09-29 起)**:
every conversation works in its OWN git worktree ("bench") created at the
start of the session. Never commit session work from the shared main
worktree — parallel sessions must not be able to sweep each other's
half-finished files into their commits.

Session start:
1. Create the bench: `git worktree add D:/ailanguage-bench/<slug> -b <slug>`
   where `<slug>` is a short session topic (e.g. `ui`, `ts-m1`; append
   `-2`, `-3` on name collision). The bench is a SIBLING directory of the
   repo (`D:\ailanguage-bench\`), shares the same `.git`, and starts from
   the current `main`. NOTE: in Git Bash pass the path with FORWARD SLASHES
   (`D:/ailanguage-bench/<slug>`) — backslashes get eaten and the worktree
   lands in a mangled path.
2. Do ALL file work inside the bench directory; the main worktree
   (`D:\ailanguage`) stays untouched by the session.

**全量提交 / full commits**: inside the bench, every commit must include
the whole bench working tree — use `git add -A` (not `git add <file>`
cherry-picking, not `git add .`), then `git commit`, then verify with
`git status` that nothing is left behind.

- Applies to all work: source, tests, docs, selfhost Aoxn sources, CI config,
  AGENTS.md updates, and any new files created during the session.
- Before committing, delete throwaway probe artifacts you created (scratch
  `.ps1`/`.ax`/log files) so they do not land in history; do not delete files
  authored by the user.

Session end (merge back + push, 合并后统一推送 — main stays current):
1. In the MAIN worktree require a clean `git status` (tracked files). If
   another session holds uncommitted work there, STOP and let the user
   coordinate — never merge on top of a dirty main.
2. `git merge --ff-only <slug>` in the main worktree (if it cannot
   fast-forward: `git rebase main` in the bench, re-run tests there,
   retry). Then `git push`, keeping
   `github.com/AlonechatWorkspace/Aoxn-language` in sync (remote moved
   from `Ryan-178` on 2026-10-01).
3. Clean up: `git worktree remove D:/ailanguage-bench/<slug>` and
   `git branch -d <slug>`. A session paused mid-work keeps its bench and
   branch until the work lands.

**README/SECURITY 重写 / README & SECURITY rewrite (user instruction,
standing)**: after every conversation that changes the project, REWRITE
`README.md` and `SECURITY.md` against the new reality — same commit as the
work, before the session-end merge. Do not patch them incrementally;
regenerate the status/version/test-count sections from the actual tree
(CHANGELOG top entry, `cargo test` totals, current commands) so they can
never drift half a version behind (the old "v0.7 · 70/70 tests" README is
the cautionary tale). Both files are bilingual (English section first,
中文 section second) — update BOTH halves.

**Wiki 冻结 / wiki frozen (user instruction, standing, 2026-10-01 起)**:
每次项目结束后**不再修改 `wiki/`** — do NOT touch `wiki/` pages at the end
of (or during) a session anymore. The old wiki-sync rule (update affected
pages in the same commit) is RETIRED; the wiki stays as it is and may
describe older behavior. `docs/` remains living documentation: keep
`docs/spec.md` in sync with the grammar and update the topical docs
(`docs/ui.md`, `docs/ts-m1-spec.md`, …) as before — just never `wiki/`.

## Commands

```powershell
cargo build                                   # build the `aoxn` compiler (Rust only — no LLVM)
cargo test                                    # tests: pipeline + lib + TS + UI (see run_pkg_tests.sh for aoxn-pkg)
bash run_pkg_tests.sh                         # aoxn-pkg tests (Smart-App-Control hash-stamp workaround)
cargo run -- run examples\hello.ax            # compile+run an Aoxn program
cargo run -- run examples\stdlib_demo.ax      # imports ../stdlib/stdlib.ax itself
cargo run -- run selfhost\lex_demo.ax         # the Aoxn-written lexer tokenizing a sample
cargo run -- run selfhost\parse_demo.ax       # the Aoxn-written parser dumping a sample AST
cargo run -- run selfhost\tycheck_demo.ax     # the Aoxn-written checker accepting + rejecting
cargo run -- run selfhost\load_demo.ax        # Aoxn loader: imports + check of examples\stdlib_demo.ax
cargo run -- run selfhost\codegen_demo.ax     # self-hosted C emitter -> stdlib_use.c + .obj (needs clang)
cargo run -- run selfhost\driver_demo.ax      # Aoxn-written compiler: hello.ax -> exe (needs clang on PATH)
cargo run -- run selfhost\driver_stdlib_demo.ax  # Aoxn-written compiler: stdlib_use.ax -> exe
cargo run -- run selfhost\driver_frontend_demo.ax  # Aoxn compiler compiles its own lexer+parser
cargo run -- run selfhost\driver_self_demo.ax  # fixed point: Aoxn compiler compiles the Aoxn compiler
cargo run -- build examples\fib.ax -o f.exe   # emit a native executable (clang -O3 default)
cargo run -- c examples\fib.ax                # print the generated C (`ir` is a deprecated alias)
cargo run -- run bad.ax --json                # diagnostics as JSON for agent consumption
cargo run -- run examples\fib.ax --O1         # clang -O1: fast compile (O3 stays the default)
cargo run -- build examples\fib.ax --O0       # clang -O0
cargo run -- run examples\ui_gallery.ax -l user32 -l gdi32   # UI widget gallery
```

Package management (`crates/aoxn-pkg`, beta): `Aoxn pkg <cmd>` plus direct
aliases `Aoxn init|add|remove|install|update|outdated|tree|why|publish|yank|
audit|cache|npm-import`. Manifest `aoxn.json`, lockfile `aoxn.lock`, install dir
`aox_modules/`; registries are directories, git repos, or read-only HTTP mirrors (`http://`, v0.29.4); resolution is
PubGrub; `Cargo.lock` pins aoxn-pkg's own dependencies. v0.29.1: bare
package imports resolve entries via the manifest's `main`/`exports`/`types`
(`src/pkg_manifest.rs`, zero-dep JSON reader). v0.29.4: read-only HTTP
registry backend (`crates/aoxn-pkg/src/registry/http.rs`, TLS-free
`std::net` client; `packages/names.json` feeds the typosquat guard).
v0.29.5: npm bridge `aoxn npm-import` (`npm.rs`) — the npm CLI is the
transport; Aoxn packages published to npm land in `vendor/<name>/` as
path dependencies. Topical doc: `docs/pkg-manager.md`.

Optimization levels (v0.29.0): `--O0/--O1/--O2/--O3` select the clang `-O`
level used to compile the generated C — the C text itself is level-independent
(test `c_text_is_opt_level_independent`). `--backend c` is accepted for
compatibility (it is the only backend); any other value errors out.
`Aoxn run` AND `Aoxn build` share the exe cache: content hash of entry+all
transitive imports + compiler identity + options in `target/cache` — an
unchanged re-run/re-build skips compile and link (run: ~1.1s → ~85ms).
`AOXN_NO_CACHE=1` disables, `AOXN_CACHE_DIR=<dir>` relocates.

Env: `AOXN_DUMP_C=1` dumps generated C to stderr; `AOXN_TIME=1` prints
pipeline phase wall-clock (lex/parse/typecheck/codegen/link);
`AOXN_TC_TRACE=1` prints per-fn typecheck markers; `AOXN_CPU=native` (or
`--cpu native`) targets the host CPU; `AOXN_CLANG=<path>` selects the clang
executable. NOTE: `AOXN_DUMP_IR`, `AOXN_PASSES`, `AOXN_BACKEND`,
`AOXN_CG_TRACE` are DEAD since v0.29.0 (LLVM-era).

Single test: `cargo test --test pipeline recursion_fib`.

## Environment facts (non-obvious, verified)

- **clang is the only external toolchain** (needed to compile the generated C
  and to link). Resolution order: `AOXN_CLANG` env → PATH → repo-local
  `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe`. The winget
  "LLVM 23.1.0" install on this machine survives purely as clang's home —
  no LLVM library is probed, linked, or shipped since v0.29.0 (`build.rs`,
  `src/llvm.rs`, `src/codegen.rs` are deleted).
- clang auto-detects MSVC Build Tools, which must be installed (Rust host is
  x86_64-pc-windows-msvc) for the final link. User programs needing C
  libraries pass `-l NAME` / `-L DIR` (forwarded to clang).
- **COFF TimeDateStamp flake trap (fixed in a23e096)**: clang stamps the
  wall-clock time into every Windows object (bytes 4–8 of the COFF header),
  so two compiles of identical C differ by exactly those bytes. The
  fixed-point test masks them (`tests/pipeline.rs`, Windows only). If you see
  an object diff "first diff at Some(4)", it is the timestamp, not a codegen
  bug.
- **Do NOT add external crates to the compiler library `src/`** — zero deps
  there is a security property (see SECURITY.md). `crates/aoxn-pkg` may use
  crates (clap/serde/semver/pubgrub/sha2/hex/flate2/tar/dirs/thiserror),
  pinned by `Cargo.lock`.
- MSVC STL (VS 2022 17.14+) rejects clang 18 ("expected Clang 19.0.0 or
  newer") when compiling C/C++ with headers — irrelevant for Aoxn emission,
  but breaks ad-hoc C++ comparison probes; declare externs manually instead
  of including headers.
- This dev machine's 10–50ms phase timings swing ±2× (AV/indexer): benchmark
  comparisons need interleaved 5× min/median or they lie.
- **`run_pkg_tests.sh` exists because Windows Smart App Control blocks
  freshly built unsigned test binaries after their first runs** — the script
  bumps a marker comment so each test build produces a new binary hash. Use
  it instead of bare `cargo test -p aoxn-pkg` on this machine.

## Windows tooling gotchas (learned the hard way)

- **PowerShell 5.1 `-Encoding UTF8` writes a BOM.** Aoxn `.ax` sources are
  read byte-wise — a BOM (EF BB BF) is an "unexpected character" at 1:1.
  After any PowerShell rewrite of a `.ax` file, strip BOMs:
  check bytes `[0]==0xEF && [1]==0xBB && [2]==0xBF` and slice them off.
  The Write tool does NOT add a BOM — prefer it over shell heredocs.
- **PowerShell quoting for inline code is a trap** (backticks, `$()`, nested
  quotes). For anything non-trivial, write a `.ps1` via the Write tool and
  execute it. Commit messages: `git commit -F <file>`, never `-m` with
  multiline text.
- **Git Bash eats backslashes in path arguments** — `git worktree add
  D:\ailanguage-bench\foo` creates a mangled directory. Use forward slashes.
- `winget download` of LLVM needs `--accept-source-agreements`; CI installs
  clang the same way (Windows).

## Architecture (pipeline order)

```
.aox source (.ax extension)  — or TypeScript (.ts/.tsx) via src/ts/
  → src/lexer.rs      tokens with line/col/file; emits NEWLINE/INDENT/DEDENT
  → src/parser.rs     recursive-descent AST (src/ast.rs), Python-style layout
  → src/typecheck.rs  strict check + FnSig table + GENERIC monomorphizer
  → src/codegen_c.rs  ISO C text → clang -c → object file (clang -O<n> -w -c)
  → src/lib.rs        load imports (src/files.rs registry) → link via clang → executable
src/main.rs           CLI (build / run / c / pkg...), --json diagnostics, -l/-L link flags
crates/aoxn-pkg       package manager crate (its own dependency set; see above)
```

- Diagnostics: `Diag { stage, file: u32, line, col, message }` in lib.rs;
  stages are `lex | parse | type | internal | link | io`. `file` indexes
  `src/files.rs` (thread_local registry) — resolved to names at print/JSON
  time. Compiler-internal failures must surface as `internal` diags, never
  panics.
- The compiler library `src/` has zero external crate dependencies. The C
  emitter is plain string building — no LLVM handles, no FFI of its own
  beyond what user programs declare.
- Multi-file: `load_program` (lib.rs) resolves `import "..."` recursively —
  include-once per canonical path, cycles rejected via an import stack,
  paths relative to the importing file. String-based APIs (`build_exe`)
  reject imports; only path-based entry points resolve them.

## C backend invariants (src/codegen_c.rs; mirrored by selfhost/codegen.ax)

- **C gives Aoxn's value semantics natively**: structs map to C structs;
  arrays are wrapped in single-field structs (`typedef struct { T data[N]; }`)
  so assignment/param/return copy by value exactly like C struct semantics.
- Aggregate ABI: aggregates cross function boundaries as pointer + sret
  out-pointer (spelled natively in C), same shape the old LLVM backend used.
- The generated C text is **level-independent** — `--O` only picks the clang
  level. Stage-1 (Rust-built) and stage-2 (Aoxn-built) compilers must emit
  byte-identical C AND byte-identical objects for the same program (the
  fixed point); the only tolerated object difference is the COFF timestamp
  the test masks.
- `target_os()` is a compile-time builtin folding `"windows" | "linux" |
  "macos" | "other"` — **both** compilers must fold the same value
  (`platform::target_os_name()` Rust-side, `target_os()` inside
  `selfhost/codegen.ax`) or the fixed point breaks.
- Reserved-word collisions in emitted C identifiers are handled by the
  keyword table in codegen_c.rs; C runtime use is limited to the small
  builtin name list (`malloc`, `memcpy`, `strlen`, `snprintf`, …).
- platform.rs centralizes exe/obj extensions (`exe_ext`/`obj_ext`), the
  Windows stack-link flag (`-Wl,/STACK:8388608`), and `target_os_name()`.
  Tests must use these helpers, not literals (CI runs the suite on four
  platforms).

## Lexer invariants (Python-style layout)

- The lexer maintains an indent stack and emits `Indent`/`Dedent` tokens;
  the parser consumes `:` Newline Indent stmt* Dedent (or one simple stmt).
- Blank lines and comment-only lines produce **no tokens** (no indent changes).
- Inside parentheses `paren_depth > 0`: newlines and indentation are ignored
  (implicit line joining).
- EOF: flush trailing `Newline`, then pending `Dedent`s, then `Eof`.
- `//` is NOT a comment (it is int division); comments are `#` only.
- Indent/dedent mismatch → lex error "unindent does not match any outer
  indentation level".
- Tests embed indented sources; `dedent()` in tests/pipeline.rs strips the
  common leading whitespace before compiling.

## Language semantics (enforced, keep strict)

- No implicit int/float conversions, no re-typing a bound name, `bool`
  conditions only, all-paths-return, no unreachable code. This strictness is
  a feature (deterministic, AI-verifiable); do not relax it casually.
- Python-style surface, native semantics: `def`/`elif`/`pass`, `x = 5`
  infers, `x: int = 5` checks, re-assignment keeps the type, `and`/`or`/`not`
  + `True`/`False` are aliases of the symbolic operators.
- Arrays `[T; N]` and structs are first-class value types (copy on
  assignment/param/return). Indexing is unchecked (C-style); struct fields by
  name; construction requires every field by name (`Point(x=1, y=2)`).
- Strings are immutable byte sequences: `+` concatenates, all six comparisons
  work byte-wise, `len()` is bytes. They live in struct fields/arrays by
  shared pointer (safe: immutability); concat results leak by design.
- **One array length parameter per function** (`def f[N](a: [T; N], ...)`;
  multiple differently-sized arrays in one signature are still unsupported —
  this shapes API design, e.g. the UI toolkit's table/tree take heap-backed
  model objects instead of parallel arrays).
- When grammar/semantics change: update `docs/spec.md`, parser, typecheck,
  codegen, and add a test in `tests/pipeline.rs` in the same change.

## UI toolkit (stdlib/ui.ax + stdlib/ui_win.ax) — v3 facts

- Qt-flavored **immediate mode** (no callbacks possible: the language has no
  function pointers/closures). Widgets are per-frame functions; app state
  travels via the write-back idiom (struct in / struct out). Reference:
  `docs/ui.md`; showcase: `examples/ui_gallery.ax`; tests: `tests/ui.rs`.
- **Signal-slot without function pointers**: an integer-channel event bus —
  `ui_connect(c, signal, slot)` + `ui_emit(c, signal, kind, a, b)` queues
  `Ev` records read via `ui_event_count`/`ui_event(i)`; the app dispatches
  one switch on `ev.slot`. Widgets don't hold handlers.
- **Model/view without interfaces**: heap-backed `TableModel`/`TreeModel`
  (pointer structs + get/set fns) feed `ui_table`/`ui_tree`. Models are
  pointers precisely so views can mutate them (no value-copy write-back).
- Text editing: `Sel{anchor, caret}` byte-offset selection, UTF-8 aware;
  `ui_textbox` (single line) + `ui_textedit` (multi-line, line cache in the
  heap block) share pure edit ops in the portable half; clipboard via
  `ui_clip_get/set` (CF_UNICODETEXT). Render text slices with
  `ui_measure_sub`/`ui_draw_text_sub` (frame-arena based — never `str_sub`
  on a frame path).
- **GDI stock pens: `DC_PEN = 19`, `DC_BRUSH = 18`** — `GetStockObject(20)`
  is out of range and fails silently (no outline ever drew; fixed in
  v0.29.2). Keep the decimal-constant discipline: no hex literals, no
  bitwise ops in the language.
- The shared heap block (`st`, 1024 i64 slots) layout is documented in the
  `ui.ax` header comment — extend it there when adding state.

## Self-hosting status (docs/selfhost.md is a historical assessment; wiki/Self-Hosting.md is frozen at v0.29.2, the source is current)

- Stages 1–4 + loader + driver all DONE (`selfhost/`, ~7k lines of Aoxn):
  lexer, parser (arena AST), typecheck (incl. generic monomorphization),
  loader (include-once + cycles), codegen (**C text emitter mirroring
  `codegen_c.rs`** — the LLVM-C FFI port is gone since v0.29.0), driver
  (load → check → emit C → `clang -O3 -w -c` → link).
- **FIXED POINT**: `selfhost/driver_self_demo.ax` — the Aoxn-written driver
  compiles the whole self-hosting compiler into `target/selfhost_stage2.exe`;
  stage-2 then compiles a stdlib program and the generated C **and the
  emitted object** match the Rust-built compiler's byte-for-byte (objects
  modulo the COFF timestamp, see above). Runs on every platform now (skips
  only when clang is missing) — no more `LLVM-C.lib` precondition. Verified
  by `selfhost_driver_self_compiles`. `gen_c_text` (codegen.ax) + `emit_c`
  (driver.ax) are the self-hosted `aoxn c`.
- The Aoxn driver also compiles its own front end
  (`selfhost/driver_frontend_demo.ax`) and the whole `examples/` suite.
  Next rung: porting `main.rs` CLI semantics (argv is still missing from the
  language).
- CRITICAL value-semantics discipline (the v0.11 segfault): a function
  taking `p: PState`/`c: CState` by value MUST return the struct — local
  `p.n_tag = vec_push(...)` mutations are discarded by the caller. All
  selfhost helpers use write-back (`p = parse_expr(p)`) and pass extra
  results through fields (`p.res_node`, `p.res_vec`, `c.res_ok`, `c.r_ty`).
  Never nest mutating calls as arguments; assign each step.
- Porting traps that still matter: call-argument chains walk ARG nodes
  (`node_child(node_next(arg))`), never `node_next` of the first value;
  `and`/`or`/`not` lex as `&&`/`||`/`!` (no separate keyword tokens); an
  extern's return type is the last child (there is no body block); the
  loader must `vec_set(n_next, ch, -1)` before splicing file roots or you
  get a self-cycle; keep the global root in LoadState (`parse_program`
  overwrites `p.root` per file); `range(n)` with ONE argument must move the
  value to `end` before zeroing `start`; `if` merge branches must come from
  the real end block of each path (`LLVMGetInsertBlock`'s C-backend
  equivalent: whatever block the builder is at after the branch body).
- Aoxn-on-Aoxn debugging recipe: runtime crash in generated code → check
  seed values first (`vec_get(empty, -1)` dereferences NULL); compiler crash
  → per-fn stage traces (`AOXN_TC_TRACE`); silent wrong output → dump both
  sides' C (`AOXN_DUMP_C=1`) and diff.

## TypeScript front end (src/ts/) — TS-M1 W1 complete (v0.28.0)

- S0 lexer + S1 parser + S2a generics/arrays/templates + S2b f64 tower,
  unions/narrowing, any/optional/tuples + S3 modules ALL DONE. `load_file`
  dispatches on extension (`.ts`/`.tsx` → `ts::parser`), `aoxn build foo.ts`
  just works. TS lowers into the EXISTING `crate::ast` (console.log→print,
  x.length→len(x), object literal→StructLit via interface annotation).
- Legacy Aoxn `import "path"` is REMOVED (one-shot switch in W1-S3) — use
  `import * from "path"` everywhere, including stdlib/selfhost sources.
- `number` is f64 (double) everywhere since S2b; explicit type args
  `f<T>(x)` stay rejected (ambiguity with `a < b > (c)`).
- Roadmap: see `docs/ts-m1-spec.md` — W2 (pkg + CSS pipeline), W3 (benchmark
  v2), W4 (TS-M2 runtime semantics: objects/closures/GC — the "full TS
  compatibility" gate), W5 (acceptance samples). `crates/aoxn-pkg` (W2's
  package manager) landed as beta in v0.29.0.

## Web benchmark suite (web/) — facts for future sessions

- Platform direction decided (docs/web-platform-plan.md + docs/ts-m1-spec.md):
  TS front end -> existing pipeline (full TS syntax compatibility goal),
  **independent** package management (own manifest `aoxn.json`/`aoxn.lock`/
  `aox_modules/`; no registry proxy), CSS full compatibility + Tailwind
  toolchain + CSS Modules. Acceptance baseline: 3 canonical samples
  (REST API / SSR page / generic util lib) until real team projects arrive.
  Render metrics: Playwright + Chrome FCP/LCP/TTI.
- `web/server_win.ax` (`aoxn build web\server_win.ax -o web\server.exe -l ws2_32`)
  / `web/server_posix.ax`; shared `web/serve.ax` + `web/http_buf.ax`, socket
  wrappers `web/sock_win.ax` (ws2_32) / `web/sock_posix.ax`. Bench targets:
  `web/node-server.mjs` (plain node:http) and `web/next-app/` (Next.js 15 app
  router, `pnpm build`/`pnpm start`). Results: `docs/web-benchmark.md`.
- Observability: `/metrics` (Prometheus text, RED counters per route, bytes,
  connections, duration sum/max ns, uptime). Metrics block layout is
  documented above `render_metrics` in http_buf.ax (slot 104 = clock scratch
  pointer). `net_now_ns(m)` lives in the sock modules: Windows
  QueryPerformanceCounter, POSIX clock_gettime — **`timespec_get` does NOT
  link on Windows** (clang/MSVC libs), don't retry it.
- **`load_i64` takes ONE argument (an address)** — offsets go on the address
  (`load_i64(ts + 8)`); only `load_u8` takes `(base, off)`.
- **C `int` returns arrive zero-extended in i64** (callee writes EAX): -1
  shows up as 4294967295. The sock modules' `i32()` helper maps it back (a
  true 64-bit -1 passes through). SOCKET/pointer returns are full 64-bit.
- **No `\r` string escape** (`\n`, `\t`, `\\`, `\"` only) — HTTP CRLF is
  written as raw bytes 13/10 (`bb_crlf`).
- Long-running servers must render into byte buffers (`bb_*` in
  `http_buf.ax`): string concat results leak by design, so per-request concat
  would balloon RSS. This is idiomatic (C-style), not a bug.
- HTTP framing: drain loop in `serve.ax` handles several requests per recv
  and partial requests (scan for `\r\n\r\n`, slide remainder with a byte loop
  — overlapping `memcpy` is UB). Responses go out as ONE segment (header
  rendered in front of the body at fixed offset 512 in `bbuf`) with
  TCP_NODELAY; a split header/body send stalls on Nagle (~ms/request).
  `render_body` returns the NEW absolute offset — subtract the body start.
- POSIX bind trap (learned via web-bench CI): always `setsockopt(SO_REUSEADDR)`
  before `bind` — without it, a restart or the previous test server's
  TIME_WAIT connections make bind fail EADDRINUSE on Linux/macOS while
  Windows binds fine (Winsock semantics differ; don't add SO_REUSEADDR on
  Windows, it enables hijacking). Constants: **Linux SOL_SOCKET=1,
  SO_REUSEADDR=2; macOS/BSD SOL_SOCKET=0xFFFF(65535), SO_REUSEADDR=4** —
  passing Linux's level 1 on macOS fails silently and reproduces the bug.
  Node/Next set it by default, so comparison servers mask the bug in mixed
  runs. bench.mjs has a port pre-flight; web-bench.yml clears stray listeners.
- Tests: `web/loadtest/parity.mjs` (cross-platform functional: body parity
  vs Node, 404, keep-alive, /metrics) and `web/loadtest/bench.mjs` (oha
  engine, 3 interleaved trials, median; raw runs in `last-results.json`).
  `.github/workflows/web-bench.yml` runs parity + a 5s reference bench on
  windows-latest / ubuntu-latest / macos-14 (runner numbers are trend-only).
  oha binary goes in `loadtest/tools/` (gitignored — download cmd in
  `web/README.md`). autocannon is NOT used: single-core client capped at
  ~4k req/s and masked server differences. Even oha caps at ~7k req/s on
  this machine — use latency/in-flight (Little's law) to tell servers apart,
  not raw rps.
- Same-machine benchmark caveat: client + server share the 4C8T i5-1135G7;
  Node p50 jumps 4→9.7ms under 2-client load while Aoxn stays at 0.1ms.

## Known issues (deliberately unfixed — do not "drive by" fix)

- The self-hosted loader opens files with narrow `fopen` (stdlib `read_file`),
  so non-ASCII paths fail there; the Rust compiler is unaffected. Tests keep
  fixture dirs ASCII.

## Repo layout

- `examples/*.ax` — demo programs (hello, fib, primes, vectors, strings, benchmarks, stdlib_demo, ui_demo, ui_gallery)
- `web/` — web benchmark suite: HTTP/1.1 server written in Aoxn (FFI sockets)
  vs pnpm+Node.js+Next.js; see `web/README.md` + `docs/web-benchmark.md`
- `stdlib/stdlib.ax` — the standard library, written in Aoxn itself;
  `stdlib/ui.ax` + `stdlib/ui_win.ax` — the Qt-flavored immediate-mode UI
  toolkit v3 (layout engine, text selection/multi-line editing, menus,
  tree/table model+view, signal-slot events)
- `selfhost/` — the compiler rewritten in Aoxn (fixed point reached; C emitter)
- `crates/aoxn-pkg/` — the package manager (`aoxn pkg`, beta)
- `docs/selfhost.md` — historical self-hosting assessment (v0.19-era);
  `docs/spec.md` — language spec + roadmap;
  `docs/ui.md` — UI toolkit reference (v3);
  `docs/llvm-independence-report.md` — why/how LLVM was removed;
  `docs/ts-m1-spec.md` / `docs/web-platform-plan.md` — TS platform decisions
- `AGENTS.md` — this file: agent/contributor working agreement (published
  since v0.29.3)
- Repo: github.com/AlonechatWorkspace/Aoxn-language · Apache-2.0 · CI: three
  jobs — windows-latest (Tier 1, winget clang), ubuntu-latest (apt clang),
  macos-14 (arm64, Apple clang). The byte-exact self-hosting fixed point
  runs on ALL of them (skips only without clang).
- **macOS Intel (macos-13 / macos-x86_64) is NOT supported** — dropped in
  v0.27.1, re-added by mistake in the v0.28.0–v0.29.0 work, removed again
  in the v0.29.0 "drop macOS Intel" change (2026-10-01). Do NOT
  reintroduce Intel-Mac support claims or CI jobs.
