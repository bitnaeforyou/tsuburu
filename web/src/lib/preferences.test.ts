/// The language a reader picks is meant to still be there next time.

import { beforeEach, describe, expect, test } from 'vitest'
import { preferred, remember } from './preferences'
import { parse, toSearch } from './router'

beforeEach(() => {
  localStorage.clear()
  remember({ language: 'all', kind: 'all' })
})

describe('what a reader usually asks for', () => {
  test('stands in for the default when the address says nothing', () => {
    remember({ language: 'korean' })
    const route = parse('#/?q=glasses')
    expect(route.name).toBe('search')
    expect(route.name === 'search' && route.language).toBe('korean')
  })

  test('loses to an address that names one', () => {
    remember({ language: 'korean' })
    const route = parse('#/?q=glasses&language=japanese')
    expect(route.name === 'search' && route.language).toBe('japanese')
  })

  test('is left out of the address, so the next launch picks it up', () => {
    remember({ language: 'korean' })
    expect(toSearch({ query: 'glasses', language: 'korean' })).toBe('#/?q=glasses')
    expect(toSearch({ query: 'glasses', language: 'all' })).toContain('language=all')
  })

  test('survives a write that only names one of the two', () => {
    remember({ language: 'korean' })
    remember({ kind: 'manga' })
    expect(preferred.language).toBe('korean')
    expect(preferred.kind).toBe('manga')
  })
})
