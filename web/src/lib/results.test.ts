/// Coming back to a search should find it as it was left.

import { beforeEach, describe, expect, test } from 'vitest'
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
