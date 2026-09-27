// Card lookups, batched.
//
// Each card used to ask for itself, so a page of 25 sent 25 requests that
// queued behind one another; galleries the snapshot has no thumbnail for
// need a network fetch, and the list sat blank while they went one at a
// time. Ids asked for in the same tick go out as a single request, which
// the server resolves concurrently.

import * as api from './api'

/** The server answers this many per request and quietly drops the rest. */
const MAX_PER_REQUEST = 50
/// How many cards to keep. A long session pages through thousands of them,
/// and each holds a title, the artists and the tags - on the device most
/// likely to have its memory taken away.
const KEEP = 400
/** Long enough for a page of cards to mount, short enough not to be felt. */
const BATCH_MS = 20

type Pending = {
  resolve: (card: api.Card | null) => void
  reject: (error: unknown) => void
}

const waiting = new Map<number, Pending[]>()
const resolved = new Map<number, api.Card>()
let timer: ReturnType<typeof setTimeout> | null = null

/// Thrown away when the language changes: what a card holds includes its
/// tags, and those are said in the language they were asked for.
export function forget() {
  resolved.clear()
}

export function card(id: number): Promise<api.Card | null> {
  const cached = resolved.get(id)
  if (cached) {
    // Asked for again, so it is not the one to drop next.
    resolved.delete(id)
    resolved.set(id, cached)
    return Promise.resolve(cached)
  }

  return new Promise((resolve, reject) => {
    const queue = waiting.get(id)
    if (queue) {
      queue.push({ resolve, reject })
      return
    }
    waiting.set(id, [{ resolve, reject }])
    if (timer === null) timer = setTimeout(flush, BATCH_MS)
  })
}

/// Drops the ones asked for longest ago. A Map keeps insertion order, so
/// the oldest is the first one out.
function trim() {
  while (resolved.size > KEEP) {
    const oldest = resolved.keys().next().value
    if (oldest === undefined) break
    resolved.delete(oldest)
  }
}

async function flush() {
  timer = null
  const ids = [...waiting.keys()].slice(0, MAX_PER_REQUEST)
  if (ids.length === 0) return

  const claimed = new Map<number, Pending[]>()
  for (const id of ids) {
    claimed.set(id, waiting.get(id)!)
    waiting.delete(id)
  }
  // Anything left over goes in the next batch.
  if (waiting.size > 0 && timer === null) timer = setTimeout(flush, BATCH_MS)

  try {
    const cards = await api.cards(ids)
    const byId = new Map(cards.map((c) => [c.id, c]))
    for (const [id, listeners] of claimed) {
      const found = byId.get(id) ?? null
      if (found) resolved.set(id, found)
      for (const listener of listeners) listener.resolve(found)
    }
    trim()
  } catch (error) {
    for (const listeners of claimed.values()) {
      for (const listener of listeners) listener.reject(error)
    }
  }
}
