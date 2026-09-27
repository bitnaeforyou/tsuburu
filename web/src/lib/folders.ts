/// Shelves, as the two screens that hold works both see them.
///
/// A shelf belongs to the work rather than to the row that stars it, so what
/// is kept and what is on disk are looking at the same shelves. A shelf is
/// made because the reader made one, not because something is on it: taking
/// the last work off leaves it there to put the next thing on.

import * as api from './api'
import type { Folder } from './api'

export type Filed = { id: number; folder?: string | null }

/// Which shelf a list is being shown: every work, one by name, or the ones
/// on no shelf at all.
export type Picked = string | null | 'all'

export function unfiled(works: Filed[]): number {
  return works.filter((work) => !work.folder).length
}

/// Whether a shelf is the named one or sits inside it.
///
/// A shelf says which shelf it is inside with `/` in its name, so looking at
/// `읽을 것` has to show what is on `읽을 것/단편` too - otherwise the outer one
/// reads as empty while holding everything.
export function within(shelf: string | null | undefined, name: string): boolean {
  if (!shelf) return false
  return shelf === name || shelf.startsWith(`${name}/`)
}

export function onShelf<T extends Filed>(works: T[], picked: Picked): T[] {
  if (picked === 'all') return works
  if (picked === null) return works.filter((work) => !work.folder)
  return works.filter((work) => within(work.folder, picked))
}

/// The shelf one is inside, and the last part of its own name.
export function parentOf(name: string): string | null {
  const cut = name.lastIndexOf('/')
  return cut === -1 ? null : name.slice(0, cut)
}

export function leafOf(name: string): string {
  return name.slice(name.lastIndexOf('/') + 1)
}

/// The value the picker sends back, as a shelf name or none at all.
/// `__new__` is the "new shelf" entry, which asks for a name.
export function chosen(value: string, ask: () => string | null): string | null | undefined {
  if (value === '__new__') {
    const name = ask()?.trim()
    return name ? name : undefined
  }
  return value === '' ? null : value
}

/// Moves a work to a shelf, showing it before the server answers.
///
/// The two screens that hold works do the same thing here, down to putting
/// the row back where it was when the server refuses, so they say it once.
export async function moveTo<T extends Filed>(
  works: T[],
  id: number,
  to: string | null,
  show: (works: T[]) => void,
  shelved: (shelves: Folder[]) => void,
  failed: (cause: unknown) => void,
) {
  const was = works.find((each) => each.id === id)?.folder ?? null
  const put = (folder: string | null) =>
    show(works.map((each) => (each.id === id ? { ...each, folder } : each)))

  put(to)
  try {
    await api.setFolder(id, to)
    // Filing on a shelf nobody made makes the shelf; the counts move too.
    shelved(await api.folders())
  } catch (cause) {
    put(was)
    failed(cause)
  }
}
