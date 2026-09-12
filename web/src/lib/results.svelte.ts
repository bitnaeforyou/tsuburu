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

const seen = new Map<string, Remembered>()

export function remember(key: string, found: Remembered) {
  seen.delete(key)
  seen.set(key, found)
  // A Map keeps insertion order, so the oldest is the first one out.
  while (seen.size > KEEP) {
    const oldest = seen.keys().next().value
    if (oldest === undefined) break
    seen.delete(oldest)
  }
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
}

/// Where the page was left, for the one that is on screen now.
export function markScroll(key: string, scrollY: number) {
  const found = seen.get(key)
  if (found) found.scrollY = scrollY
}
