/**
 * Deriving the flat explorer tree from a path->text map.
 *
 * This exists for the browser-mode fixture (`lib/bridge.ts`), where there is
 * no Rust walker: instead of maintaining a hand-written tree that drifts
 * from the files actually created in the preview session, the tree is
 * RE-DERIVED from the fixture map on every change, with the same rules the
 * Rust side applies in `fsops::walk` — no dot entries, directories first,
 * case-insensitive by name, depth-first.
 */

import type { TreeNode } from './bridge'

export function treeFromPaths(
  paths: Record<string, string>,
  root: string,
  /** Directories kept even when they hold no file (created with New folder). */
  extraDirs: string[] = [],
): TreeNode[] {
  const sep = root.endsWith('/') ? '' : '/'
  const prefix = `${root}${sep}`
  const dirs = new Set<string>(extraDirs)

  for (const p of Object.keys(paths)) {
    if (!p.startsWith(prefix)) continue
    const rel = p.slice(prefix.length).split('/')
    // dot entries are invisible, and so is anything living inside one —
    // the same rule `fsops::walk` applies natively
    if (rel.some((seg) => seg.startsWith('.'))) continue
    // every intermediate segment is a directory; the last is the file itself
    for (let i = 1; i < rel.length; i++) {
      dirs.add(`${root}${sep}${rel.slice(0, i).join('/')}`)
    }
  }

  const out: TreeNode[] = []
  const name = (p: string) => p.slice(p.lastIndexOf('/') + 1)
  const parentOf = (p: string) => p.slice(0, p.lastIndexOf('/'))
  const byName = (a: string, b: string) =>
    a.toLowerCase().localeCompare(b.toLowerCase()) || a.localeCompare(b)

  const walk = (dir: string, depth: number): void => {
    const kidDirs: string[] = []
    for (const d of dirs) {
      if (parentOf(d) === dir) kidDirs.push(d)
    }
    const kidFiles: string[] = []
    for (const p of Object.keys(paths)) {
      if (parentOf(p) === dir && !dirs.has(p) && !name(p).startsWith('.')) kidFiles.push(p)
    }
    for (const d of kidDirs.sort(byName)) {
      out.push({ name: name(d), path: d, isDir: true, depth })
      walk(d, depth + 1)
    }
    for (const f of kidFiles.sort(byName)) {
      out.push({ name: name(f), path: f, isDir: false, depth })
    }
  }
  walk(root, 0)
  return out
}
