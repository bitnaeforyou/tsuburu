/// A backup is only worth having if it reads back, so these check the file
/// both ways and what happens to a file that has been damaged.

import { describe, expect, test, vi } from 'vitest'
import type { Favorite, HistoryEntry } from './api'
import { nameFor, NotABackup, pack, restore, unpack, VERSION, type Restorer } from './backup'

const work = (id: number): Favorite => ({
  id,
  title: `Work ${id}`,
  language: 'korean',
  kind: 'manga',
  pages: 20,
  thumbnail_hash: `hash${id}`,
  added_at: 1700000000,
})

const read = (id: number, page: number): HistoryEntry => ({
  ...work(id),
  last_seen_at: 1700000001,
  last_page: page,
})

function fake(): Restorer & { calls: string[] } {
  const calls: string[] = []
  return {
    calls,
    favorite: async (id) => void calls.push(`favorite:${id}`),
    follow: async (name) => void calls.push(`follow:${name}`),
    progress: async (id, page) => void calls.push(`progress:${id}:${page}`),
  }
}

describe('the file', () => {
  test('is named for the day it was made', () => {
    expect(nameFor(new Date('2026-09-12T04:00:00Z'))).toBe('tsuburu-2026-09-12.json')
  })

  test('reads back exactly as it was written', () => {
    const made = pack([work(1), work(2)], ['himura', 'mitsumi'], [read(3, 7)])
    expect(unpack(JSON.stringify(made))).toEqual(made)
    expect(made.tsuburu).toBe(VERSION)
  })
})

describe('a file that is not one', () => {
  test('is refused rather than half read', () => {
    expect(() => unpack('not json')).toThrow(NotABackup)
    expect(() => unpack('[]')).toThrow(NotABackup)
    expect(() => unpack('{"favorites":[]}')).toThrow(NotABackup)
  })

  test('from a newer tsuburu is refused, not guessed at', () => {
    expect(() => unpack(JSON.stringify({ tsuburu: VERSION + 1 }))).toThrow(NotABackup)
  })

  test('missing everything restores nothing rather than failing', () => {
    const empty = unpack(JSON.stringify({ tsuburu: VERSION }))
    expect(empty).toEqual({
      tsuburu: VERSION,
      exported: '',
      favorites: [],
      artists: [],
      history: [],
    })
  })

  test('keeps the records that survive and drops the rest', () => {
    const damaged = unpack(
      JSON.stringify({
        tsuburu: VERSION,
        favorites: [work(1), { title: 'no id' }, null, 7],
        artists: ['himura', '', 3, null],
        history: [read(2, -5), { id: 3 }],
      }),
    )
    expect(damaged.favorites.map((f) => f.id)).toEqual([1])
    expect(damaged.artists).toEqual(['himura'])
    expect(damaged.history.map((h) => [h.id, h.last_page])).toEqual([
      [2, 0],
      [3, 0],
    ])
  })
})

describe('putting it back', () => {
  test('asks for every favorite, artist and page', async () => {
    const into = fake()
    const result = await restore(
      pack([work(1), work(2)], ['himura'], [read(3, 7), read(4, 0)]),
      into,
    )

    expect(result).toEqual({ done: 5, failed: 0 })
    expect(into.calls.sort()).toEqual(
      ['favorite:1', 'favorite:2', 'follow:himura', 'progress:3:7', 'progress:4:0'].sort(),
    )
  })

  test('one the server refuses does not stop the others', async () => {
    const into = fake()
    into.favorite = vi.fn(async (id: number) => {
      if (id === 1) throw new Error('refused')
      into.calls.push(`favorite:${id}`)
    })

    const result = await restore(pack([work(1), work(2)], [], [read(3, 1)]), into)

    expect(result).toEqual({ done: 3, failed: 1 })
    expect(into.calls).toContain('favorite:2')
    expect(into.calls).toContain('progress:3:1')
  })

  test('says how far along it is', async () => {
    const seen: number[] = []
    await restore(pack([work(1), work(2)], ['a'], []), fake(), (done, total) => {
      expect(total).toBe(3)
      seen.push(done)
    })
    expect(seen).toEqual([1, 2, 3])
  })

  test('an empty backup is not an error', async () => {
    expect(await restore(pack([], [], []), fake())).toEqual({ done: 0, failed: 0 })
  })
})
