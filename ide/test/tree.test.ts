/**
 * Tests for the fixture-tree derivation (`lib/tree.ts`).
 *
 * The browser preview has no Rust walker, so the tree is derived from the
 * path map on every change. These tests pin the rules that keep the preview
 * looking like the native app: directories first, case-insensitive order,
 * depth-first rows, dot entries invisible, and folders created with no file
 * inside still visible.
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { treeFromPaths } from '../lib/tree.ts'

test('derives a sorted, depth-first tree with directories first', () => {
  const tree = treeFromPaths(
    {
      '/p/zz.ax': 'z',
      '/p/src/main.ax': 'm',
      '/p/aa.ax': 'a',
      '/p/src/util.ax': 'u',
    },
    '/p',
  )
  assert.deepEqual(
    tree.map((n) => `${n.name}@${n.depth}`),
    ['src@0', 'main.ax@1', 'util.ax@1', 'aa.ax@0', 'zz.ax@0'],
  )
  assert.equal(tree[0].isDir, true)
})

test('dot entries never appear', () => {
  const tree = treeFromPaths({ '/p/.hidden': 'x', '/p/visible.ax': 'v' }, '/p')
  assert.deepEqual(tree.map((n) => n.name), ['visible.ax'])
})

test('a folder created empty still shows up', () => {
  const tree = treeFromPaths({ '/p/main.ax': 'm' }, '/p', ['/p/gen'])
  assert.deepEqual(tree.map((n) => n.name), ['gen', 'main.ax'])
  assert.equal(tree[0].isDir, true)
})

test('creating a file nests folders automatically', () => {
  const tree = treeFromPaths({ '/p/main.ax': 'm', '/p/a/b/c.ax': 'c' }, '/p')
  assert.deepEqual(tree.map((n) => `${n.name}@${n.depth}`), ['a@0', 'b@1', 'c.ax@2', 'main.ax@0'])
})

test('sorting is case-insensitive', () => {
  const tree = treeFromPaths(
    { '/p/Beta.ax': 'b', '/p/alpha.ax': 'a', '/p/Alpha2.ax': 'a2' },
    '/p',
  )
  assert.deepEqual(tree.map((n) => n.name), ['alpha.ax', 'Alpha2.ax', 'Beta.ax'])
})
