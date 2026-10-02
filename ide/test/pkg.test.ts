/**
 * Tests for the packages panel's pure logic (`lib/pkg.ts`).
 *
 * The manifest normalizer must survive a backend from before the feature
 * (undefined, wrong types) without throwing — the panel renders whatever it
 * gets — and the name validator must refuse exactly what the Rust side
 * refuses, because the two validate the same prompt.
 *
 *     pnpm test
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { EMPTY_MANIFEST, formatDep, formatManifest, validatePkgName } from '../lib/pkg.ts'

test('formatManifest normalizes a full backend payload', () => {
  const m = formatManifest({
    hasManifest: true,
    name: 'demo',
    version: '0.1.0',
    hasLockfile: true,
    parseError: null,
    dependencies: [
      { name: 'http', req: '^2', dev: false },
      { name: 'axtest', req: '*', dev: true },
    ],
    installed: ['http'],
  })
  assert.equal(m.hasManifest, true)
  assert.equal(m.name, 'demo')
  assert.equal(m.dependencies.length, 2)
  assert.equal(m.dependencies[1].dev, true)
  assert.deepEqual(m.installed, ['http'])
})

test('formatManifest survives junk without throwing', () => {
  for (const junk of [undefined, null, 42, 'nope', {}, { dependencies: 'nope' }, { dependencies: [42, null, { name: 'ok', req: '^1', dev: false }] }]) {
    const m = formatManifest(junk)
    assert.equal(typeof m.hasManifest, 'boolean')
    assert.ok(Array.isArray(m.dependencies))
    assert.ok(Array.isArray(m.installed))
  }
  const m = formatManifest({ dependencies: [42, null, { name: 'ok', req: '^1', dev: false }] })
  assert.deepEqual(m.dependencies, [{ name: 'ok', req: '^1', dev: false }])
})

test('a missing manifest is the empty shape', () => {
  assert.deepEqual(formatManifest(undefined), EMPTY_MANIFEST)
})

test('formatDep marks dev dependencies', () => {
  assert.equal(formatDep({ name: 'http', req: '^2', dev: false }), 'http ^2')
  assert.equal(formatDep({ name: 'axtest', req: '*', dev: true }), 'axtest * (dev)')
})

test('package-name validation mirrors the Rust whitelist', () => {
  for (const ok of ['http', 'json', 'http@^2', 'my-pkg_2', 'a@~1.0', 'x*y']) {
    assert.equal(validatePkgName(ok), null, `${ok} should be accepted`)
  }
  for (const bad of ['', '   ', '-x', 'a b', '../evil', 'a/b', 'a\\b', 'a;b', '$PATH', 'x'.repeat(101)]) {
    assert.ok(validatePkgName(bad), `${bad} should be refused`)
  }
})
