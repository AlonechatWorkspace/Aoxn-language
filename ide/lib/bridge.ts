'use client'

/**
 * The bridge to the Rust backend.
 *
 * Every call goes through `call()`, which asks Tauri for the result and
 * falls back to a browser-mode stub when there is no Tauri runtime. That
 * fallback is not decoration: it is what makes `pnpm dev` a usable way to
 * work on the workbench's layout, theme and interaction design without
 * paying for a native rebuild on every change, and it is what the CI
 * screenshot of the UI is taken against.
 *
 * In browser mode the stub answers from an in-memory fixture folder, so the
 * explorer, tabs, editor and output panel are all exercised; only the
 * compiler is inert, and it says so rather than pretending.
 */

import { invoke } from '@tauri-apps/api/core'

/** Mirror of `model::TreeNode` in src-tauri/src/model.rs. */
export interface TreeNode {
  name: string
  path: string
  isDir: boolean
  depth: number
}

/** Mirror of `model::FileContents`. */
export interface FileContents {
  path: string
  text: string
}

/** Mirror of `model::ExecResult`. */
export interface ExecResult {
  code: number
  output: string
  durationMs: number
}

/** Mirror of `model::ToolchainInfo`. */
export interface ToolchainInfo {
  compiler: string
  compilerFound: boolean
  clangFound: boolean
  version: string
}

/** True when running inside the Tauri webview. */
export const isNative = (): boolean =>
  typeof window !== 'undefined' &&
  ('__TAURI_INTERNALS__' in window || '__TAURI__' in window)

// ---- browser-mode fixture ----
// A tiny synthetic project: enough shape to prove the explorer renders,
// expand/collapse works and a file opens into the editor.

const FIXTURE: Record<string, string> = {
  'hello.ax': `# Aoxn IDE — browser preview
# This tree is a fixture; run the packaged app for a real folder.

import * from "stdlib"

def main() -> int:
    n = fib(20)
    print("fib(20) = " + str(n))
    return 0

def fib(n: int) -> int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
`,
  'notes.md': `# Notes

The IDE shells out to the real \`aoxn\` binary; nothing about the build
is reimplemented here.
`,
  'src/util.ax': `def clamp_i(v: int, lo: int, hi: int) -> int:
    if v < lo:
        return lo
    if v > hi:
        return hi
    return v
`,
  'src/main.ax': `import * from "./util.ax"

def main() -> int:
    print(clamp_i(300, 0, 255))
    return 0
`,
  'README.txt': 'Aoxn IDE — browser preview fixture.',
}

const FIXTURE_TREE: TreeNode[] = [
  { name: 'hello.ax', path: '/preview/hello.ax', isDir: false, depth: 0 },
  { name: 'notes.md', path: '/preview/notes.md', isDir: false, depth: 0 },
  { name: 'README.txt', path: '/preview/README.txt', isDir: false, depth: 0 },
  { name: 'src', path: '/preview/src', isDir: true, depth: 0 },
  { name: 'main.ax', path: '/preview/src/main.ax', isDir: false, depth: 1 },
  { name: 'util.ax', path: '/preview/src/util.ax', isDir: false, depth: 1 },
]

async function call<T>(cmd: string, args?: Record<string, unknown>, fallback?: () => T | Promise<T>): Promise<T> {
  if (isNative()) {
    return invoke<T>(cmd, args)
  }
  if (!fallback) {
    throw new Error(`${cmd} is only available in the desktop app`)
  }
  return fallback()
}

// ---- commands ----

export const openFolder = (root: string): Promise<TreeNode[]> =>
  call('ide_open_folder', { root }, () => FIXTURE_TREE)

export const scan = (): Promise<TreeNode[]> =>
  call('ide_scan', undefined, () => FIXTURE_TREE)

export const readFile = (path: string): Promise<FileContents> =>
  call('ide_read', { path }, () => ({ path, text: FIXTURE[path] ?? '' }))

export const saveFile = (path: string, text: string): Promise<void> =>
  call('ide_save', { path, text }, () => {
    FIXTURE[path] = text
  })

export const toolchain = (): Promise<ToolchainInfo> =>
  call(
    'ide_toolchain',
    undefined,
    async () => ({
      compiler: 'aoxn',
      compilerFound: false,
      clangFound: false,
      version: 'browser preview — no compiler available',
    }),
  )

export const check = (path: string): Promise<ExecResult> =>
  call('ide_check', { path }, notInBrowser)

export const build = (path: string): Promise<ExecResult> =>
  call('ide_build', { path }, notInBrowser)

export const runProgram = (path: string): Promise<ExecResult> =>
  call('ide_run', { path }, notInBrowser)

export const buildAndRun = (path: string): Promise<ExecResult> =>
  call('ide_build_and_run', { path }, notInBrowser)

function notInBrowser(): ExecResult {
  return {
    code: -1,
    output:
      'This is the browser preview — there is no Aoxn toolchain behind it.\n' +
      'Run `pnpm ide:dev` (or the packaged app) to build and run for real.\n',
    durationMs: 0,
  }
}

/** Open the native folder picker. Null when cancelled or in the browser. */
export async function pickFolder(): Promise<string | null> {
  if (!isNative()) return null
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, multiple: false, title: 'Open folder' })
  return typeof picked === 'string' ? picked : null
}

/** The folder the backend opened on launch, if any. */
export async function currentRoot(): Promise<string | null> {
  try {
    const tree = await scan()
    if (tree.length === 0) return null
    // every node's path shares the root; take the shortest one
    return tree[0].path.replace(/[\\/][^\\/]*$/, '')
  } catch {
    return null
  }
}