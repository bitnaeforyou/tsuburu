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

/// How long a search is put back as it was found, rather than asked again.
///
/// Long enough to read a work and come back to the list at the same place.
/// Not for ever: hitomi is posted to all day, and kept with no age at all
/// the first screen showed what it had first found for weeks, through
/// updates, until the reload button was pressed.
const KEPT_FOR = 3 * 60 * 60 * 1000

/// The same, for what was written down by a program that has since been
/// closed. Opening it again is coming back for what is new.
const KEPT_ACROSS_LAUNCHES = 30 * 60 * 1000

/// A search as it is held: what was found, and when its first page was.
type Held = Remembered & { foundAt: number }

/// Kept on the disk as well as in memory.
///
/// A phone takes the app's window away when it needs the memory and builds it
/// again on the way back, which to the page is a reload: twenty pressings of
/// "load more" gone, and the top of the list again. What was found is small
/// enough to write down - a few thousand numbers - so it is.
///
/// The passages a dialogue search found are not written down; they are large,
/// and asking again is answered from the store's own cache.
type Stored = Omit<Held, 'hits' | 'terms'>

type Written = Omit<Stored, 'reachable' | 'narrowed' | 'foundAt'> &
  Partial<Pick<Stored, 'reachable' | 'narrowed' | 'foundAt'>>

function load(): Map<string, Held> {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return new Map()
    // Rows written before a field existed come back without it - and one
    // with no time on it is as old as can be, so it is not put back.
    const rows = JSON.parse(raw) as [string, Written][]
    return new Map(
      rows
        .filter(([, row]) => Date.now() - (row.foundAt ?? 0) < KEPT_ACROSS_LAUNCHES)
        .map(([key, row]) => [
          key,
          {
            ...row,
            foundAt: row.foundAt ?? 0,
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

const seen: Map<string, Held> = load()

function save() {
  try {
    const rows: [string, Stored][] = [...seen].map(
      ([key, { ids, total, offset, reachable, narrowed, scrollY, anchor, foundAt }]) => [
        key,
        { ids, total, offset, reachable, narrowed, scrollY, anchor, foundAt },
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
  // Loading another page is the same answer, found when its first page was.
  const foundAt = before?.foundAt ?? Date.now()
  seen.delete(key)
  seen.set(key, { ...found, scrollY, anchor, foundAt })
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
  // Too old to show as it was: dropped, so the search asks again and what
  // it finds is written down as new rather than under the old time.
  if (Date.now() - found.foundAt >= KEPT_FOR) {
    seen.delete(key)
    save()
    return null
  }
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
