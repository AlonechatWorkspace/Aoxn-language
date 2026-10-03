/**
 * Tests for the language-service logic over the compiler's symbol table.
 *
 * The ranking, the jump resolution and the per-file outline are the three
 * decisions a reader notices when they are wrong, and none of them need a
 * React render to exercise — so they live in `lib/symbols.ts` as pure
 * functions and are tested here. `node --test`, no framework: a dependency
 * that exists only to assert `assert.equal` is not worth it.
 *
 *     pnpm test
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
  definitionOf,
  outlineFor,
  sameFile,
  score,
  search,
  symbolLabel,
  symbolOrigin,
  wordAt,
} from '../lib/symbols.ts'
import type { SymbolInfo, SymbolTable } from '../lib/bridge.ts'

/** A symbol with sensible defaults, so each test states only what it cares
 *  about — a test that has to fill in twelve fields to check a ranking is a
 *  test that will be wrong in a way the reader cannot see. */
function sym(over: Partial<SymbolInfo> = {}): SymbolInfo {
  return {
    kind: 'function',
    name: 'f',
    signature: 'def f()',
    file: 'C:/p/main.ax',
    line: 1,
    col: 1,
    endLine: 2,
    ret: 'void',
    isExtern: false,
    params: [],
    fields: [],
    ...over,
  }
}

const MAIN = 'C:/p/main.ax'
const UTIL = 'C:/p/util.ax'

test('the outline shows one file, not the whole program', () => {
  // The table spans every import. Listing all of it would put stdlib's
  // declarations under every file the reader opens.
  const table: SymbolTable = {
    symbols: [
      sym({ name: 'main', file: MAIN, line: 10 }),
      sym({ name: 'helper', file: MAIN, line: 20 }),
      sym({ name: 'clamp_i', file: UTIL, line: 1 }),
    ],
  }
  const rows = outlineFor(table, MAIN)
  assert.deepEqual(
    rows.map((r) => r.symbol.name),
    ['main', 'helper'],
  )
})

test('the outline is in source order', () => {
  const table: SymbolTable = {
    symbols: [
      sym({ name: 'third', file: MAIN, line: 30 }),
      sym({ name: 'first', file: MAIN, line: 10 }),
      sym({ name: 'second', file: MAIN, line: 20 }),
    ],
  }
  assert.deepEqual(
    outlineFor(table, MAIN).map((r) => r.symbol.name),
    ['first', 'second', 'third'],
  )
})

test('an outline path spelled with the other separator still matches', () => {
  // The compiler spells a path with the platform separator; the tree may
  // carry the other one. A mismatch here makes the outline silently empty.
  const table: SymbolTable = { symbols: [sym({ name: 'main', file: 'C:\\p\\main.ax' })] }
  assert.equal(outlineFor(table, 'C:/p/main.ax').length, 1)
  assert.equal(outlineFor(table, 'C:\\p\\main.ax').length, 1)
})

test('an empty table yields an empty outline rather than throwing', () => {
  assert.deepEqual(outlineFor({ symbols: [] }, MAIN), [])
})

test('wordAt reads the identifier under the caret', () => {
  const text = 'def clamp_i(v: int) -> int:\n    return clamp_i(3, 0, 9)\n'
  // 1-based line and column, the way Monaco and the compiler both report.
  // `clamp_i` starts at column 12 of the second line; column 20 is the `3`.
  assert.equal(wordAt(text, 1, 5), 'clamp_i')
  assert.equal(wordAt(text, 2, 12), 'clamp_i')
  assert.equal(wordAt(text, 2, 20), '3')
})

test('wordAt returns nothing on whitespace, past the end, or off the file', () => {
  const text = 'def f() -> int:\n    return 1\n'
  assert.equal(wordAt(text, 1, 1), 'def')
  // Column 4 is the space after `def`: the caret is NOT on the identifier,
  // and reporting one here would make Ctrl+click jump to a name the user
  // is not pointing at.
  assert.equal(wordAt(text, 1, 4), '')
  assert.equal(wordAt(text, 1, 5), 'f', 'column 5 is the `f`')
  assert.equal(wordAt(text, 1, 999), '')
  assert.equal(wordAt(text, 99, 1), '')
  assert.equal(wordAt('', 1, 1), '')
})

test('go to definition finds a declaration in another file', () => {
  // The cross-file case: the name is used in main.ax and declared in
  // util.ax, which is what an import means.
  const table: SymbolTable = { symbols: [sym({ name: 'clamp_i', file: UTIL, line: 4, col: 1 })] }
  const target = definitionOf(table, 'clamp_i', MAIN)
  assert.ok(target)
  assert.equal(target.path, UTIL)
  assert.equal(target.line, 4)
})

test('go to definition prefers the current file when a name is declared twice', () => {
  const table: SymbolTable = {
    symbols: [
      sym({ name: 'step', file: UTIL, line: 1 }),
      sym({ name: 'step', file: MAIN, line: 30 }),
    ],
  }
  assert.equal(definitionOf(table, 'step', MAIN)?.path, MAIN)
})

