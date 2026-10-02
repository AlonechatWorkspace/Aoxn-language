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
import { treeFromPaths } from './tree'
import { EMPTY_MANIFEST, formatManifest, type PkgManifest } from './pkg'

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
// expand/collapse works and a file opens into the editor. Keys are the
// SAME absolute paths the tree hands out — the editor reads and saves
// through them, so half-paths here would open empty editors.

const FIXTURE_ROOT = '/preview'

const FIXTURE: Record<string, string> = {
  '/preview/hello.ax': `# Aoxn IDE — browser preview
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
  '/preview/notes.md': `# Notes

The IDE shells out to the real \`aoxn\` binary; nothing about the build
is reimplemented here.
`,
  '/preview/src/util.ax': `def clamp_i(v: int, lo: int, hi: int) -> int:
    if v < lo:
        return lo
    if v > hi:
        return hi
    return v
`,
  '/preview/src/main.ax': `import * from "./util.ax"

def main() -> int:
    print(clamp_i(300, 0, 255))
    return 0
`,
  '/preview/aoxn.json': `{
  "name": "preview-fixture",
  "version": "0.1.0",
  "dependencies": {
    "stdlib-demo": "^1.2"
  },
  "devDependencies": {
    "axtest": "*"
  }
}
`,
  '/preview/aoxn.lock': '{ "lockfile_version": 1 }\n',
  '/preview/README.txt': 'Aoxn IDE — browser preview fixture.',
}

/** Directories kept in the fixture even when they hold no file (New folder). */
const FIXTURE_DIRS: string[] = []

/** The fixture tree is always derived, never hand-maintained: a created
 *  file or folder must show up without the fixture learning about it. */
const fixtureTree = (): TreeNode[] => treeFromPaths(FIXTURE, FIXTURE_ROOT, FIXTURE_DIRS)

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
  call('ide_open_folder', { root }, () => fixtureTree())

export const scan = (): Promise<TreeNode[]> =>
  call('ide_scan', undefined, () => fixtureTree())

/** The folder the backend has open (the fixture root in browser mode). */
export const workspaceRoot = (): Promise<string | null> =>
  call('ide_root', undefined, () => FIXTURE_ROOT)

export const readFile = (path: string): Promise<FileContents> =>
  call('ide_read', { path }, () => ({ path, text: FIXTURE[path] ?? '' }))

export const saveFile = (path: string, text: string): Promise<void> =>
  call('ide_save', { path, text }, () => {
    FIXTURE[path] = text
  })

/**
 * Create a file and get the refreshed tree back. The backend refuses to
 * clobber; the fixture mirrors that so the preview behaves like the app.
 */
export const newFile = (path: string): Promise<TreeNode[]> =>
  call('ide_new_file', { path }, () => {
    if (FIXTURE[path] !== undefined) throw new Error(`${path} already exists`)
    FIXTURE[path] = ''
    return fixtureTree()
  })

/** Create a folder and get the refreshed tree back. */
export const newDir = (path: string): Promise<TreeNode[]> =>
  call('ide_new_dir', { path }, () => {
    if (FIXTURE[path] !== undefined || FIXTURE_DIRS.includes(path)) {
      throw new Error(`${path} already exists`)
    }
    FIXTURE_DIRS.push(path)
    return fixtureTree()
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

/** `aoxn doctor` — the toolchain's own self-check, into the output panel. */
export const doctor = (): Promise<ExecResult> => call('ide_doctor', {}, notInBrowser)

/**
 * The workspace's `aoxn.json`, read tolerantly. The fixture ships a small
 * manifest so the packages panel has something real to show in the browser.
 */
export const pkgManifest = (): Promise<PkgManifest> =>
  call('ide_pkg_manifest', undefined, () => {
    const raw = FIXTURE[`${FIXTURE_ROOT}/aoxn.json`]
    if (raw === undefined) return { ...EMPTY_MANIFEST, installed: [] }
    let parsed: Record<string, unknown>
    try {
      parsed = JSON.parse(raw) as Record<string, unknown>
    } catch (e) {
      return { ...formatManifest(null), parseError: String(e) }
    }
    return formatManifest({ ...parsed, hasManifest: true, hasLockfile: true })
  })

/**
 * Run a whitelisted `aoxn pkg` subcommand from the workspace root. The
 * backend refuses everything not on its list (publish, yank, cache, …);
 * the browser preview has no package manager behind it and says so.
 */
export const pkgRun = (subcommand: string, arg?: string): Promise<ExecResult> =>
  call('ide_pkg_run', { subcommand, arg: arg ?? null }, notInBrowser)

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
    return await workspaceRoot()
  } catch {
    return null
  }
}