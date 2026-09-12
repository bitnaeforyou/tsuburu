/// Moving what this machine remembers to another one.
///
/// Favorites, followed artists and how far each work was read are the only
/// things tsuburu keeps about you, and they are small. This puts them in one
/// file and puts them back through the same calls the interface uses, so a
/// restore cannot leave the databases in a shape the program cannot read.

import type { Favorite, HistoryEntry, Summary } from './api'

/// Bumped only if a later file could not be read by an earlier tsuburu.
export const VERSION = 1

export type Backup = {
  tsuburu: number
  exported: string
  favorites: Favorite[]
  artists: string[]
  history: HistoryEntry[]
}

/// How many restores are in flight at once. The server answers these from
/// disk, so the limit is politeness rather than protection.
const AT_ONCE = 6

export function pack(
  favorites: Favorite[],
  artists: string[],
  history: HistoryEntry[],
  now = new Date(),
): Backup {
  return {
    tsuburu: VERSION,
    exported: now.toISOString(),
    favorites,
    artists,
    history,
  }
}

export function nameFor(now = new Date()): string {
  return `tsuburu-${now.toISOString().slice(0, 10)}.json`
}

export class NotABackup extends Error {}

/// Reads a file back, keeping only what has the shape of a record and can be
/// restored. A file half full of junk restores the half that is not.
export function unpack(text: string): Backup {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    throw new NotABackup('not a tsuburu backup')
  }
  if (!raw || typeof raw !== 'object') throw new NotABackup('not a tsuburu backup')
  const value = raw as Partial<Backup>
  if (typeof value.tsuburu !== 'number') throw new NotABackup('not a tsuburu backup')
  if (value.tsuburu > VERSION) throw new NotABackup('made by a newer tsuburu')

  return {
    tsuburu: value.tsuburu,
    exported: typeof value.exported === 'string' ? value.exported : '',
    favorites: works<Favorite>(value.favorites).map((item) => ({
      ...item,
      added_at: Number.isFinite(item.added_at) ? item.added_at : 0,
    })),
    artists: Array.isArray(value.artists)
      ? value.artists.filter((name): name is string => typeof name === 'string' && name !== '')
      : [],
    history: works<HistoryEntry>(value.history).map((item) => ({
      ...item,
      last_seen_at: Number.isFinite(item.last_seen_at) ? item.last_seen_at : 0,
      last_page: Number.isFinite(item.last_page) ? Math.max(0, Math.trunc(item.last_page)) : 0,
    })),
  }
}

function works<T extends Summary>(value: unknown): T[] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (item): item is T =>
      !!item && typeof item === 'object' && Number.isInteger((item as Summary).id),
  )
}

export type Restorer = {
  favorite: (id: number, summary: Omit<Summary, 'id'>) => Promise<unknown>
  follow: (name: string) => Promise<unknown>
  progress: (id: number, page: number, summary: Omit<Summary, 'id'>) => Promise<unknown>
}

export type Restored = { done: number; failed: number }

/// Puts a backup back, a few at a time, reporting as it goes. One record the
/// server refuses does not stop the rest.
export async function restore(
  backup: Backup,
  into: Restorer,
  onstep?: (done: number, total: number) => void,
): Promise<Restored> {
  const jobs: (() => Promise<unknown>)[] = [
    ...backup.favorites.map((item) => () => into.favorite(item.id, summaryOf(item))),
    ...backup.artists.map((name) => () => into.follow(name)),
    ...backup.history.map((item) => () => into.progress(item.id, item.last_page, summaryOf(item))),
  ]

  const total = jobs.length
  let done = 0
  let failed = 0
  let next = 0

  async function worker() {
    while (next < jobs.length) {
      const job = jobs[next++]
      try {
        await job()
      } catch {
        failed++
      }
      done++
      onstep?.(done, total)
    }
  }

  await Promise.all(Array.from({ length: Math.min(AT_ONCE, jobs.length) }, worker))
  return { done, failed }
}

function summaryOf(item: Summary): Omit<Summary, 'id'> {
  return {
    title: item.title ?? null,
    language: item.language ?? null,
    kind: item.kind ?? null,
    pages: Number.isFinite(item.pages) ? item.pages : 0,
    thumbnail_hash: item.thumbnail_hash ?? null,
  }
}
