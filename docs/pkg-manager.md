# Aoxn Package Manager (`aoxn pkg`)

*Status: beta. Ships since v0.29.0; manifest entry resolution since
v0.29.1; read-only HTTP registry backend since v0.29.4; npm import bridge
since v0.29.5.*

The package manager gives Aoxn **independent** infrastructure (the W2
goal): its own manifest, its own lockfile, its own registry protocol —
with npm as a *bridge target*, not a foundation. The crate lives in
[`crates/aoxn-pkg`](../crates/aoxn-pkg) and keeps a deliberately
build-script-free dependency set (clap, serde, semver, pubgrub, sha2,
flate2+tar, dirs, thiserror).

## Quick start

```bash
aoxn init                 # aoxn.json + hello world
aoxn add http@^2          # add a registry dependency, resolve + install
aoxn tree                 # what is installed, and why
aoxn npm-import left-pad  # bridge: import an npm-hosted Aoxn package
aoxn publish              # ship this package to the registry
```

Every `pkg` subcommand is also a direct alias of `aoxn` (`aoxn install`,
`aoxn outdated`, …).

## The three artifacts

| artifact | role |
|---|---|
| `aoxn.json` | the manifest: identity, entry points, dependencies, registries |
| `aoxn.lock` | the lockfile: resolved versions + manifest hashes, shared across the workspace |
| `aox_modules/<name>/` | the *fully managed* install dir: recreated on every install, never edited by hand |

### Manifest (`aoxn.json`)

```json
{
  "name": "my-app",
  "version": "0.1.0",
  "main": "src/main.ax",
  "exports": { ".": "src/lib.ax", "./client": "src/client.ax" },
  "types": "src/types.ax",
  "dependencies": {
    "http": "^1.2",
    "json": { "version": "^1", "registry": "main" },
    "my-utils": { "path": "../my-utils" }
  },
  "registries": {
    "default": { "url": "https://github.com/you/aoxn-registry" },
    "mirror":  { "url": "http://mirror.lan:8080/aoxn", "kind": "http" }
  },
  "workspace": { "members": ["packages/*"] }
}
```

- `main` / `exports` / `types` (v0.29.1): the compiler resolves bare
  package imports (`import * from "http"`, `import * from "http/client"`)
  against the installed package's `aoxn.json` — `exports["."]` is the root
  entry, `exports["./client"]` the `http/client` subpath. Packages without
  a manifest fall back to the legacy `index.ax` probe. The reader is
  `src/pkg_manifest.rs`, a zero-dependency JSON parser (the compiler's
  `src/` crate stays free of external crates).
- dependencies are a semver requirement string (registry dep against the
  default registry) or an object with `version` + `registry` override, or
  a `path` (local/workspace member — resolved live, no registry).
- `registries` entries: a git URL (default), a directory (`kind: "dir"`),
  or an HTTP mirror (`http://` URL, `kind: "http"` optional). Priority for
  un-hinted deps: `default` key → first declared → `$AOXN_REGISTRY` →
  global config.

### Lockfile (`aoxn.lock`)

Records every resolved package's version, manifest hash and source
registry, plus a requirements fingerprint of the manifests. `install`
skips re-resolution when the fingerprint matches (works offline);
`--frozen` (CI) fails instead of re-resolving. One lockfile covers the
whole workspace.

### `aox_modules/` materialization

- registry deps: the tarball is extracted to `aox_modules/<name>/`; if the
  entry is not a root-level `index.ax`, a one-line shim `index.ax`
  (`import * from "./<entry>"`) is generated — the loader merges names
  transitively, so the shim is transparent.
- path deps: `aox_modules/<name>/` holds only a shim pointing at the real
  source — edits are live.
- everything in `aox_modules/` is managed; stale entries are pruned on
  every install. Do not commit it; do not edit it.

## Resolution

