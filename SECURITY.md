# Security Policy

Aoxn is a compiler: it consumes untrusted text (`.ax` sources, import paths) and
produces native machine code that people then execute. Defects that turn that
pipeline into memory corruption or code execution are security issues, and we
want to hear about them privately before they are public.

## Supported versions

Aoxn is pre-1.0. Only the latest release line receives security fixes; older
minors do not.

| Version | Supported |
|---|---|
| `0.26.x` (current) | ✅ yes |
| `0.25.x` and earlier | ❌ no — please reproduce on `main` or the latest release |
| `main` (development) | ✅ yes, fixes land here first |

Fix versions are always noted in [`CHANGELOG.md`](CHANGELOG.md). If you need a
fix backported to an older tag, say so in your report and we will discuss it.

## Reporting a vulnerability

**Email `zmjsjsg3@163.com`.** Please do not open a public issue, and do not post
the details in a discussion, chat, or social media before a fix is available.

A useful report contains:

1. **Version** — the first line of `aoxn --help` (or the `version` field in
   `Cargo.toml`), and the commit/tag if you build from source.
2. **Platform** — OS and architecture, LLVM version, and whether you are on a
   Tier 1 (Windows) or Tier 2 (Linux / macOS) target; see
   [`docs/platform-support.md`](docs/platform-support.md).
3. **Reproduction** — the smallest `.ax` source you can manage, the exact
   command you ran, and observed versus expected behavior. If the problem is in
   generated code, include the IR (`aoxn ir file.ax`, or `AOXN_DUMP_IR=1`) and
   say whether it still reproduces with `--O0`.
4. **Impact** — what an attacker gains and what they must already control
   (e.g. "compiling this source executes a shell command" or "the emitted
   function writes past a stack buffer").
5. **Credit** — the name or handle you want used, or a request to stay anonymous.

If you want to encrypt the report, say so in a first email with no details and
we will arrange a channel.

### What to expect

| Stage | Target |
|---|---|
| Acknowledgement of your report | within 3 business days |
| Initial assessment (accepted / not a vulnerability / need more info) | within 10 business days |
| Fix for a confirmed critical issue | next release, or a point release if the line is otherwise closed |
| Coordinated disclosure | 90 days after acknowledgement, or as soon as a fix ships — whichever comes first |

We will keep you updated at each stage and ask before publishing anything that
credits you. If we conclude something is not a vulnerability, we will explain
why, and we are happy to be corrected.

There is no bug bounty. We will credit reporters in the advisory and the
changelog unless you prefer otherwise. One thing we will not do is trade a fix
for silence: if an issue is being exploited in the wild, we will publish with
the information needed to protect users even if the reporter disagrees.

## In scope

- **Memory corruption or code execution in generated code** that the Aoxn source
  did not opt into — wrong `memcpy` sizes, aggregate copy/ABI mistakes, bad
  `inbounds`/`nsw` flags, string/buffer length mishandling, miscompilation that
  turns a defined program into an unsafe one.
- **Code execution or command injection in the toolchain** — the final link step
  shells out to `clang`, and the self-hosted driver calls `system("clang ...")`.
  Crafted file names, `-l`/`-L` values, or paths that escape into a shell are
  in scope.
- **Memory-safety defects inside the compiler itself** — unsafe Rust, use of
  freed or recycled AST nodes, misuse of the hand-written LLVM-C FFI in
  `src/llvm.rs`.
- **Build and supply-chain integrity** — `build.rs`, the CI workflow, the
  published artifacts, or anything that lets a source file or config influence
  the compiler's own binaries. (The crate has zero external dependencies by
  design, which is also a security property; a PR that adds one needs a very
  good reason.)
- **Build-cache poisoning** — `aoxn run` executes executables from
  `target/cache` based on a content hash; a way to run attacker-controlled code
  through that path is a vulnerability.
- **Denial of service that is not just "a bad program"** — a small, well-formed
  input that hangs the compiler indefinitely or exhausts memory catastrophically
  is worth reporting; see the note below on where the line is.

## Out of scope (documented behavior)

These are deliberate design decisions, documented in
[`docs/spec.md`](docs/spec.md). Please do not report them as vulnerabilities —
though a bug report about the *documentation* is welcome.

- **Unchecked array indexing and signed integer overflow.** Like C, these are
  undefined behavior in the language contract. Indexing out of bounds or
  overflowing an `int` in Aoxn source is the program's bug, not the compiler's.
- **Raw memory builtins** (`load_i64`, `store_i64`, `load_f64`, `store_f64`,
  `load_u8`, `store_u8`, `as_ptr`, `as_string`). These exist as the
  self-hosting escape hatch and are unsafe by design; non-`inbounds` GEPs are
  intentional.
- **Memory growth from string concatenation.** Concat results are never freed
  (immutable strings, no GC yet). It is stated behavior, not a leak bug.
- **Compiler crashes, hangs, or wrong error messages on malformed input.** These
  are bugs — file them with the
  [bug report template](https://github.com/Ryan-178/Aoxn-language/issues/new?template=bug_report.yml).
  Escalate to a security report only if the failure involves memory corruption
  in the compiler process, code execution, or a wrong-code emission that
  silently makes a valid program unsafe.
- **Vulnerabilities in programs people compile with Aoxn.** Aoxn provides no
  sandbox and no runtime safety net; the compiled program's behavior is the
  program author's responsibility.
- **Upstream LLVM / clang / MSVC defects.** Report those upstream — but do tell
  us if the compiler depends on the broken behavior.
- **Anything requiring an attacker who already controls the machine** or the
  terminal the compiler runs in. The documented `AOXN_*` knobs are
  configuration, not an attack surface — but a crafted value that escapes into
  the link command or poisons the build cache is in scope (see above).

## Safe harbor

We will not pursue or support legal action against researchers who:

- act in good faith and follow this policy,
- test only against their own builds and data,
- avoid privacy violations, data destruction, and disruption of services they do
  not own,
- give us reasonable time to fix the issue before public disclosure, and
- do not use social engineering, physical attacks, or denial-of-service against
  infrastructure (the compiler is a local tool; there is no service to test).

If you are unsure whether something is in scope, ask first — email
`zmjsjsg3@163.com` with just enough detail to describe the area, and we will tell
you how we want it handled.
