# Contributing to Aoxn

Thanks for taking a look at Aoxn. This repository **is** the compiler: a
Python-syntax, statically typed language that lowers to ISO C and compiles
through clang to native code, written in Rust with **zero external crate
dependencies**. Bug reports,
spec corrections, tests, stdlib work, performance measurements, and docs are all
useful — you do not have to write compiler internals to help.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
Please report security vulnerabilities privately per [SECURITY.md](SECURITY.md)
— not in a public issue.

- [Ways to contribute](#ways-to-contribute)
- [Getting started](#getting-started)
- [Build and test](#build-and-test)
- [Repository layout](#repository-layout)
- [Changing the language](#changing-the-language)
- [Code conventions](#code-conventions)
- [Tests](#tests)
- [Commits and pull requests](#commits-and-pull-requests)
- [AI-assisted contributions](#ai-assisted-contributions)

## Ways to contribute

| Kind | Start here |
|---|---|
| Compiler bug (wrong output, crash, bad diagnostic) | [Bug report](https://github.com/AlonechatWorkspace/Aoxn-language/issues/new?template=bug_report.yml) |
| Feature or tooling request | [Feature request](https://github.com/AlonechatWorkspace/Aoxn-language/issues/new?template=feature_request.yml) |
| Grammar or semantics change | [Language proposal](https://github.com/AlonechatWorkspace/Aoxn-language/issues/new?template=language_proposal.yml) — discuss before implementing |
| Unclear or missing docs | Fix it directly, or file a bug report and say it is documentation |
| Standard library (`stdlib/stdlib.ax`) | `.ax` source, generics, value semantics — no Rust needed |
| Self-hosting (`selfhost/*.ax`) | The compiler being rewritten in Aoxn; see [`docs/selfhost.md`](docs/selfhost.md) |
| Performance | Measure first. [`docs/optimization-report.md`](docs/optimization-report.md) shows the format: numbers, machine, method, before/after |

## Getting started

Prerequisites for the fully supported (Tier 1) platform:

| Requirement | Notes |
|---|---|
| **Rust** (stable, `x86_64-pc-windows-msvc` host) | `cargo build` / `cargo test` |
| **clang** | The C backend compiles the generated C and the final link step shells out to it: `AOXN_CLANG` → `PATH` → repo-local `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe` |
| **MSVC Build Tools** | clang auto-detects them; required for the MSVC host |

Since v0.29.0 the compiler carries **no LLVM dependency** — the C-emitting
backend (`src/codegen_c.rs`) is the only backend, so `cargo build` needs only
the Rust toolchain and there is no `AOXN_LLVM_DIR`/`LLVM-C` anywhere. On Tier 2
(Linux x86_64, macOS x86_64/arm64) only clang is needed (`apt-get install
clang` on Debian/Ubuntu; the preinstalled Apple clang on macOS). Read
[`docs/platform-support.md`](docs/platform-support.md) §7 before touching
platform assumptions in tests or `selfhost/`. CI runs the same suite on all
four targets (`.github/workflows/ci.yml`).

```powershell
git clone https://github.com/AlonechatWorkspace/Aoxn-language.git
cd Aoxn-language
cargo build
cargo run -- run examples\hello.ax
```

## Build and test

```powershell
cargo build                                    # build the `aoxn` compiler
cargo test                                     # the full end-to-end suite (97 tests)
cargo test --test pipeline recursion_fib       # a single test
cargo run -- run examples\hello.ax             # compile + run a program
cargo run -- build examples\fib.ax -o fib.exe  # emit a standalone executable
cargo run -- c examples\fib.ax                 # print the generated C text
cargo run -- run bad.ax --json                 # diagnostics as JSON
cargo run -- run selfhost\lex_demo.ax          # the Aoxn-written lexer
cargo run -- run selfhost\driver_self_demo.ax  # the fixed point: Aoxn compiles the compiler
```

`cargo test` compiles `.ax` sources to executables and runs them, so a full
suite run is slower than a typical Rust crate. While iterating on something big,
`--O1` cuts compile time roughly in half (O3 stays the default and is the level
that must match `clang -O3`).

Debugging switches (all environment variables, all verified in the source):

| Variable | Effect |
|---|---|
| `AOXN_DUMP_C=1` | dump the generated C text to stderr |
| `AOXN_TIME=1` | per-stage wall clock (lex / parse / typecheck / codegen / link, plus codegen sub-phases) |
| `AOXN_TC_TRACE=1` | per-function typecheck markers on stderr |
| `AOXN_CG_TRACE=1` | per-function codegen markers on stderr |
| `AOXN_CPU=native` | same as `--cpu native` (host SIMD; off by default so output stays reproducible) |
| `AOXN_NO_CACHE=1`, `AOXN_CACHE_DIR=<dir>` | disable / relocate the `aoxn run` build cache |
| `AOXN_CLANG` | override the clang path used for the C compile and final link |

## Repository layout

| Path | Contents |
|---|---|
| `src/lexer.rs` | tokens, line/col, `NEWLINE`/`INDENT`/`DEDENT` (Python-style layout) |
| `src/parser.rs`, `src/ast.rs` | recursive-descent parser → AST |
| `src/typecheck.rs` | strict type rules, `FnSig` table, generic monomorphization |
| `src/codegen_c.rs` | the C-emitting backend: ISO C99 text → `clang -c` → object file (the only backend since v0.29.0) |
| `src/lib.rs`, `src/files.rs` | import loading, diagnostics, clang link |
| `src/main.rs` | CLI: `build` / `run` / `c`, `--json`, `-l`/`-L` |
| `stdlib/stdlib.ax` | standard library, written in Aoxn |
| `examples/*.ax` | demo programs and benchmarks |
| `selfhost/*.ax` | the compiler rewritten in Aoxn (lexer → parser → typecheck → loader → codegen → driver) |
| `tests/pipeline.rs` | the end-to-end suite |
| `docs/spec.md` | language specification and roadmap |

## Changing the language

When a change touches the grammar or the semantics, it is only done when all of
this is done in the same change:

1. **`docs/spec.md`** — the normative spec is updated (it is the contract).
2. **`src/parser.rs`** and **`src/typecheck.rs`** — parse it, then check it.
3. **`src/codegen_c.rs`** — emit it.
4. **`tests/pipeline.rs`** — a test that would fail before the change.
5. **`selfhost/*.ax`** — port the same behavior to the Aoxn-written compiler.
   The fixed point (`selfhost/driver_self_demo.ax`) compares the generated C
   text **and the emitted object** byte-for-byte between the Rust-built and
   the Aoxn-built compiler, so any `target_os()`-style compile-time fold must
   agree in both implementations. It runs on every platform with clang (since
   v0.29.0 no LLVM library is involved); the self-host tests skip themselves
   only when clang cannot be found.
6. **`CHANGELOG.md`** — a version bump always comes with an entry.

House rules that are deliberately strict (proposals to relax them are language
proposals, not drive-by patches):

- **No implicit `int`/`float` conversions**, no re-typing a bound name, `bool`
  conditions only, all paths must return, no unreachable code.
- **No new external crates.** The compiler is Rust-only with zero dependencies;
  the generated C is the integration surface with the outside world.
- **Codegen rests on the C mapping.** Structs map to C structs, arrays are
  wrapped in single-field structs (`typedef struct { T data[N]; }`) so they copy
  by value like the spec says, and no C standard header is ever included — the
  runtime surface uses clang `__builtin_*` forms plus the program's own
  `extern def` declarations. Before touching emission, read the module comment
  in `src/codegen_c.rs`; the observable rules are in
  [`docs/spec.md`](docs/spec.md).
- **Compiler failures surface as `internal` diagnostics, never panics.**
- Array indexing is unchecked and raw memory builtins are unsafe **by design**;
  that is documented behavior, not a bug to fix in passing.

## Code conventions

- Rust: `cargo fmt` before committing; keep the warning count clean.
- Aoxn: 4-space indentation, `#` comments (`//` is integer division), `.ax`
  extension, and no statement-terminating semicolons (`;` appears only inside
  `[T; N]` array types).
- **Never write a UTF-8 BOM into a `.ax` file.** Aoxn sources are read byte-wise,
  so `EF BB BF` is an "unexpected character" at 1:1. PowerShell 5.1's
  `-Encoding UTF8` adds one — prefer editor writes over shell heredocs, and if
  you do rewrite a `.ax` file from a script, strip the BOM.
- Prefer ASCII in test fixtures and fixture paths: the self-hosted loader opens
  files with narrow `fopen`, so non-ASCII paths fail there even though the Rust
  compiler handles them.
- Delete throwaway probe files (scratch `.ps1` / `.ax` / logs) before committing
  so they never land in history.

## Tests

`tests/pipeline.rs` is the suite that matters: it compiles `.ax` source to a
native executable and checks its stdout / exit code. Add a test whenever you
change observable behavior, and give it a name that reads like the invariant it
protects (`optimization_levels_agree_on_program_output`,
`selfhost_driver_links_hello`).

- Indented Aoxn sources embedded in tests go through the `dedent()` helper —
  strip the common leading whitespace before compiling.
- Prefer the platform helpers in `src/platform.rs` over literals such as
  `.exe` (some older tests still hardcode it — do not add more of the same);
  the suite must pass on Windows, Linux, and macOS.
- Do not weaken or delete an existing test to make a change fit. If a test
  encodes outdated behavior, change it deliberately and say so in the PR.

## Commits and pull requests

**Commit policy for this repository: full commits.** Every commit includes the
whole working tree:

```powershell
git add -A            # not `git add <file>`, not `git add .`
git commit -m "..."
git status            # verify nothing is left behind
git push
```

This applies to source, tests, docs, `selfhost/` sources, CI config, and any new
file created in the session. Work happens on `main` and is pushed, so CI
exercises every change on all four platforms.

Using an AI agent to do the work does not change the policy: run `git add -A`,
commit the whole tree, and push.

Pull requests:

1. One logical change per PR; keep unrelated cleanups in a separate one.
2. Fill in [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md)
   — the summary, the verification commands you ran, and the checklist.
3. Green CI on Windows (Tier 1) and the Tier 2 targets is required before
   review. `cargo test` must pass locally too; paste the result if anything is
   platform-specific.
4. Performance claims need numbers and a method (machine, warm/cold, best of N).
   Prefer `--release` for benchmarks: the dev profile is `opt-level = 1`.
5. Expect review to push on the spec, the tests, and the fixed point. That is
   the point of the project.

## AI-assisted contributions

AI agents and automated tooling are welcome and explicitly anticipated — Aoxn's
`--json` diagnostics and strict semantics exist to make machine-generated code
verifiable.

Rules for them:

- **Disclose it** in the PR description ("generated with <tool>, reviewed by me").
- **Verify every claim.** A generated patch must actually build, actually pass
  `cargo test`, and must not invent builtins, benchmarks, or spec text.
  Run the commands; do not report what the tool said would happen.
- **Keep the diff reviewable.** No bulk reformatting, no unrelated rewrite of a
  file your change barely touches, no unreviewed dump of generated files.
- **You own the result.** If a reviewer finds a problem in AI-written code, it
  is your patch, not the tool's.

## Questions

- Language rules: [`docs/spec.md`](docs/spec.md)
- Self-hosting plan and status: [`docs/selfhost.md`](docs/selfhost.md)
- Release history: [`CHANGELOG.md`](CHANGELOG.md)
- Anything else: open a discussion, or write to **zmjsjsg3@163.com**

Security issues go to the same address, but as described in
[SECURITY.md](SECURITY.md).
