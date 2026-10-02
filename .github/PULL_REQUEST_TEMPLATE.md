<!--
Thanks for contributing to Aoxn. Please fill in every section; delete the
guidance comments and anything that does not apply. Keep one logical change per
pull request. Security fixes: do not describe the exploit here — see SECURITY.md.
-->

## Summary

<!-- What does this change, in one or two sentences? State the observable effect. -->

## Related issues

<!-- Fixes #123, or "none". Link the language proposal if this implements one. -->

## Type of change

- [ ] Bug fix (no behavior change intended beyond the defect)
- [ ] Language change (grammar / typing / semantics) — spec + both compilers
- [ ] Tooling / CLI / diagnostics
- [ ] Standard library (`stdlib/stdlib.ax`)
- [ ] Self-hosting port (`selfhost/*.ax`)
- [ ] Performance (numbers required below)
- [ ] Documentation / CI / build
- [ ] Refactor with no behavior change

## What changed

<!-- Bullet points, grouped by area (lexer / parser / typecheck / codegen /
lib / CLI / stdlib / selfhost / docs / CI). Mention every file whose behavior
changed, and why. -->

## How this was verified

<!--
Paste the exact commands and their results. Reviewers trust reproducible
output, not "tests pass".
-->

```text
> cargo build
> cargo test                # 97 tests before this change; report the new total
> cargo run -- run examples\hello.ax
```

<!-- For a language change, show a program that exercises the new behavior.
For a performance change, give machine, method, and before/after numbers.
For a codegen change, note whether the IR (`aoxn ir`) or object output changed. -->

## Compatibility and risk

<!-- Answer each that applies; delete the rest. -->

- **Spec:** does this change `docs/spec.md`? (link the section, or "no spec change")
- **Source compatibility:** existing valid programs still compile and behave identically? If not, what breaks?
- **Self-hosting:** which `selfhost/*.ax` ports change, and does the byte-exact fixed-point test
  (Windows-only today; it self-skips elsewhere) still compare identical IR / object output?
- **Codegen changes:** which C constructs are now emitted, and did the generated
  C still compile with the pinned clang?
- **New dependencies:** none expected — this project has zero external crates.
  If you added one, justify it here.
- **Platform:** Windows tested (`src/platform.rs` helpers)? Anything that changes
  the single-file installer's behaviour or the payload layout?
- **Rollback:** how to revert safely if this turns out wrong.

## Checklist

- [ ] `cargo build` succeeds with no new warnings, and `cargo fmt` was run.
- [ ] `cargo test` passes locally (state the total test count).
- [ ] Tests added or updated in `tests/pipeline.rs` for every observable behavior change.
- [ ] `docs/spec.md` updated for grammar / typing / semantics changes.
- [ ] `CHANGELOG.md` updated if this is a version bump.
- [ ] `selfhost/*.ax` ports updated and kept in sync with the Rust compiler.
- [ ] No new external crates; codegen output still compiles with plain clang.
- [ ] No UTF-8 BOM in any `.ax` file (Aoxn reads sources byte-wise).
- [ ] Throwaway probe files (scratch scripts, dumps, logs) deleted before committing.
- [ ] Commit uses the repository policy: `git add -A` (whole working tree), then push.
- [ ] No existing test weakened or removed to make this pass.
- [ ] I agree to follow this project's [Code of Conduct](../CODE_OF_CONDUCT.md).

## AI-assisted work

<!--
If any part of this patch, its tests, or its description was produced with an AI
agent, disclose it here ("generated with <tool>, reviewed and verified by me").
You are responsible for its correctness: it must build, pass the suite, and make
no invented claims about APIs, symbols, or benchmarks.
-->

- [ ] No AI assistance used, or it is disclosed here and I verified every claim myself.
