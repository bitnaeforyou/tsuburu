/// What a search found, kept for as long as it is likely to be wanted again.
///
/// Opening a work takes the results off the screen; coming back used to ask
/// for them all over again, from the first page, at the top. The answer to a
/// search does not change while you read one of its results, so it is kept
/// and put back exactly as it was left - including how far down it was.

import type { DialogueHit, Term } from './api'

/// The result at the top of the screen, and how far down the screen it sat.
///
/// A pixel offset alone does not find the place again: cards that have not
/// been drawn since the list came back are given a guessed height, so the
/// same offset lands some rows away from where it was.
export type Anchor = { href: string; top: number }

/// Where the page was left.
export type Place = { scrollY: number; anchor?: Anchor }

export type Remembered = Place & {
  ids: number[]
  terms: Term[]
  hits: DialogueHit[]
  total: number
  offset: number
  /// How far into a dialogue search paging can go, which is what decides
  /// whether there is a "load more" to offer.
  reachable: number
  /// Whether the first screen's tags narrowed what was found.
  narrowed: boolean
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

type Written = Omit<Stored, 'reachable' | 'narrowed'> &
  Partial<Pick<Stored, 'reachable' | 'narrowed'>>

function load(): Map<string, Remembered> {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return new Map()
    // Rows written before a field existed come back without it.
    const rows = JSON.parse(raw) as [string, Written][]
    return new Map(
      rows.map(([key, row]) => [
        key,
        {
          ...row,
          reachable: row.reachable ?? 0,
          narrowed: row.narrowed ?? false,
          terms: [],
          hits: [],
        },
      ]),
    )
  } catch {
    return new Map()
  }
}

const seen = load()

function save() {
  try {
    const rows: [string, Stored][] = [...seen].map(
      ([key, { ids, total, offset, reachable, narrowed, scrollY, anchor }]) => [
        key,
        { ids, total, offset, reachable, narrowed, scrollY, anchor },
      ],
    )
    localStorage.setItem(STORED, JSON.stringify(rows))
  } catch {
    // A browser with storage switched off still gets to search.
  }
}

export function remember(key: string, found: Remembered) {
  // Where it was left belongs to the search, not to this particular set of
  // results: loading another page must not send it back to the top.
  const before = seen.get(key)
  const scrollY = found.scrollY || (before?.scrollY ?? 0)
  const anchor = found.anchor ?? before?.anchor
  seen.delete(key)
  seen.set(key, { ...found, scrollY, anchor })
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
export function markScroll(key: string, place: Place) {
  const found = seen.get(key)
  if (!found) return
  found.scrollY = place.scrollY
  found.anchor = place.anchor
  save()
}
