/**
 * The packages panel's pure logic: normalizing the backend's manifest and
 * report JSON, and validating package names typed into the panel's prompts.
 *
 * The Rust side (`pkg.rs`) already guarantees these shapes; this mirrors it
 * for the browser-mode fixture and defends the panel against a backend
 * from before the feature — the same "read text written by another program"
 * discipline the diagnostics parser follows.
 *
 *     pnpm test
 */

import type { InstalledPkg, OutdatedPkg, PkgReport } from './bridge'

export interface PkgDep {
  name: string
  req: string
  dev: boolean
}

export interface PkgManifest {
  hasManifest: boolean
  name: string
  version: string
  hasLockfile: boolean
  parseError: string | null
  dependencies: PkgDep[]
  installed: string[]
}

export const EMPTY_MANIFEST: PkgManifest = {
  hasManifest: false,
  name: '',
  version: '',
  hasLockfile: false,
  parseError: null,
  dependencies: [],
  installed: [],
}

/** Normalize whatever the backend returned into a `PkgManifest`. */
export function formatManifest(raw: unknown): PkgManifest {
  if (typeof raw !== 'object' || raw === null) return { ...EMPTY_MANIFEST }
  const o = raw as Record<string, unknown>
  const str = (v: unknown) => (typeof v === 'string' ? v : '')
  const deps: PkgDep[] = Array.isArray(o.dependencies)
    ? o.dependencies
        .filter((d): d is Record<string, unknown> => typeof d === 'object' && d !== null)
        .map((d) => ({
          name: str(d.name),
          req: str(d.req),
          dev: d.dev === true,
        }))
        .filter((d) => d.name.length > 0)
    : []
  return {
    hasManifest: o.hasManifest === true,
    name: str(o.name),
    version: str(o.version),
    hasLockfile: o.hasLockfile === true,
    parseError: typeof o.parseError === 'string' ? o.parseError : null,
    dependencies: deps,
    installed: Array.isArray(o.installed)
      ? o.installed.filter((n): n is string => typeof n === 'string')
      : [],
  }
}

/** How a dependency reads in the panel: `http ^2` with a dev marker. */
export function formatDep(d: PkgDep): string {
  return d.dev ? `${d.name} ${d.req} (dev)` : `${d.name} ${d.req}`
}

/**
 * Mirror of `pkg::validate_pkg_name` in src-tauri/src/pkg.rs: package names
 * (optionally `name@req`) for Add / Why / Remove. Flag-shaped strings,
 * paths and whitespace are refused so the argument array that reaches
 * `aoxn pkg` means exactly what the panel meant.
 */
export function validatePkgName(raw: string): string | null {
  const name = raw.trim()
  if (!name) return 'Type a package name.'
  if (name.length > 100) return 'That name is too long.'
  if (name.startsWith('-')) return "A package name cannot start with '-'."
  if (/\s/.test(name)) return 'A package name cannot contain spaces.'
  if (name.includes('/') || name.includes('\\')) return 'A package name is not a path.'
  if (!/^[A-Za-z0-9_.@^~*-]+$/.test(name)) {
    return 'Only letters, digits and - _ . @ ^ ~ * are allowed in a package name.'
  }
  return null
}

export const EMPTY_REPORT: PkgReport = { installed: [], outdated: [], error: null }

/** Normalize whatever the backend returned into a `PkgReport`. */
export function formatReport(raw: unknown): PkgReport {
  if (typeof raw !== 'object' || raw === null) return { ...EMPTY_REPORT }
  const o = raw as Record<string, unknown>
  const str = (v: unknown): string => (typeof v === 'string' ? v : '')
  // A row without a name is a row about nothing: the backend already drops
  // those, and the panel drops them again so a backend from before this
  // feature cannot put a nameless line in the list.
  const rows = <T>(v: unknown, map: (e: Record<string, unknown>) => T): T[] =>
    Array.isArray(v)
      ? v
          .filter((e): e is Record<string, unknown> => typeof e === 'object' && e !== null)
          .filter((e) => str(e.name).length > 0)
          .map(map)
      : []
  return {
    installed: rows<InstalledPkg>(o.installed, (e) => ({
      name: str(e.name),
      version: str(e.version),
      scope: str(e.scope),
      source: str(e.source),
    })),
    outdated: rows<OutdatedPkg>(o.outdated, (e) => ({
      name: str(e.name),
      locked: str(e.locked),
      newest: str(e.newest),
      note: str(e.note),
    })),
    error: typeof o.error === 'string' ? o.error : null,
  }
}

/** An installed package with the version that could be newer, if any. */
export interface ResolvedDep extends InstalledPkg {
  /** The newest version known, when one is. */
  upgrade: string | null
}

/**
 * The installed inventory, each row carrying its upgrade if it has one.
 *
 * This is the join the panel exists for: the manifest says what the project
 * ASKS for (`^2`), the lockfile says what it GOT (`2.1.0`), and the
 * registry says what is available (`2.4.0`). Showing only the first is what
 * a package panel that reads `aoxn.json` alone shows, and it is the reason
 * `pip list` and `pnpm outdated` are separate commands people run.
 */
export function withUpgrades(report: PkgReport): ResolvedDep[] {
  const newer = new Map(report.outdated.map((o) => [o.name.toLowerCase(), o.newest]))
  return report.installed.map((p) => ({
    ...p,
    upgrade: newer.get(p.name.toLowerCase()) ?? null,
  }))
}

/** How an installed row reads in the panel: `http 2.1.0 (dev · local)`. */
export function formatInstalled(p: InstalledPkg, upgrade: string | null): string {
  const bits: string[] = []
  if (p.scope === 'dev') bits.push('dev')
  if (p.source && p.source !== 'registry') bits.push(p.source)
  const tail = bits.length ? ` (${bits.join(' · ')})` : ''
  return `${p.name} ${p.version}${tail}`
}

/**
 * The upgrade arrow for a row: `2.1.0 → 2.4.0`, or nothing when the
 * installed version is already the newest. A row whose name has no entry in
 * `outdated` is NOT shown as current — `outdated` failing (a registry that
 * did not answer) is different from a package being up to date, and the
 * panel distinguishes those by not claiming anything it did not verify.
 */
export function formatUpgrade(locked: string, newest: string): string {
  return locked && newest && locked !== newest ? `${locked} → ${newest}` : ''
}
