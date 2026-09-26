/// Searches lately made, so one made before is one keystroke away.
///
/// Kept here rather than asked of the server: what somebody looked for is
/// theirs, and the program has no reason to hold it anywhere but the machine
/// they typed it on.

const STORED = 'tsuburu.recent'
const KEEP = 12

function read(): string[] {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return []
    const rows = JSON.parse(raw) as unknown
    return Array.isArray(rows) ? rows.filter((row): row is string => typeof row === 'string') : []
  } catch {
    return []
  }
}

export function recent(): string[] {
  return read()
}

export function remember(query: string) {
  const asked = query.trim()
  if (!asked) return
  const rows = [asked, ...read().filter((row) => row !== asked)].slice(0, KEEP)
  try {
    localStorage.setItem(STORED, JSON.stringify(rows))
  } catch {
    // A browser with storage switched off still gets to search.
  }
}

export function forget(query?: string) {
  const rows = query === undefined ? [] : read().filter((row) => row !== query)
  try {
    localStorage.setItem(STORED, JSON.stringify(rows))
  } catch {
    // Nothing was written down in the first place.
  }
}
