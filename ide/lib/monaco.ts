'use client'

/**
 * Monaco wiring for the Aoxn IDE.
 *
 * Two things live here and nowhere else:
 *
 * 1. **The Aoxn language definition.** Monaco ships grammars for most
 *    languages but has never heard of Aoxn, so it gets a Monarch grammar
 *    plus an Aoxn-aware configuration (comments, brackets, auto-closing
 *    pairs, folding markers).
 *
 * 2. **The colour theme**, defined from the same CSS custom properties the
 *    rest of the workbench uses. That is why the IDE can switch to a light
 *    theme in one place: Monaco reads the variables back out of the DOM
 *    instead of duplicating sixteen colours.
 *
 * The language services (IntelliSense, the TypeScript checker) are NOT
 * wired up. They need a language server and type definitions for a language
 * that has neither, and shipping a half-configured service is worse than
 * shipping none: it would report a wall of red squiggles for a perfectly
 * valid Aoxn program.
 */

import type * as Monaco from 'monaco-editor'

export const LANG_AOXN = 'aoxn'

const AOXN_KEYWORDS = [
  'def', 'elif', 'else', 'if', 'while', 'for', 'in', 'not', 'and', 'or',
  'return', 'break', 'continue', 'pass', 'import', 'from', 'struct',
  'extern', 'True', 'False', 'None',
]

const AOXN_TYPES = ['int', 'float', 'bool', 'string']

const AOXN_BUILTINS = ['len', 'str', 'print', 'range', 'to_int', 'to_float']

/** Read a CSS custom property off <html>, falling back to a sane value. */
function cssVar(name: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback
  let v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  // The CSS pipeline (Turbopack/lightningcss) minifies `#cccccc` down to
  // `#ccc`, and Monaco's theme parser THROWS on the short form ("Illegal
  // value for token color") — so every short hex is expanded back before
  // it reaches defineTheme.
  const short3 = /^#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])$/.exec(v)
  if (short3) v = `#${short3[1]}${short3[1]}${short3[2]}${short3[2]}${short3[3]}${short3[3]}`
  const short4 = /^#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])$/.exec(v)
  if (short4) {
    v = `#${short4[1]}${short4[1]}${short4[2]}${short4[2]}${short4[3]}${short4[3]}${short4[4]}${short4[4]}`
  }
  return v || fallback
}

/**
 * Register the Aoxn language and the workbench theme with Monaco.
 *
 * Safe to call more than once: `monaco.languages.register` throws on a
 * duplicate, and React may run the effect that calls this twice.
 */
export function registerAoxn(monaco: typeof Monaco): void {
  if (!monaco.languages.getLanguages().some((l) => l.id === LANG_AOXN)) {
    registerLanguage(monaco)
  }
  registerTheme(monaco)
}

