import { describe, expect, test } from 'vitest'
import {
  DEFAULTS,
  thumbnailOf,
  fitKey,
  fitOf,
  forwardFor,
  forwardForTap,
  sanitise,
  spreadOf,
  spreads,
  step,
  toPrefetch,
} from './reader.svelte'

describe('spreads', () => {
  test('the cover stands alone, and the rest pair up behind it', () => {
    expect(spreads(7, true)).toEqual([[0], [1, 2], [3, 4], [5, 6]])
  })

  test('without a cover the pairing starts at the first page', () => {
    expect(spreads(6, false)).toEqual([
      [0, 1],
      [2, 3],
      [4, 5],
    ])
  })

  test('an odd last page is shown by itself rather than paired with nothing', () => {
    expect(spreads(6, true)).toEqual([[0], [1, 2], [3, 4], [5]])
    expect(spreads(1, true)).toEqual([[0]])
  })

  test('a work with no pages has no spreads', () => {
    expect(spreads(0, true)).toEqual([])
    expect(spreads(-3, true)).toEqual([])
  })

  test('every page belongs to exactly one spread', () => {
    for (const cover of [true, false]) {
      for (let count = 1; count <= 12; count++) {
        const groups = spreads(count, cover)
        const flat = groups.flat()
        expect(flat).toEqual([...Array(count).keys()])
      }
    }
  })
})

describe('spreadOf', () => {
  const groups = spreads(7, true)

  test('finds the spread a page sits in', () => {
    expect(spreadOf(0, groups)).toBe(0)
    expect(spreadOf(2, groups)).toBe(1)
    expect(spreadOf(6, groups)).toBe(3)
  })

  test('a page outside the work falls back to the first', () => {
    expect(spreadOf(99, groups)).toBe(0)
  })
})

describe('step', () => {
  test('moves one page at a time when pages are shown one at a time', () => {
    expect(step(3, true, 10, 'page', true)).toBe(4)
    expect(step(3, false, 10, 'page', true)).toBe(2)
    expect(step(3, true, 10, 'scroll', true)).toBe(4)
  })

  test('moves a whole spread at a time, landing on its first page', () => {
    // [0] [1,2] [3,4] [5,6]
    expect(step(0, true, 7, 'spread', true)).toBe(1)
    expect(step(1, true, 7, 'spread', true)).toBe(3)
    expect(step(2, true, 7, 'spread', true)).toBe(3)
    expect(step(4, false, 7, 'spread', true)).toBe(1)
  })

  test('stops at both ends instead of running off', () => {
    expect(step(0, false, 10, 'page', true)).toBe(0)
    expect(step(9, true, 10, 'page', true)).toBe(9)
    expect(step(0, false, 7, 'spread', true)).toBe(0)
    expect(step(6, true, 7, 'spread', true)).toBe(5)
  })

  test('a work with no pages cannot be stepped through', () => {
    expect(step(0, true, 0, 'page', true)).toBe(0)
    expect(step(0, true, 0, 'spread', true)).toBe(0)
  })
})

describe('forwardFor', () => {
  test('the arrows swap with the reading direction', () => {
    expect(forwardFor('ArrowRight', 'ltr')).toBe(true)
    expect(forwardFor('ArrowLeft', 'ltr')).toBe(false)
    expect(forwardFor('ArrowRight', 'rtl')).toBe(false)
    expect(forwardFor('ArrowLeft', 'rtl')).toBe(true)
  })

  test('space and the page keys always mean onward or back', () => {
    for (const direction of ['ltr', 'rtl'] as const) {
      expect(forwardFor(' ', direction)).toBe(true)
      expect(forwardFor('PageDown', direction)).toBe(true)
      expect(forwardFor('PageUp', direction)).toBe(false)
    }
  })

  test('any other key means nothing', () => {
    expect(forwardFor('Escape', 'ltr')).toBe(null)
    expect(forwardFor('a', 'rtl')).toBe(null)
  })
})

describe('forwardForTap', () => {
  test('the outer thirds turn the page, the way the direction says', () => {
    expect(forwardForTap(10, 900, 'ltr')).toBe(false)
    expect(forwardForTap(890, 900, 'ltr')).toBe(true)
    expect(forwardForTap(10, 900, 'rtl')).toBe(true)
    expect(forwardForTap(890, 900, 'rtl')).toBe(false)
  })

  test('the middle turns nothing, so the controls can be reached', () => {
    expect(forwardForTap(450, 900, 'ltr')).toBe(null)
    expect(forwardForTap(450, 900, 'rtl')).toBe(null)
  })

  test('a viewport with no width cannot be tapped through', () => {
    expect(forwardForTap(5, 0, 'ltr')).toBe(null)
  })
})

describe('toPrefetch', () => {
  test('reaches ahead, and one back because turning back is common', () => {
    expect(toPrefetch(4, 20, 2)).toEqual([5, 6, 3])
  })

  test('does not reach past either end', () => {
    expect(toPrefetch(0, 3, 2)).toEqual([1, 2])
    expect(toPrefetch(2, 3, 2)).toEqual([1])
    expect(toPrefetch(0, 1, 2)).toEqual([])
  })
})

describe('toPrefetch, out of range', () => {
  test('never names a page the work does not have', () => {
    expect(toPrefetch(40, 3, 3)).toEqual([])
    expect(toPrefetch(0, 0, 3)).toEqual([])
    expect(toPrefetch(2, 3, 3)).toEqual([1])
  })
})

describe('sanitise', () => {
  test('keeps what is valid', () => {
    const settings = {
      layout: 'spread',
      direction: 'rtl',
      fit: 'height',
      pageFit: 'original',
      coverAlone: false,
    } as const
    expect(sanitise(settings)).toEqual(settings)
  })

  test('replaces what is not, rather than rendering an impossible state', () => {
    expect(sanitise({ layout: 'sideways' as never })).toEqual(DEFAULTS)
    expect(sanitise({ fit: '' as never }).fit).toBe(DEFAULTS.fit)
    expect(sanitise({ pageFit: 'tall' as never }).pageFit).toBe(DEFAULTS.pageFit)
    expect(sanitise({ direction: 'ttb' as never }).direction).toBe('ltr')
  })

  test('an empty object is the defaults', () => {
    expect(sanitise({})).toEqual(DEFAULTS)
  })
})

describe('fit', () => {
  test('scrolling and turning remember their own', () => {
    expect(fitKey('scroll')).toBe('fit')
    expect(fitKey('page')).toBe('pageFit')
    expect(fitKey('spread')).toBe('pageFit')
  })

  test('the one in force is the one for the layout', () => {
    const settings = { ...DEFAULTS, fit: 'width', pageFit: 'height' } as const
    expect(fitOf({ ...settings, layout: 'scroll' })).toBe('width')
    expect(fitOf({ ...settings, layout: 'spread' })).toBe('height')
  })
})

describe('thumbnailOf', () => {
  test('asks for the small copy under the same hash', () => {
    expect(thumbnailOf('/img/abc123.avif')).toBe('/tn/abc123.avif')
  })

  test('leaves anything that is not a page alone', () => {
    expect(thumbnailOf('/tn/abc123.avif')).toBe('/tn/abc123.avif')
    expect(thumbnailOf('https://example.test/img/a.avif')).toBe('https://example.test/img/a.avif')
  })
})
