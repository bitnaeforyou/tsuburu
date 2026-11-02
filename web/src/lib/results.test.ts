/// Coming back to a search should find it as it was left.

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { forget, markScroll, recall, remember } from './results.svelte'

const found = (n: number) => ({
  ids: [n],
  terms: [],
  hits: [],
  total: n,
  offset: n,
  reachable: 0,
  narrowed: false,
  scrollY: 0,
})

beforeEach(forget)

describe('a search that has been run', () => {
  test('is found again by the same question', () => {
    remember('korean', found(3))
    expect(recall('korean')?.total).toBe(3)
  })

  test('is not confused with a different one', () => {
    remember('korean', found(3))
    expect(recall('japanese')).toBeNull()
  })

  test('remembers how far down the page was', () => {
    remember('korean', found(3))
    markScroll('korean', { scrollY: 1200, anchor: { href: '#/g/7', top: 40 } })
    expect(recall('korean')?.scrollY).toBe(1200)
    expect(recall('korean')?.anchor).toEqual({ href: '#/g/7', top: 40 })
  })

  test('loading another page keeps where the page was', () => {
    remember('korean', found(3))
    markScroll('korean', { scrollY: 1200, anchor: { href: '#/g/7', top: 40 } })
    remember('korean', found(4))
    expect(recall('korean')?.scrollY).toBe(1200)
    expect(recall('korean')?.anchor?.href).toBe('#/g/7')
  })

  /// The "load more" under a dialogue search is offered only while there is
  /// more within reach, so coming back without it took the button away.
  test('keeps how far a dialogue search can page', () => {
    remember('대사', { ...found(30), reachable: 100 })
    expect(recall('대사')?.reachable).toBe(100)
  })

  test('marking a search nobody ran is not an error', () => {
    markScroll('nothing', { scrollY: 500 })
    expect(recall('nothing')).toBeNull()
  })
})

describe('when there have been many', () => {
  test('the oldest is the one that goes', () => {
    for (let i = 0; i < 8; i++) remember(`q${i}`, found(i))
    expect(recall('q0')).toBeNull()
    expect(recall('q1')).toBeNull()
    expect(recall('q7')?.total).toBe(7)
  })

  test('asking for one again keeps it from being the oldest', () => {
    for (let i = 0; i < 6; i++) remember(`q${i}`, found(i))
    expect(recall('q0')?.total).toBe(0)
    remember('q6', found(6))
    // q1 was the oldest once q0 was asked for again.
    expect(recall('q1')).toBeNull()
    expect(recall('q0')?.total).toBe(0)
  })
})

describe('a search left for a while', () => {
  afterEach(() => vi.useRealTimers())

  /// hitomi is posted to all day; put back with no age at all, the first
  /// screen showed what it had first found for weeks.
  test('is asked again once it is old', () => {
    vi.useFakeTimers()
    remember('home', found(3))
    vi.advanceTimersByTime(60 * 60 * 1000)
    expect(recall('home')?.total).toBe(3)
    vi.advanceTimersByTime(3 * 60 * 60 * 1000)
    expect(recall('home')).toBeNull()
  })

  test('is as old as its first page, however many pages came after', () => {
    vi.useFakeTimers()
    remember('home', found(3))
    vi.advanceTimersByTime(2 * 60 * 60 * 1000)
    remember('home', found(4))
    vi.advanceTimersByTime(90 * 60 * 1000)
    expect(recall('home')).toBeNull()
  })

  /// What a closed program wrote down is only put back if it is recent:
  /// opening the program again is coming back for what is new.
  test('written down by a program since closed, is put back only if recent', async () => {
    const write = (foundAt?: number) =>
      localStorage.setItem(
        'tsuburu.results',
        JSON.stringify([['home', { ids: [1], total: 1, offset: 1, scrollY: 0, foundAt }]]),
      )
    const fresh = async () => {
      vi.resetModules()
      return (await import('./results.svelte')).recall('home')
    }

    write(Date.now() - 5 * 60 * 1000)
    expect((await fresh())?.total).toBe(1)
    write(Date.now() - 2 * 60 * 60 * 1000)
    expect(await fresh()).toBeNull()
    // Written by a version that kept no time at all.
    write(undefined)
    expect(await fresh()).toBeNull()
  })
})
