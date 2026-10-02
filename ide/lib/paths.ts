/**
 * Validation for names typed into the "new file" / "new folder" prompt.
 *
 * The prompt takes a name RELATIVE to the folder it was opened from, and it
 * may create nested entries in one step (`src/util.ax`). Everything the
 * backend would reject with an OS error — or worse, accept — is refused
 * HERE with a human message. The workspace boundary itself is still
 * enforced by the Rust side (`Workspace::resolve`); this is the layer that
 * makes the dialog feel like an editor instead of a stack trace.
 */

export function validateEntryName(raw: string): string | null {
  const name = raw.trim()
  if (!name) return 'Type a name.'
  if (name.length > 200) return 'That name is too long.'
  if (name.includes('\\')) return "Use '/' as the separator (the other one is not portable)."
  if (name.startsWith('/')) return 'The name is relative to the selected folder — do not start it with "/".'
  if (name.endsWith('/')) return 'The name must end with a file or folder name.'
  if (name.includes(':')) return "':' is not allowed in a name."
  // Control characters are invisible and break the backend's error messages
  // more than they break the filesystem.
  if (/[\x00-\x1f]/.test(name)) return 'Control characters are not allowed.'
  for (const part of name.split('/')) {
    if (!part) return "'//' is not a name."
    if (part === '.' || part === '..') {
      return "'.' and '..' are not allowed — the workspace boundary is not negotiable."
    }
    if (part.trim() !== part) {
      return `Segment "${part}" starts or ends with a space, which is invisible and easy to lose.`
    }
  }
  return null
}

/** Join a directory and a validated name with the portable separator. */
export function joinEntry(dir: string, name: string): string {
  return `${dir.replace(/[\\/]+$/, '')}/${name.trim()}`
}
