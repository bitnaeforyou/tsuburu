/// Shelves, as the two screens that hold works both see them.
///
/// A shelf belongs to the work rather than to the row that stars it, so what
/// is kept and what is on disk are looking at the same shelves. There is no
/// folder apart from the works that name one: none is created, and none is
/// left behind empty when the last work moves off it.

export type Filed = { id: number; folder?: string | null }

/// Which shelf a list is being shown: every work, one by name, or the ones
/// on no shelf at all.
export type Picked = string | null | 'all'

export function shelvesOf(works: Filed[]): [string, number][] {
  const counts = new Map<string, number>()
  for (const work of works) {
    if (work.folder) counts.set(work.folder, (counts.get(work.folder) ?? 0) + 1)
  }
  return [...counts].sort(([a], [b]) => a.localeCompare(b))
}

export function unfiled(works: Filed[]): number {
  return works.filter((work) => !work.folder).length
}

export function onShelf<T extends Filed>(works: T[], picked: Picked): T[] {
  if (picked === 'all') return works
  if (picked === null) return works.filter((work) => !work.folder)
  return works.filter((work) => work.folder === picked)
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
