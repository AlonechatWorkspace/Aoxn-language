/**
 * The language-service logic, over the compiler's symbol table.
 *
 * `aoxn symbols --json` hands over every top-level declaration of a program
 * and of everything it imports, with a signature and a source position for
 * each. Everything the workbench does with that — an outline, a "go to
 * definition", a workspace symbol search — is decided here, in pure
 * functions, because those decisions are the parts worth testing and the
 * parts that must not depend on a React render.
 *
 * Three decisions are worth stating, because each has a wrong answer that
 * looks reasonable:
 *
 * 1. **The outline is per-file, not per-program.** The table spans every
 *    import, so drawing all of it would list `stdlib`'s several hundred
 *    declarations under every file the user opens. A reader wants the file
 *    in front of them; the imports are reached by jumping to them.
 *
 * 2. **Matching is case-insensitive and substring-based, but a prefix
 *    match ranks first.** `clamp` should find `clamp_i` before it finds
 *    `reclamp`, the way every command palette behaves. Ranking is done by
 *    SCORE rather than by two passes over the list so the ordering is
 *    defined in one readable place.
 *
 * 3. **A jump goes to the DECLARATION, not to a call site.** When several
 *    symbols share a name (an overload is not in the language, but a name
 *    can be declared in two files a program imports), the one in the file
 *    being edited wins, because that is the one the reader is looking at;
 *    only if the current file has none does an imported one answer.
 *
 *     pnpm test
 */

import type { SymbolInfo, SymbolTable } from './bridge.ts'
import { basename } from './diagnostics.ts'

/** One row of the outline. */
export interface OutlineRow {
  symbol: SymbolInfo
  /** Nesting depth among the file's own declarations (structs are 0,
   *  functions are 0 too — Aoxn has no nested declarations, so the outline
   *  is flat and this exists for the row's own indentation to stay stable
   *  if that ever changes). */
  depth: number
}

/**
 * The outline of one file: its own declarations, in source order.
 *
 * `currentPath` is matched loosely (see `sameFile`) because the table's
 * paths come from the compiler and the workbench's come from the tree, and
 * the two agree on spelling only after both have normalised separators.
 */
export function outlineFor(table: SymbolTable, currentPath: string): OutlineRow[] {
  return table.symbols
    .filter((s) => sameFile(s.file, currentPath))
    .sort((a, b) => a.line - b.line || a.col - b.col)
    .map((symbol) => ({ symbol, depth: 0 }))
}

/** The identifier under the caret, or `''` when there is not one. */
export function wordAt(text: string, line: number, column: number): string {
  const lines = text.split('\n')
  const src = lines[line - 1]
  if (src === undefined) return ''
  // `column` is 1-based (Monaco and the compiler both say so); convert once
  // here rather than making every caller remember.
  let start = column - 1
  if (start < 0) start = 0
  if (start > src.length) start = src.length
  let end = start
  const isWord = (c: string): boolean => /[A-Za-z0-9_]/.test(c)
  // The character UNDER the caret decides, not the ones beside it. Without
  // this, a caret resting just past `def` (on the following space) would
  // expand backwards over the identifier and report a word the user is not
  // pointing at — and Ctrl+click would then jump somewhere they did not ask
  // for.
  if (!isWord(src[start] ?? '')) return ''
  while (start > 0 && isWord(src[start - 1])) start--
  while (end < src.length && isWord(src[end])) end++
  return src.slice(start, end)
}

/** What "go to definition" resolved to, or `null` when it could not. */
export interface JumpTarget {
  path: string
  line: number
  column: number
  /** The full declaration's range, so the editor can select all of it. */
  endLine: number
}

/**
 * Resolve a name to its declaration.
 *
 * Prefers a declaration in the file being edited, then one from an import,
 * then nothing. An `extern def` IS a valid answer — it is where the symbol
 * is declared, even though there is no body to show — so it is not skipped.
 */
