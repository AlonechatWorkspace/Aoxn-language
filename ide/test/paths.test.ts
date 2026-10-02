/**
 * Tests for the new-file / new-folder prompt validation.
 *
 * The rule the tests protect: everything that would escape the workspace,
 * create an invisible or unportable name, or confuse the backend is refused
 * HERE, before a byte reaches the filesystem. `..` is the load-bearing case
 * — it is the workspace-escape vector — and the separator rules exist
 * because the backend canonicalises but the frontend compares spellings.
 *
 *     pnpm test
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { joinEntry, validateEntryName } from '../lib/paths.ts'

test('accepts ordinary names, including nested ones', () => {
  for (const ok of ['main.ax', 'src/util.ax', 'a/b/c/deep.ax', 'my notes.txt', '2026.ax']) {
    assert.equal(validateEntryName(ok), null, `${ok} should be accepted`)
  }
})

test('refuses the workspace-escape spellings', () => {
  assert.ok(validateEntryName('..'))
  assert.ok(validateEntryName('../secret.ax'))
  assert.ok(validateEntryName('src/../../secret.ax'))
  assert.ok(validateEntryName('.'))
})

test('refuses absolute and backslash spellings', () => {
  assert.ok(validateEntryName('/etc/passwd'))
  assert.ok(validateEntryName('C:evil.ax'))
  assert.ok(validateEntryName('src\\util.ax'))
})

test('refuses empty and structural garbage', () => {
  assert.ok(validateEntryName(''))
  assert.ok(validateEntryName('   '))
  assert.ok(validateEntryName('src//util.ax'))
  assert.ok(validateEntryName('src/'))
  assert.ok(validateEntryName('a'.repeat(201)))
  assert.ok(validateEntryName('bad\x01name'))
})

test('trims stray spaces outside, refuses them inside segments', () => {
  // the whole name is trimmed — a stray space on either end is a typo,
  // not a name
  assert.equal(validateEntryName(' name.ax'), null)
  assert.equal(validateEntryName('name.ax '), null)
  // a space at a segment edge is INVISIBLE in the tree and easy to lose
  assert.ok(validateEntryName('src /util.ax'))
  assert.ok(validateEntryName('src/ util.ax'))
  assert.equal(validateEntryName('my name.ax'), null)
})

test('joinEntry trims and does not double the separator', () => {
  assert.equal(joinEntry('D:/proj/src', ' util.ax '), 'D:/proj/src/util.ax')
  assert.equal(joinEntry('D:/proj/src/', 'util.ax'), 'D:/proj/src/util.ax')
})
