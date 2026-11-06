/// Finding a work among those downloaded, and handing their numbers on.

import { describe, expect, test } from 'vitest'
import { matchesAsked, numberList } from './downloads'
import { galleriesNamed } from './router'

const work = (id: number, title: string | null) => ({ id, title })

describe('matchesAsked', () => {
  test('anything is an answer to nothing asked', () => {
    expect(matchesAsked(work(1, '제목'), '   ')).toBe(true)
  })

  test('finds a part of the title, whatever its case', () => {
    expect(matchesAsked(work(1, 'Summer Vacation'), 'vaca')).toBe(true)
    expect(matchesAsked(work(1, 'Summer Vacation'), 'winter')).toBe(false)
  })

  test('finds Hangul however it was composed', () => {
    expect(matchesAsked(work(1, '여름 방학'.normalize('NFD')), '방학')).toBe(true)
  })

  test('finds a number, with or without its mark', () => {
    expect(matchesAsked(work(4183648, null), '41836')).toBe(true)
    expect(matchesAsked(work(4183648, null), '#4183648')).toBe(true)
    expect(matchesAsked(work(4183648, null), '#83648')).toBe(false)
  })

  test('every word has to be there, in any order', () => {
    expect(matchesAsked(work(42, '여름 방학'), '방학 42')).toBe(true)
    expect(matchesAsked(work(42, '여름 방학'), '방학 겨울')).toBe(false)
  })
})

describe('numberList', () => {
  test('separates numbers with commas', () => {
    expect(numberList([4183648, 4183650])).toBe('4183648, 4183650')
  })

  test('is what the box that takes numbers reads back', () => {
    expect(galleriesNamed(numberList([4183648, 4183650])).ids).toEqual([4183648, 4183650])
  })
})