export function definitionOf(
  table: SymbolTable,
  name: string,
  currentPath: string,
): JumpTarget | null {
  const hits = table.symbols.filter((s) => s.name === name)
  if (hits.length === 0) return null
  const local = hits.find((s) => sameFile(s.file, currentPath))
  const hit = local ?? hits[0]
  return {
    path: hit.file,
    line: hit.line,
    // The AST records the `def`/`struct` KEYWORD's column, which is where
    // the declaration begins — the right place to put the caret.
    column: Math.max(1, hit.col),
    endLine: Math.max(hit.line, hit.endLine),
  }
}

/** How well a symbol matches a query. Lower sorts first; `null` = no match. */
export function score(symbol: SymbolInfo, query: string): number | null {
  if (!query) return 2 // everything matches an empty query, weakly
  const name = symbol.name.toLowerCase()
  const q = query.toLowerCase()
  if (name === q) return 0 // exact
  if (name.startsWith(q)) return 1 // prefix
  // Word-initials (`clampInt` matches `ci`) are what make a subsequence
  // search useful without it becoming a guessing game.
  if (camelBoundary(symbol.name, q)) return 2
  if (name.includes(q)) return 3
  if (symbol.signature.toLowerCase().includes(q)) return 4
  return null
}

/**
 * True when `q` spells the initials of `name`'s words: `ci` matches
 * `clampInt`, because `c` starts the name and `I` starts a new word.
 *
 * Aoxn names are `snake_case` in practice, so this fires rarely — but
 * parameters and locals are not restricted, and a subsequence search that
 * only knew about substrings would miss `parseJson` for `pj`.
 */
function camelBoundary(name: string, q: string): boolean {
  let qi = 0
  for (let i = 0; i < name.length && qi < q.length; i++) {
    const isBoundary = i === 0 || (/[a-z0-9]/.test(name[i - 1]) && /[A-Z]/.test(name[i]))
    if (isBoundary && name[i].toLowerCase() === q[qi]) qi++
  }
  return qi === q.length
}

/**
 * Workspace symbol search: the best matches for `query`, best first.
 *
 * Ties break on name and then on position so the order is stable — a list
 * that reshuffles between two identical keystrokes makes the arrow keys lie
 * about what Enter will open.
 */
export function search(table: SymbolTable, query: string, limit = 50): SymbolInfo[] {
  const scored: { symbol: SymbolInfo; score: number }[] = []
  for (const symbol of table.symbols) {
    const s = score(symbol, query)
    if (s !== null) scored.push({ symbol, score: s })
  }
  scored.sort(
    (a, b) =>
      a.score - b.score ||
      a.symbol.name.localeCompare(b.symbol.name) ||
      a.symbol.file.localeCompare(b.symbol.file) ||
      a.symbol.line - b.symbol.line,
  )
  return scored.slice(0, limit).map((e) => e.symbol)
}

/**
 * The row a symbol search should show: the kind, the name, and enough of
 * the signature to tell two declarations with the same name apart.
 */
export function symbolLabel(s: SymbolInfo): string {
  const kind = s.kind === 'struct' ? 'struct' : s.isExtern ? 'extern' : 'def'
  return `${kind}  ${s.signature}`
}

/** Where a symbol is declared, as a short place to show. */
export function symbolOrigin(s: SymbolInfo): string {
  return `${basename(s.file)}:${s.line}`
}

/**
 * Do two paths name the same file?
 *
 * Separators and case are both normalised. Separators because the compiler
 * spells a path with the platform's separator while the tree may carry a
 * different one; case because Windows and macOS filesystems do not
 * distinguish `Main.ax` from `main.ax` and a mismatch there would make the
 * outline silently empty on exactly the platform where it must not be.
 */
export function sameFile(a: string, b: string): boolean {
  if (!a || !b) return false
  const norm = (p: string): string => p.replace(/\\/g, '/').toLowerCase()
  return norm(a) === norm(b)
}
