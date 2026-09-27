# Changelog

Notable changes to the Aoxn compiler and language. Aoxn follows semver-ish
minor bumps while pre-1.0: each minor version is a language milestone.

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

