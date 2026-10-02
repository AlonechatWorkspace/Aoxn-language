# The curated registry: one repository, three jobs

*Since v0.32.0.*

A **curated registry** is a normal Aoxn registry that also carries a trust
index and an advisory database. Because all three live in one repository,
one command wires up all three:

```bash
aoxn trust bootstrap https://github.com/AlonechatWorkspace/Aoxn-trusted-third-party-package
```

The reference instance of this layout is
[AlonechatWorkspace/Aoxn-trusted-third-party-package](https://github.com/AlonechatWorkspace/Aoxn-trusted-third-party-package).
It is ordinary git infrastructure — the git registry backend was already
"clone a repo, read `packages/`" — so curating a source of packages costs a
review, not a server.

## The three jobs

| job | what the file is | who reads it | what it buys you |
|---|---|---|---|
| **registry** | `packages/<name>/index.json` + `<version>.tar.gz` | `aoxn install`, `aoxn add` | packages resolve and install |
| **trust** | `trust.json` at the root | `aoxn trust list/check`, `install` warnings | "somebody actually reviewed this" |
| **advisories** | `advisories/*.json` | `aoxn audit` | known-bad versions block a release |

`advisories/` is optional, `trust.json` is optional, `packages/` is not.
A registry that carries none of the extra files behaves exactly as before —
`aoxn trust list` then reports "this registry makes no trust claims" rather
than failing.

## On-disk layout

```text
Aoxn-trusted-third-party-package/
├── trust.json                    # the trust index (optional)
├── advisories/
│   ├── http.json                 # advisories for one package …
│   └── json.json                 # … or one file holding many
├── README.md
└── packages/
    ├── names.json                # full name list — the HTTP backend's only source
    ├── http/
    │   ├── index.json            # PackageIndex: versions -> metadata
    │   ├── 1.2.0.tar.gz
    │   └── 1.3.0.tar.gz
    └── json/
        ├── index.json
        └── 2.0.0.tar.gz
```

## `trust.json`

```json
{
  "schema": 1,
  "updated": "2026-10-02",
  "packages": {
    "http": {
      "tier": "audited",
      "reviewer": "@ryan",
      "reviewed": "2026-09-30",
      "summary": "HTTP/1.1 client, TLS-free by design",
      "advisories": "advisories/http.json"
    },
    "json": {
      "tier": "community"
    },
    "hot-new-thing": {}
  }
}
```

| field | required | meaning |
|---|---|---|
| `schema` | yes | must be `1`; aoxn refuses an index it cannot read rather than guessing |
| `updated` | no | when the list was last curated — shown by `aoxn trust list` |
| `packages` | yes | package name → review record |
| `tier` | no (`unreviewed`) | `unreviewed` \| `community` \| `audited` |
| `reviewer` | no | who did the review (`@handle` or a team slug) |
| `reviewed` | no | ISO `YYYY-MM-DD` |
| `summary` | no | one line, shown by `aoxn trust list` |
| `advisories` | no | repo-relative path, for packages with their own advisory file |

### Tiers

| tier | means | install warns? | passes `trust check` |
|---|---|---|---|
| `unreviewed` | the registry hosts it; nobody has read it | yes | no |
| `community` | read by a community member, no maintainer sign-off | no | yes (`--tier community`) |
| `audited` | reviewed and signed off by the maintainers | no | yes (`--tier audited`) |

A package the registry **hosts but does not list** is `unreviewed` too —
there is simply no entry. `aoxn trust check` distinguishes the two by
reporting "not listed" versus "unreviewed".

## Advisories

`advisories/*.json` holds either an array of advisories or a single object
(the walker recurses, so a nested layout works). Two shapes are accepted:

**Native**:

```json
[
  {
    "id": "AOXN-2026-0001",
    "package": "http",
    "vulnerable": "<1.4.1",
    "patched": "1.4.1",
    "severity": "high",
    "url": "https://example.com/advisory/1"
  }
]
```

**OSV** (an `osv.dev` export works unconverted):

```json
{
  "id": "OSV-2026-1234",
  "affected": [
    {
      "package": { "name": "http" },
      "ranges": [
        { "type": "SEMVER",
          "events": [{ "introduced": "0" }, { "fixed": "1.4.1" }] }
      ]
    }
  ]
}
```

Severities understood by `--audit-level`: `low`, `medium` (or `moderate`),
`high`, `critical`. An advisory with no severity sorts lowest, so
`--audit-level medium` does not accidentally fail on records that simply
never filled the field in.

## Commands

```bash
# wire default registry + advisories + trust to one URL (writes $AOXN_HOME/config.json)
aoxn trust bootstrap https://github.com/AlonechatWorkspace/Aoxn-trusted-third-party-package

# what has been reviewed
aoxn trust list
aoxn trust list --json

# CI gate: fail unless the package is reviewed at the required tier
aoxn trust check http --tier audited

# advisories (finds the registry's advisories/ automatically when no
# advisories source is configured)
aoxn audit
aoxn audit --audit-level high --json
aoxn audit --fix
```

`aoxn trust bootstrap` verifies the URL really is a curated registry
**before** writing the config, so a typo fails immediately rather than on
every later install.

## What a trust record is not

The trust index is a **curation signal surfaced to you**. It is not:

- a signature or proof of provenance — nothing is cryptographically signed,
- an allowlist that blocks installs on its own — installing an unreviewed
  package warns, it does not fail,
- a replacement for `aoxn.lock`. The cryptographic anchor is still the
  manifest hash pinned there; the trust index says *who looked at the
  source*, the lockfile says *what exactly you are compiling*.

If you need to refuse unreviewed packages in CI, that is
`aoxn trust check <pkg> --tier audited` in a script — deliberately a
separate, visible step rather than a hidden default.
