/**
 * Tests for the compiler-output parser.
 *
 * This is the highest-risk pure logic on the frontend: it reads text
 * written by a DIFFERENT program, it has to survive a Windows drive letter,
 * and a false positive paints the editor red on a successful build. It runs
 * under `node --test` with no test framework — Node strips the types, and a
 * dependency that exists only to assert `assert.equal` is not worth it.
 *
 *     pnpm test
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
  basename,
  diagnosticsByFile,
  diagnosticsFrom,
  fromStructured,
  logLineClass,
  parseDiagnostic,
  parseDiagnostics,
} from '../lib/diagnostics.ts'

const NL = '\n'

test('parses a relative path', () => {
  const d = parseDiagnostic("[type] src/main.ax:12:5: cannot find name 'foo'")
  assert.ok(d)
  assert.equal(d!.severity, 'error')
  assert.equal(d!.file, 'src/main.ax')
  assert.equal(d!.line, 12)
  assert.equal(d!.column, 5)
  assert.equal(d!.message, "cannot find name 'foo'")
})

// The case that rules out splitting on the first colon: `C:` is followed by
// a backslash, never by digits, so a scanner skips it and a splitter does not.
test('parses a Windows absolute path without mistaking the drive for a position', () => {
  const d = parseDiagnostic('[link] C:\\src\\proj\\main.ax:12:9: cannot open file')
  assert.ok(d)
  assert.equal(d!.file, 'C:\\src\\proj\\main.ax')
  assert.equal(d!.line, 12)
  assert.equal(d!.column, 9)
  assert.equal(d!.message, 'cannot open file')
})

test('parses a path with spaces', () => {
  const d = parseDiagnostic('[parse] /home/me/my project/main.ax:1:1: bad indent')
  assert.ok(d)
  assert.equal(d!.file, '/home/me/my project/main.ax')
  assert.equal(d!.line, 1)
  assert.equal(d!.column, 1)
})

test('accepts every compiler stage', () => {
  for (const stage of ['lex', 'parse', 'type', 'internal', 'link', 'io']) {
    const d = parseDiagnostic(`[${stage}] a.ax:1:1: boom`)
    assert.ok(d, `stage ${stage} should be a diagnostic`)
  }
})

test('ignores program output that merely looks bracketed', () => {
  // A stage outside the closed set is a program's own print, not an error.
  assert.equal(parseDiagnostic('[info] hello from the program'), null)
  assert.equal(parseDiagnostic('hello world'), null)
  assert.equal(parseDiagnostic(''), null)
})

test('a successful run produces no diagnostics', () => {
  const log = ['fib(20) = 6765', 'done in 12 ms', ''].join(NL)
  assert.deepEqual(parseDiagnostics(log), [])
})

test('a diagnostic with no position still parses', () => {
  const d = parseDiagnostic('[io] cannot open the toolchain')
  assert.ok(d)
  assert.equal(d!.file, '')
  assert.equal(d!.line, 0)
  assert.equal(d!.message, 'cannot open the toolchain')
})

test('groups diagnostics by file name so imports resolve', () => {
  const log = [
    "[type] src/main.ax:3:1: cannot find name 'helper'",
    "[lex] src/util.ax:7:9: unindent does not match",
    "[type] src/main.ax:9:1: another",
  ].join(NL)
  const byFile = diagnosticsByFile(parseDiagnostics(log))
  assert.equal(byFile.get('main.ax')?.length, 2)
  assert.equal(byFile.get('util.ax')?.length, 1)
})

test('basename handles both separators', () => {
  assert.equal(basename('C:\\a\\b\\c.ax'), 'c.ax')
  assert.equal(basename('/a/b/c.ax'), 'c.ax')
  assert.equal(basename('c.ax'), 'c.ax')
})

test('colours diagnostics as errors and prose as plain text', () => {
  assert.ok(logLineClass("[type] a.ax:1:1: x").includes('logline--err'))
  assert.ok(!logLineClass('just some output').includes('logline--err'))
})

// ---- the structured path (v0.34.0) ----

test('structured diagnostics carry every field through untouched', () => {
  const d = fromStructured([
    { stage: 'type', file: 'D:\\p\\main.ax', line: 12, col: 5, message: "unknown variable 'nope'" },
  ])
  assert.equal(d.length, 1)
  assert.equal(d[0].stage, 'type')
  assert.equal(d[0].file, 'D:\\p\\main.ax')
  assert.equal(d[0].line, 12)
  assert.equal(d[0].column, 5)
  assert.equal(d[0].message, "unknown variable 'nope'")
})

test('an unfamiliar stage is shown, not dropped', () => {
  // The structured form is the compiler TELLING us. A stage this version
  // has never heard of is still an error the reader needs to see.
  const d = fromStructured([{ stage: 'linkage', file: 'a.ax', line: 1, col: 1, message: 'x' }])
  assert.equal(d.length, 1)
  assert.equal(d[0].stage, 'linkage')
})

test('a missing or empty structured list is simply no diagnostics', () => {
  assert.deepEqual(fromStructured(undefined), [])
  assert.deepEqual(fromStructured(null), [])
  assert.deepEqual(fromStructured([]), [])
})

test('the structured path is preferred over scanning the same text', () => {
  // Both paths can see the same error; showing it twice would put two
  // markers on one line for one mistake.
  const text = "[type] D:\\p\\main.ax:3:1: boom\n"
  const d = diagnosticsFrom({
    diags: [{ stage: 'type', file: 'D:\\p\\main.ax', line: 3, col: 1, message: 'boom' }],
    output: text,
  })
  assert.equal(d.length, 1)
})

test('the scanner is the fallback when there is no structured report', () => {
  // A compiler older than v0.34.0 says nothing under --json, and a command
  // that died before printing has to degrade rather than fail.
  const text = "[type] src/main.ax:3:1: cannot find name 'helper'\n"
  const d = diagnosticsFrom({ diags: [], output: text })
  assert.equal(d.length, 1)
  assert.equal(d[0].message, "cannot find name 'helper'")
  assert.equal(d[0].stage, 'type')
})

test('a clean run with no diagnostics at all yields none', () => {
  assert.deepEqual(diagnosticsFrom({ diags: [], output: 'all good\n' }), [])
})