function registerLanguage(monaco: typeof Monaco): void {
  monaco.languages.register({
    id: LANG_AOXN,
    extensions: ['.ax'],
    aliases: ['Aoxn', 'aoxn'],
    mimetypes: ['text/x-aoxn'],
  })

  monaco.languages.setLanguageConfiguration(LANG_AOXN, {
    comments: { lineComment: '#' },
    brackets: [
      ['{', '}'],
      ['[', ']'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"', notIn: ['string', 'comment'] },
      { open: "'", close: "'", notIn: ['string', 'comment'] },
    ],
    surroundingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    // Aoxn is an indentation language, so Enter inside an unclosed brace or
    // bracket indents the next line. There are no block keywords that end
    // one (`end` does not exist in the language), which is why the pattern
    // is bracket-based rather than keyword-based.
    indentationRules: {
      increaseIndentPattern: /^.*\{[^}"']*$/,
      decreaseIndentPattern: /^\s*(\}|\]|\))/,
    },
    folding: { markers: { start: /^\s*def\s/, end: /^\s*struct\s/ } },
  })

  monaco.languages.setMonarchTokensProvider(LANG_AOXN, {
    defaultToken: '',
    keywords: AOXN_KEYWORDS,
    types: AOXN_TYPES,
    builtins: AOXN_BUILTINS,
    tokenizer: {
      root: [
        [/#.*$/, 'comment'],

        // `f"..."` is an interpolation string; the prefix is part of the
        // literal, so it enters the string state immediately.
        [/f"/, { token: 'string', next: '@dqString' }],
        [/"/, { token: 'string', next: '@dqString' }],
        [/'/, { token: 'string', next: '@sqString' }],

        [
          /[a-zA-Z_]\w*/,
          {
            cases: {
              '@keywords': 'keyword',
              '@types': 'type',
              '@builtins': 'type',
              '@default': 'identifier',
            },
          },
        ],

        [/\d*\.\d+([eE][-+]?\d+)?/, 'number'],
        [/\d+/, 'number'],

        // No bitwise forms here on purpose: the language has no bitwise
        // operators, so a `|` in Aoxn is only ever a malformed `or`.
        [/[+\-*/%<>=!]=?/, 'operator'],
        [/[&]{2}|[|]{2}/, 'operator'],
        [/\/\//, 'operator'],
        [/[()[\]{}]/, 'delimiter.bracket'],
        [/[.,:;]/, 'delimiter'],
      ],

      dqString: [
        [/[^\\"]+/, 'string'],
        [/\\./, 'string.escape'],
        [/"/, { token: 'string', next: '@pop' }],
      ],

      sqString: [
        [/[^\\']+/, 'string'],
        [/\\./, 'string.escape'],
        [/'/, { token: 'string', next: '@pop' }],
      ],
    },
  })
}

/**
 * Build the Monaco theme from the workbench's CSS variables.
 *
 * Because the theme is derived rather than hard-coded, a light theme is a
 * change to `:root[data-theme='light']` in globals.css and nothing else —
 * Monaco follows.
 */
export function registerTheme(monaco: typeof Monaco, themeName = 'aoxn-dark'): void {
  const op = cssVar('--syn-operator', '#d4d4d4')
  monaco.editor.defineTheme(themeName, {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'comment', foreground: cssVar('--syn-comment', '#6a9955'), fontStyle: 'italic' },
      { token: 'keyword', foreground: cssVar('--syn-keyword', '#c586c0') },
      { token: 'type', foreground: cssVar('--syn-type', '#4ec9b0') },
      { token: 'string', foreground: cssVar('--syn-string', '#ce9178') },
      { token: 'string.escape', foreground: cssVar('--syn-number', '#b5cea8') },
      { token: 'number', foreground: cssVar('--syn-number', '#b5cea8') },
      { token: 'identifier', foreground: cssVar('--syn-variable', '#9cdcfe') },
      { token: 'operator', foreground: op },
      { token: 'delimiter', foreground: op },
      { token: 'delimiter.bracket', foreground: op },
    ],
    colors: {
      'editor.background': cssVar('--bg-editor', '#1e1e1e'),
      'editor.foreground': cssVar('--fg', '#cccccc'),
      'editorLineNumber.foreground': cssVar('--fg-faint', '#6e6e6e'),
      'editorLineNumber.activeForeground': cssVar('--fg', '#cccccc'),
      'editor.lineHighlightBackground': cssVar('--bg-hover', '#2a2d2e'),
      'editor.selectionBackground': cssVar('--accent-soft', '#264f78'),
      'editorCursor.foreground': cssVar('--fg', '#cccccc'),
      'editorIndentGuide.background1': cssVar('--border', '#2b2b2b'),
      'editorGutter.background': cssVar('--bg-editor', '#1e1e1e'),
      'editorWidget.background': cssVar('--bg-panel', '#202020'),
      'editorWidget.border': cssVar('--border-strong', '#3c3c3c'),
      'editorSuggestWidget.background': cssVar('--bg-panel', '#202020'),
      'editorSuggestWidget.selectedBackground': cssVar('--bg-active', '#37373d'),
      'editorHoverWidget.background': cssVar('--bg-panel', '#202020'),
      'editorHoverWidget.border': cssVar('--border-strong', '#3c3c3c'),
      'scrollbarSlider.background': 'rgba(121,121,121,0.35)',
      'scrollbarSlider.hoverBackground': 'rgba(100,100,100,0.6)',
      'editorOverviewRuler.border': cssVar('--bg-editor', '#1e1e1e'),
    },
  })
  monaco.editor.setTheme(themeName)
}