test('go to definition finds nothing for an unknown name', () => {
  const table: SymbolTable = { symbols: [sym({ name: 'known' })] }
  assert.equal(definitionOf(table, 'unknown', MAIN), null)
  assert.equal(definitionOf({ symbols: [] }, 'anything', MAIN), null)
})

test('an extern declaration is a valid jump target', () => {
  // It has no body, but it IS where the symbol is declared — refusing it
  // would make every C-backed call a dead Ctrl+click.
  const table: SymbolTable = {
    symbols: [sym({ name: 'GetTickCount', file: MAIN, isExtern: true, endLine: 1 })],
  }
  const target = definitionOf(table, 'GetTickCount', MAIN)
  assert.ok(target)
  assert.equal(target.line, 1)
})

test('a jump selects the whole declaration, never a range below its start', () => {
  const table: SymbolTable = {
    symbols: [sym({ name: 'f', line: 10, endLine: 3 })],
  }
  assert.equal(definitionOf(table, 'f', MAIN)?.endLine, 10)
})

test('ranking puts exact, then prefix, then substring', () => {
  assert.equal(score(sym({ name: 'clamp' }), 'clamp'), 0)
  assert.equal(score(sym({ name: 'clamp_i' }), 'clamp'), 1)
  assert.equal(score(sym({ name: 'reclamp' }), 'clamp'), 3)
  assert.equal(score(sym({ name: 'other' }), 'clamp'), null)
})

test('ranking is case-insensitive', () => {
  assert.equal(score(sym({ name: 'Clamp' }), 'clamp'), 0)
  assert.equal(score(sym({ name: 'clamp_i' }), 'CLAMP'), 1)
})

test('a word-initials match beats a plain substring one', () => {
  // `clampInt` for `ci`: `c` starts the name, `I` starts a new word. This
  // is what makes a subsequence search useful without becoming a guess.
  assert.equal(score(sym({ name: 'clampInt' }), 'ci'), 2)
  assert.equal(score(sym({ name: 'xcci' }), 'ci'), 3, 'no boundary, substring only')
  assert.equal(score(sym({ name: 'clamp_i' }), 'ci'), null, 'not a subsequence at all')
})

test('an empty query matches everything, weakly', () => {
  assert.equal(score(sym({ name: 'anything' }), ''), 2)
})

test('search returns best matches first and respects the limit', () => {
  const table: SymbolTable = {
    symbols: [
      sym({ name: 'reclamp', file: MAIN }),
      sym({ name: 'clamp_i', file: UTIL }),
      sym({ name: 'clamp', file: MAIN }),
      sym({ name: 'unrelated', file: MAIN }),
    ],
  }
  assert.deepEqual(
    search(table, 'clamp').map((s) => s.name),
    ['clamp', 'clamp_i', 'reclamp'],
  )
  assert.equal(search(table, 'clamp', 2).length, 2)
  assert.equal(search(table, 'zzz').length, 0)
})

test('search order is stable between two identical queries', () => {
  // A list that reshuffles makes the arrow keys lie about what Enter opens.
  const table: SymbolTable = {
    symbols: [
      sym({ name: 'aa', file: 'b.ax', line: 5 }),
      sym({ name: 'aa', file: 'a.ax', line: 9 }),
    ],
  }
  assert.deepEqual(
    search(table, 'aa').map((s) => `${s.file}:${s.line}`),
    search(table, 'aa').map((s) => `${s.file}:${s.line}`),
  )
})

test('search also matches inside a signature, so a parameter finds its function', () => {
  const table: SymbolTable = {
    symbols: [sym({ name: 'clamp_i', signature: 'def clamp_i(v: int, lo: int) -> int' })],
  }
  assert.equal(search(table, 'lo: int').length, 1)
})

test('a symbol row shows its kind, its signature and where it lives', () => {
  const s = sym({ name: 'clamp_i', signature: 'def clamp_i(v: int) -> int', file: UTIL, line: 3 })
  assert.equal(symbolLabel(s), 'def  def clamp_i(v: int) -> int')
  assert.equal(symbolOrigin(s), 'util.ax:3')
})

test('an extern row is labelled extern, a struct row is labelled struct', () => {
  assert.equal(symbolLabel(sym({ kind: 'struct', isExtern: false })), 'struct  def f()')
  assert.equal(symbolLabel(sym({ isExtern: true })), 'extern  def f()')
})

test('sameFile normalizes separators and case, and rejects empties', () => {
  assert.equal(sameFile('C:/p/main.ax', 'C:\\p\\main.ax'), true)
  assert.equal(sameFile('C:/p/Main.ax', 'c:/p/main.ax'), true)
  assert.equal(sameFile('C:/p/main.ax', 'C:/p/util.ax'), false)
  assert.equal(sameFile('', 'C:/p/main.ax'), false)
  assert.equal(sameFile('C:/p/main.ax', ''), false)
})
