/**
 * The packages panel's pure logic: normalizing the backend's manifest JSON
 * and validating package names typed into the panel's prompts.
 *
 * The Rust side (`pkg.rs`) already guarantees these shapes; this mirrors it
 * for the browser-mode fixture and defends the panel against a backend
 * from before the feature — the same "read text written by another program"
 * discipline the diagnostics parser follows.
 *
 *     pnpm test
 */

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
