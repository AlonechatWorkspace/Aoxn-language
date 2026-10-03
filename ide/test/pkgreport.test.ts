/**
 * Tests for the packages panel's report normalization.
 *
 * The panel's job is to show what the resolver DELIVERED next to what the
 * manifest ASKED for, and the join between the two is where a wrong answer
 * hides: a name matched case-sensitively misses on Windows, an upgrade
 * silently attached to the wrong package is worse than none at all, and a
 * report that failed to load must not read as "everything is current".
 *
 *     pnpm test
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
  EMPTY_REPORT,
  formatInstalled,
  formatReport,
  formatUpgrade,
  withUpgrades,
} from '../lib/pkg.ts'
import type { PkgReport } from '../lib/bridge.ts'

const FULL: PkgReport = {
  installed: [
    { name: 'http', version: '2.1.0', scope: 'prod', source: 'registry.local' },
    { name: 'axtest', version: '0.3.0', scope: 'dev', source: 'local' },
  ],
  outdated: [{ name: 'http', locked: '2.1.0', newest: '2.4.0', note: '' }],
  error: null,
}

test('a well-formed report survives normalization', () => {
  const r = formatReport(FULL)
  assert.equal(r.installed.length, 2)
  assert.equal(r.installed[0].version, '2.1.0')
  assert.equal(r.outdated[0].newest, '2.4.0')
  assert.equal(r.error, null)
})

test('a missing or malformed report becomes an empty one, not a crash', () => {
  // The panel must open in a folder with no aoxn.json and show a hint.
  for (const junk of [null, undefined, 42, 'text', [], { installed: 'no' }]) {
    const r = formatReport(junk)
    assert.deepEqual(r.installed, [])
    assert.deepEqual(r.outdated, [])
  }
  assert.deepEqual(formatReport(undefined), EMPTY_REPORT)
})

test('a nameless row is dropped rather than shown blank', () => {
  const r = formatReport({ installed: [{ version: '1.0.0' }, { name: 'ok' }] })
  assert.equal(r.installed.length, 1)
  assert.equal(r.installed[0].name, 'ok')
})

test('a report error is carried through as text', () => {
  const r = formatReport({ error: 'outdated: registry unreachable' })
  assert.equal(r.error, 'outdated: registry unreachable')
  assert.equal(formatReport({ error: 42 }).error, null)
})

test('an installed row carries its upgrade when one exists', () => {
  const rows = withUpgrades(FULL)
  assert.equal(rows[0].upgrade, '2.4.0')
  assert.equal(rows[1].upgrade, null, 'axtest is not in the outdated list')
})

test('the upgrade lookup is case-insensitive', () => {
  // Registry names and manifest keys are not required to agree on case, and
  // a mismatch would silently drop the upgrade badge.
  const rows = withUpgrades({
    installed: [{ name: 'HTTP', version: '2.1.0', scope: 'prod', source: 'r' }],
    outdated: [{ name: 'http', locked: '2.1.0', newest: '2.4.0', note: '' }],
    error: null,
  })
  assert.equal(rows[0].upgrade, '2.4.0')
})

test('an empty report joins to nothing without throwing', () => {
  assert.deepEqual(withUpgrades(EMPTY_REPORT), [])
})

test('an installed row reads with its version, scope and source', () => {
  assert.equal(formatInstalled(FULL.installed[0], '2.4.0'), 'http 2.1.0 (registry.local)')
  assert.equal(formatInstalled(FULL.installed[1], null), 'axtest 0.3.0 (dev · local)')
  // A plain prod dependency from the default registry states nothing extra.
  assert.equal(
    formatInstalled({ name: 'json', version: '1.0.0', scope: 'prod', source: 'registry' }, null),
    'json 1.0.0',
  )
})

test('an upgrade reads as locked to newest, and only when they differ', () => {
  assert.equal(formatUpgrade('2.1.0', '2.4.0'), '2.1.0 → 2.4.0')
  assert.equal(formatUpgrade('2.1.0', '2.1.0'), '', 'already current')
  assert.equal(formatUpgrade('', '2.4.0'), '', 'nothing locked to compare')
  assert.equal(formatUpgrade('2.1.0', ''), '', 'nothing known to compare against')
})
