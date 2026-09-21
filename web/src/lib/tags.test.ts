/// A tag says who it is about before it says anything else.

import { describe, expect, test } from 'vitest'
import { split } from './tags'

describe('splitting a tag', () => {
  test('takes the namespace off and remembers it', () => {
    expect(split('female:big breasts')).toEqual({ word: 'big breasts', who: 'female' })
    expect(split('male:sole male')).toEqual({ word: 'sole male', who: 'male' })
  })

  test('leaves one with no namespace alone', () => {
    expect(split('group')).toEqual({ word: 'group', who: null })
    expect(split('mmf threesome')).toEqual({ word: 'mmf threesome', who: null })
  })

  test('does not mistake a colon elsewhere for one', () => {
    expect(split('re:zero')).toEqual({ word: 're:zero', who: null })
  })
})
