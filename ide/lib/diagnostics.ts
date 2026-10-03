'use client'

/**
 * The Aoxn compiler's diagnostics.
 *
 * There are TWO sources, and the order matters:
 *
 * 1. **Structured**, from `aoxn --json`. The compiler emits
 *    `{"ok":…,"errors":[{"stage","file","line","col","message"}]}`, the
 *    backend parses it, and it arrives here already exact. Every field is
 *    the compiler's own, so nothing has to be recovered from text.
 * 2. **Scanned from text**, as a fallback. `parseDiagnostic` below reads
 *    the compiler's human one-line form, and it is still needed: a
 *    compiler older than v0.34.0 says nothing under `--json`, and a
 *    command that died before printing anything has to degrade rather than
 *    fail.
 *
 * The scanner is no longer the primary path, but it is not decoration
 * either, and its two hard-won details are still load-bearing:
 *
 * 1. **A Windows path contains a colon.** `C:\src\main.ax:12:5:` — a parser
 *    that splits on the first `:` reads `C` as the filename and falls over.
 *    The position is therefore found by SCANNING for a `:digits:digits:`
 *    run rather than by splitting, and a candidate is only accepted when
 *    both numbers are really numbers.
 *
 * 2. **The stage set is closed.** It is exactly `lex`, `parse`, `type`,
 *    `internal`, `link`, `io` — every one of them an error. Anything else
 *    in the log is a program's own output and must NOT become a marker, or
 *    a successful `hello world` run paints the editor red.
 */

import type { DiagInfo } from './bridge'

export type Severity = 'error' | 'warning'

export interface Diagnostic {
  severity: Severity
  file: string
  line: number // 1-based, as the compiler prints it
  column: number
  message: string
  /** The compiler stage (`type`, `parse`, …); `''` when unknown. */
  stage: string
}

/** The compiler's stages, from `Diag.stage` in the Rust `lib.rs`. */
const ERROR_STAGES = new Set(['lex', 'parse', 'type', 'internal', 'link', 'io'])

const STAGE_RE = /^\[([a-z]+)\]\s*/

/**
 * Turn the compiler's structured report into editor diagnostics.
 *
 * The stage is carried through even when it is not one this version knows:
 * the structured form is the compiler TELLING us, so an unfamiliar stage is
 * a fact to display, not a reason to drop the error on the floor.
 */
export function fromStructured(diags: DiagInfo[] | undefined | null): Diagnostic[] {
  if (!diags || diags.length === 0) return []
  return diags.map((d) => ({
    severity: 'error' as const,
    file: d.file ?? '',
    line: d.line ?? 0,
    column: d.col ?? 0,
    message: d.message ?? '',
    stage: d.stage ?? '',
  }))
}

/**
 * The diagnostics of one command run: the structured ones when there are
 * any, the scanned ones otherwise.
 *
 * The fallback is per-RESULT, not per-diagnostic. A structured list that
 * came back empty means the compiler did not speak JSON at all, and mixing
 * a structured set with a scanned set would risk showing the same error
 * twice when both paths could see the same line.
 */
export function diagnosticsFrom(result: {
  diags?: DiagInfo[] | null
  output: string
}): Diagnostic[] {
  const structured = fromStructured(result.diags)
  return structured.length > 0 ? structured : parseDiagnostics(result.output)
}

export function parseDiagnostic(line: string): Diagnostic | null {
  const m = STAGE_RE.exec(line)
  if (!m) return null
  if (!ERROR_STAGES.has(m[1])) return null

  const rest = line.slice(m[0].length)
  const pos = findPosition(rest)
  if (!pos) {
    // A diagnostic with no position (a global failure) still belongs in the
    // log, but it has nowhere to put a marker.
    return { severity: 'error', file: '', line: 0, column: 0, message: rest, stage: m[1] }
  }

  return {
    severity: 'error',
    file: rest.slice(0, pos.at),
    line: Number.parseInt(rest.slice(pos.at + 1, pos.lineEnd), 10),
    column: Number.parseInt(rest.slice(pos.colStart, pos.colEnd), 10),
    message: rest.slice(pos.msgStart),
    stage: m[1],
  }
}

/** Every diagnostic in a compiler log, in order. */
export function parseDiagnostics(log: string): Diagnostic[] {
  const out: Diagnostic[] = []
  for (const line of log.split('\n')) {
    const d = parseDiagnostic(line)
    if (d) out.push(d)
  }
  return out
}

/**
 * Index of the line number within a compiler line, for jumping from a log
 * entry to a source position. Returns 0 when there is none.
 */
export function lineOfMessage(line: string): number {
  const d = parseDiagnostic(line)
  return d && d.line > 0 ? d.line : 0
}

/**
 * Locate `:line:column: ` inside a diagnostic body.
 *
 * Scans for a colon that is followed by digits, then another colon, then
 * more digits, then `: `. A drive letter fails the first test — the
 * character after `C:` is a backslash — which is the whole reason this is
 * a scanner and not a split.
 */
function findPosition(s: string): {
  at: number
  lineEnd: number
  colStart: number
  colEnd: number
  msgStart: number
} | null {
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== ':') continue
    const lineEnd = scanDigits(s, i + 1)
    if (lineEnd === i + 1) continue
    if (s[lineEnd] !== ':') continue
    const colStart = lineEnd + 1
    const colEnd = scanDigits(s, colStart)
    if (colEnd === colStart) continue
    if (s[colEnd] !== ':' || s[colEnd + 1] !== ' ') continue
    return { at: i, lineEnd, colStart, colEnd, msgStart: colEnd + 2 }
  }
  return null
}

function scanDigits(s: string, from: number): number {
  let i = from
  while (i < s.length && s.charCodeAt(i) >= 48 && s.charCodeAt(i) <= 57) i++
  return i
}

/**
 * Map diagnostics onto the files that are currently open.
 *
 * The compiler echoes back whatever path it was given, which is the
 * absolute native path, while a model is keyed by the path the tree sent.
 * Comparing the full strings would therefore miss whenever the two spell
 * a separator differently, so the file name is the fallback key — which is
 * also what makes an error inside an *import* find its tab.
 */
export function diagnosticsByFile(
  diagnostics: Diagnostic[],
): Map<string, Diagnostic[]> {
  const byName = new Map<string, Diagnostic[]>()
  for (const d of diagnostics) {
    if (!d.file) continue
    const key = basename(d.file)
    const list = byName.get(key)
    if (list) list.push(d)
    else byName.set(key, [d])
  }
  return byName
}

export function basename(p: string): string {
  const i = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'))
  return i < 0 ? p : p.slice(i + 1)
}

/** How a log line should be coloured. */
export function logLineClass(line: string): string {
  if (parseDiagnostic(line)) return 'logline logline--err'
  if (/^\[.*\]\s*$/.test(line)) return 'logline logline--meta'
  if (/\bwarning\b/i.test(line)) return 'logline logline--warn'
  return 'logline'
}