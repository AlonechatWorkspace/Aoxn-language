# CSS assets — `.css` as a first-class build input

> v0.34.0, extended in v0.36.0. Implements decision **B1** in
> [`web-platform-plan.md`](web-platform-plan.md) §7. Plain `.css` and CSS
> Modules both work from `.ax` and `.ts` sources; `url()` targets are
> rewritten and emitted; `styles.title` gives typed class access; Tailwind
> enters through this pipeline or through the built-in `--tailwind` generator
> rather than as a dependency — see [Tailwind](#tailwind).

## The problem this solves

Before this, a `.css` import did not fail cleanly. Module resolution's
exact-path short-circuit (`complete_module_path` in `src/lib.rs`) accepted
`./styles.css` happily, and the file was then handed to the **Aoxn lexer**,
which reported `unexpected character '{'`. Nothing in that message points at
the real problem.

Now a `.css` file reached through `import` is diverted to the asset pipeline in
`src/assets.rs` before any front end sees it, and becomes part of the program.

## Using it

Import the stylesheet like any other module:

```ax
import * from "./style.css"

def main() -> int:
    print(styles())
    return 0
```

or, in TypeScript:

```ts
import "./style.css";

function main() {
  console.log(styles());
}
```

Two functions are generated for you:

| Function | Returns |
|---|---|
| `styles() -> string` | every plain stylesheet, bundled and minified |
| `styles_fingerprint() -> string` | `<16 hex>.css`, stable for identical input |

A server can inline the result directly, which needs no file layout at all:

```ts
function page() {
  return `<!doctype html><style>${styles()}</style><h1>hi</h1>`;
}
```

`styles_fingerprint()` gives you a cache-busting name for a `<link>`:

```ts
`<link rel="stylesheet" href="/static/${styles_fingerprint()}">`
```

## What the pipeline does

1. **`@import` inlining** — `@import "./base.css";` is replaced by that file's
   contents, recursively, in place. Import order is preserved, so the cascade
   matches a browser's. A cycle is an error; a repeated import contributes its
   text once.
2. **Minification** — comments are removed and redundant whitespace collapsed.
   Deliberately *nothing else*: no selector merging, no rule reordering, no
   dropping the final `;`, no empty-rule elision, no color shortening. Each of
   those can change meaning in some CSS corner, and the payoff is cosmetic. The
   emitted text stays a pure function of the source. A comment counts as
   whitespace, so `a/**/b` never collapses into `ab`.
3. **Fingerprinting** — the 16-hex name derives from the final text, so any
   edit changes it. It uses the same `FastBuild` hasher as the build cache.

A `@import` of a **non-CSS** resource (`url("font.woff2")`) passes through
untouched. Dropping it would change rendering, and rejecting it would refuse
CSS a browser accepts.

## CSS Modules

A file named `*.module.css` has its class names scoped, and is **not** joined
into the global bundle — its rules are reachable only through the generated
accessor, which is what makes the scoping meaningful:

```css
/* page.module.css */
.title { font-weight: 700; }
```

```ts
import * from "./page.module.css";

function head() {
  return `<h1 class="${page_class("title")}">`;  // -> <h1 class="title_87b780ec">
}
```

`page.module.css` generates `page_class(name: string) -> string`, named for the
file stem with `.module` dropped. The hash seed is the file's own path, so the
same class name in two different modules gets two different hashes. An unknown
name returns `""`.

The rewriter is **context-aware**, which is the whole difficulty here. A `.`
inside a string literal (`content: ".x"`), inside an `@media` prelude
(`(min-width: 30rem)`), or inside a declaration value (`1.5rem`) is not a class
selector and is left alone. A naive text replacement gets all three wrong and
silently changes rendering; each case is pinned by a test in `tests/assets.rs`.

## Tailwind

Tailwind enters as a **pre-generated stylesheet**, not as a dependency. Run it
yourself, then import the output like any other CSS:

```sh
npx tailwindcss -i app.css -o generated.css
```

```ts
import * from "./generated.css";
```

This is deliberate. Tailwind is a plain-JavaScript npm package with no
`aoxn.json`, and `aoxn npm-import` **rejects** those by design
(`crates/aoxn-pkg/src/npm.rs`, pinned by the `plain_js_package_is_rejected`
test) — Aoxn links Aoxn packages, not JavaScript. Shelling out to the CLI the
way the compiler shells out to clang would also make Node a build prerequisite,
which the one-click Windows install deliberately avoids. Taking the generated
CSS keeps the toolchain self-contained.

A JIT scanner that discovers class names in source and generates the CSS is
**not** implemented. That is a much larger piece of work, and a
"Tailwind-compatible subset" would be a long maintenance liability.

### Built-in generation (`--tailwind`, v0.36.0)

The generator now lives in the compiler. `aoxn build app.ts --tailwind` scans
the sources and prepends the rules for the utilities it recognises:

```ts
const html = `<div class="flex p-4 text-red-500">`;   // -> .flex{...}.p-4{...}
```

- **Only a `class=` / `className=` / `class:` attribute is mined.** An earlier
  "any string shaped like a class list" heuristic was tried and removed:
  prose like `"the red-500 of it is p-4"` passes the same shape test a real
  attribute does, and would silently ship dead CSS.
- Generated rules are prepended, so a hand-written stylesheet overrides them.
- Utilities outside the subset are **named at build time** rather than
  dropped, so a gap is visible instead of mysterious.
- It is a **documented subset**, not Tailwind: the default spacing scale, a
  short palette, common flex/grid/typography/position utilities. No config
  files, no plugins, no arbitrary values (`bg-[#abc]`), no preflight reset.

## Emitting assets to disk (v0.36.0)

`--emit-assets <dir>` writes every fingerprinted artifact beside the
executable, so a server can serve them over `<link>` instead of inlining:

```sh
aoxn build app.ax -o app.exe --emit-assets assets
```

```
assets/
  app.3f2a….css      the stylesheet, url()s rewritten
  82b4fb25….css      the bundle — exactly what styles_fingerprint() returns
  logo.0d6b….png     every url() target
```

A relative path resolves against the **output executable's** directory, so the
command means the same thing from any cwd. The emitted names are
compiler-generated and re-checked at write time; a stylesheet can influence the
text of a string constant and nothing else.

A `url()` target that does not resolve **passes through with a warning**. A
stylesheet may legitimately reference a CDN font or a file another tool copies
in, and failing the build would reject valid CSS over an asset the compiler was
never asked to manage.

### Finding the directory at runtime

A compiled program cannot learn where it lives: `main` takes no arguments and
the language has no argv/cwd builtin. `stdlib/stdlib.ax` closes that with
`GetModuleFileNameA`, which kernel32 already provides through the CRT:

```Aoxn
import * from "stdlib"

def main() -> int:
    print(asset_path(styles_fingerprint()))   # …\assets\82b4fb25….css
    return 0
```

`exe_path()` and `exe_dir()` are also available. Deriving the path at runtime
(rather than baking an absolute one in at build time) is what keeps the
executable relocatable.

## Build cache

`.css` files are part of the build cache's dependency set, and so is everything
they pull in: an `@import`ed partial and a `url()` target both change the
emitted artifact, so editing either invalidates the cached executable exactly
as editing a source file does. `--emit-assets` and `--tailwind` are part of the
cache key. `AOXN_NO_CACHE=1` disables caching as usual.

## Diagnostics

Asset problems report stage `asset`, with the stylesheet's own file name and the
line of the offending `@import`:

```
[asset] web/static/style.css:3:1: @import './missing.css' does not resolve to a file in '...'
```

Warnings (a `url()` that names no local file, a utility the Tailwind subset
does not cover) go to stderr without failing the build.

## Limits

- No JSX, and `import * as ns` still needs namespace objects (TS-M2).
  `styles.title` works because the binding is rewritten to a call, not
  because namespaces exist.
- `url()` rewriting is local-file only; protocol-relative and cross-origin
  references pass through.
- Plain CSS is embedded verbatim-minified; only comments and whitespace change.
- A CSS Module's rules stay out of the global bundle, so a `<link>` to a module
  stylesheet is what makes them apply — `styles()` will not contain them.

See also: [`web-platform-plan.md`](web-platform-plan.md) §7 (the decision),
[`ts-m1-spec.md`](ts-m1-spec.md) (the TypeScript front end).