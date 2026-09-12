/// Coming back to a search should find it as it was left.

import { beforeEach, describe, expect, test } from 'vitest'
import { forget, markScroll, recall, remember } from './results.svelte'

const found = (n: number) => ({
  ids: [n],
  terms: [],
  hits: [],
  total: n,
  offset: n,
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
    markScroll('korean', 1200)
    expect(recall('korean')?.scrollY).toBe(1200)
  })

  test('marking a search nobody ran is not an error', () => {
    markScroll('nothing', 500)
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