Single-stage [PubGrub](https://github.com/pubgrub-rs/pubgrub) over the
whole dependency graph with **soft preference**: a locked version is
preferred, otherwise the newest non-yanked version. Conflicts print a full
derivation with `aoxn <cmd> --explain`. Newly added packages run through a
**typosquat guard**: a name close to an existing registry package requires
explicit confirmation.

## Registries

All three backends serve the same tree layout:

```text
packages/<name>/index.json       # PackageIndex: version -> {checksum, deps, yanked, …}
packages/<name>/<version>.tar.gz
packages/names.json              # full name list (HTTP backend only; feeds the typosquat guard)
```

| backend | read | write | notes |
|---|---|---|---|
| **git** (default) | shallow clone into the global cache | commit + tag `pkg/<name>/v<ver>` + push (retry loop for concurrent publishes) | zero infrastructure; offline installs read the last snapshot |
| **dir** | direct file access | direct file access | air-gapped CI, tests |
| **http** (v0.29.4) | plain HTTP/1.1 GET | **read-only** | a static file server or mirror is enough; publish through the git/dir backend that owns the tree |

Publishing is idempotent (same name+version+checksum ⇒ success) and
versions must be strictly increasing per package. `aoxn yank <pkg> <ver>`
marks a version unresolvable for *fresh* resolution; existing lockfiles
keep working.

### The HTTP backend's constraints (by design)

The client is hand-rolled on `std::net::TcpStream` because the crate's
dependency set must stay free of build scripts — which rules out every
TLS-capable HTTP crate. Consequences: only `http://` URLs are accepted
(`https://` fails early with an explanation; put a local reverse proxy in
front of a remote registry), requests use `Connection: close`, and both
`Content-Length` and chunked bodies are supported. Tarball downloads are
**sha512/sha256-verified at download** before entering the cache.

## npm bridge (`aoxn npm-import`)

A **one-shot import tool**, not a proxy (decided 2026-10-02): the npm CLI
is the transport — `npm view` for metadata, `npm pack` for the tarball —
so auth, https and private registries come from the user's `.npmrc` for
free, and this crate never needs TLS.

```bash
aoxn npm-import my-pkg@1.2.0      # one package
aoxn npm-import --from package.json   # every dep of an npm package.json
```

Only **Aoxn packages** can be imported: the npm tarball must contain an
`aoxn.json` in its package root. Plain JavaScript packages are rejected
explicitly (Aoxn cannot link JavaScript). An imported package lands in
`vendor/<name>/` and is recorded in `aoxn.json` as a path dependency
(`{"path": "vendor/<name>"}`) — reusing the entire install pipeline with
zero new concepts. `vendor/` is excluded from packing. The npm tarball is
verified against `dist.integrity` (sha512) before it is unpacked.
`AOXN_NPM` overrides the npm binary (default: `npm.cmd` on Windows).

## Cache (`aoxn cache`)

Content-addressed under `$AOXN_HOME` (default `~/.aoxn`):
`tarballs/<sha256>.tar.gz` and per-registry snapshots
(`registries/<sha8(url)>/`). Safe to share read-only between CI workers.
`aoxn cache gc` drops unreferenced tarballs (TTL-protected), `prune`
drops registry snapshots, `clean` deletes everything.

## Security model

- **Integrity = manifest hash**, not tarball bytes:
  `sha256(Σ "<rel>\0<sha256(file)>\n" over sorted files)`, computed at
  publish, pinned in `aoxn.lock`, re-verified over the extracted tree at
  install — the bytes you run are exactly what the checksum describes,
  immune to tar/gzip quirks and re-packing.
- tarball extraction rejects `..` traversal entries (supply-chain hard
  stop).
- typosquat guard on newly added packages.
- `aoxn audit` checks the lockfile against an advisory database (git repo
  or local dir, configured globally).
- HTTP backend verifies tarball sha512 at download; git/dir backends
  verify at extraction.
- yanked versions never enter fresh resolution; lockfiles pinning them
  keep working (migration, not breakage).

## Testing

`bash run_pkg_tests.sh` (48 tests; it exists because Windows Smart App
Control blocks freshly built unsigned test binaries after their first
runs — the script bumps a marker so each build gets a fresh hash). The
HTTP backend is tested against an in-process `TcpListener` static server;
the npm bridge against npm-layout tarballs packed in-process (no network).
