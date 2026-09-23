/// What a search found, kept for as long as it is likely to be wanted again.
///
/// Opening a work takes the results off the screen; coming back used to ask
/// for them all over again, from the first page, at the top. The answer to a
/// search does not change while you read one of its results, so it is kept
/// and put back exactly as it was left - including how far down it was.

import type { DialogueHit, Term } from './api'

export type Remembered = {
  ids: number[]
  terms: Term[]
  hits: DialogueHit[]
  total: number
  offset: number
  scrollY: number
}

/// How many searches back it remembers. Enough to wander between a few
/// results and a couple of searches without losing any of them.
const KEEP = 6

const STORED = 'tsuburu.results'

/// Kept on the disk as well as in memory.
///
/// A phone takes the app's window away when it needs the memory and builds it
/// again on the way back, which to the page is a reload: twenty pressings of
/// "load more" gone, and the top of the list again. What was found is small
/// enough to write down - a few thousand numbers - so it is.
///
/// The passages a dialogue search found are not written down; they are large,
/// and asking again is answered from the store's own cache.
type Stored = Omit<Remembered, 'hits' | 'terms'>

function load(): Map<string, Remembered> {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return new Map()
    const rows = JSON.parse(raw) as [string, Stored][]
    return new Map(rows.map(([key, row]) => [key, { ...row, terms: [], hits: [] }]))
  } catch {
    return new Map()
  }
}

const seen = load()

function save() {
  try {
    const rows: [string, Stored][] = [...seen].map(([key, { ids, total, offset, scrollY }]) => [
      key,
      { ids, total, offset, scrollY },
    ])
    localStorage.setItem(STORED, JSON.stringify(rows))
  } catch {
    // A browser with storage switched off still gets to search.
  }
}

export function remember(key: string, found: Remembered) {
  // Where it was left belongs to the search, not to this particular set of
  // results: loading another page must not send it back to the top.
  const scrollY = found.scrollY || (seen.get(key)?.scrollY ?? 0)
  seen.delete(key)
  seen.set(key, { ...found, scrollY })
  // A Map keeps insertion order, so the oldest is the first one out.
  while (seen.size > KEEP) {
    const oldest = seen.keys().next().value
    if (oldest === undefined) break
    seen.delete(oldest)
  }
  save()
}

export function recall(key: string): Remembered | null {
  const found = seen.get(key)
  if (!found) return null
  // Asked for again, so it is the freshest thing here.
  seen.delete(key)
  seen.set(key, found)
  return found
}

export function forget() {
  seen.clear()
  save()
}

/// Where the page was left, for the one that is on screen now.
export function markScroll(key: string, scrollY: number) {
  const found = seen.get(key)
  if (!found || found.scrollY === scrollY) return
  found.scrollY = scrollY
  save()
}
