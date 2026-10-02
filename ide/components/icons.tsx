'use client'

/**
 * Inline SVG icons.
 *
 * Hand-written rather than pulled from an icon package: the workbench needs
 * fourteen glyphs, a package would be a hundred kilobytes and a supply
 * chain for the sake of them. All are 16×16 on a 16-unit grid with
 * `currentColor` strokes, so they inherit the theme automatically and stay
 * crisp at the 1× DPI the UI runs at.
 */

type P = { className?: string }

const base = {
  viewBox: '0 0 16 16',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.3,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
}

export const IconFiles = (p: P) => (
  <svg {...base} {...p}>
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.2 1.5h4.8A1.5 1.5 0 0 1 14 6v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12z" />
  </svg>
)

export const IconSearch = (p: P) => (
  <svg {...base} {...p}>
    <circle cx="7" cy="7" r="4.25" />
    <path d="m10.2 10.2 3.3 3.3" />
  </svg>
)

export const IconSource = (p: P) => (
  <svg {...base} {...p}>
    <path d="M6 3.5 2.5 8 6 12.5M10 3.5 13.5 8 10 12.5" />
  </svg>
)

export const IconRun = (p: P) => (
  <svg {...base} {...p}>
    <path d="M4.5 3.2v9.6l8-4.8z" fill="currentColor" stroke="none" />
  </svg>
)

export const IconGear = (p: P) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="2.1" />
    <path d="M8 1.6v1.6M8 12.8v1.6M14.4 8h-1.6M3.2 8H1.6M12.5 3.5l-1.1 1.1M4.6 11.4l-1.1 1.1M12.5 12.5l-1.1-1.1M4.6 4.6 3.5 3.5" />
  </svg>
)

export const IconChevron = (p: P) => (
  <svg {...base} {...p}>
    <path d="m6 3.5 5 4.5-5 4.5" />
  </svg>
)

export const IconFolder = (p: P) => (
  <svg {...base} {...p}>
    <path d="M1.8 4.2A1.2 1.2 0 0 1 3 3h3.1l1.3 1.6h5.6A1.2 1.2 0 0 1 14 5.8v6.4a1.2 1.2 0 0 1-1.2 1.2H3a1.2 1.2 0 0 1-1.2-1.2z" />
  </svg>
)

export const IconFile = (p: P) => (
  <svg {...base} {...p}>
    <path d="M4 2.2h4.6L12 5.6v8.2H4z" />
    <path d="M8.6 2.4v3.2H12" />
  </svg>
)

export const IconClose = (p: P) => (
  <svg {...base} {...p}>
    <path d="m4 4 8 8M12 4l-8 8" />
  </svg>
)

export const IconRefresh = (p: P) => (
  <svg {...base} {...p}>
    <path d="M13.4 8a5.4 5.4 0 1 1-1.6-3.8" />
    <path d="M13.6 2.2v3.2h-3.2" />
  </svg>
)

export const IconSave = (p: P) => (
  <svg {...base} {...p}>
    <path d="M2.8 3.2A1.2 1.2 0 0 1 4 2h7l2.2 2.2v8.6A1.2 1.2 0 0 1 12 14H4a1.2 1.2 0 0 1-1.2-1.2z" />
    <path d="M5.2 2v3.4h5V3.2M5.2 14v-4h5.6v4" />
  </svg>
)

export const IconFolderOpen = (p: P) => (
  <svg {...base} {...p}>
    <path d="M1.8 12.6V4.2A1.2 1.2 0 0 1 3 3h3.1l1.3 1.6h4.4a1.2 1.2 0 0 1 1.2 1.2v1" />
    <path d="M2.4 12.6 4 7.6h10.2l-1.6 5z" />
  </svg>
)

export const IconWarning = (p: P) => (
  <svg {...base} {...p}>
    <path d="M8 2.4 14.4 13H1.6z" />
    <path d="M8 6.4v3.2M8 11.4h.01" />
  </svg>
)

export const IconError = (p: P) => (
  <svg {...base} {...p}>
    <circle cx="8" cy="8" r="5.9" />
    <path d="M8 5v3.6M8 11h.01" />
  </svg>
)

export const IconCheck = (p: P) => (
  <svg {...base} {...p}>
    <path d="m3 8.4 3.2 3.2L13 4.8" />
  </svg>
)

export const IconNewFile = (p: P) => (
  <svg {...base} {...p}>
    <path d="M4 2.2h4.6L12 5.6v8.2H4z" />
    <path d="M8.6 2.4v3.2H12" />
    <path d="M8 8v3.4M6.3 9.7h3.4" />
  </svg>
)