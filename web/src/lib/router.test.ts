/// A number is an address, not a search term. These say which is which.

import { describe, expect, test } from 'vitest'
import { galleryNamed } from './router'

describe('a query that names a gallery', () => {
  test('is a bare number long enough to be one', () => {
    expect(galleryNamed('4183648')).toBe(4183648)
    expect(galleryNamed('  4183648  ')).toBe(4183648)
    expect(galleryNamed('#4183648')).toBe(4183648)
    expect(galleryNamed('12345')).toBe(12345)
  })

  test('is an address with one in it', () => {
    expect(galleryNamed('https://hitomi.la/doujinshi/some-title-korean-4183648.html')).toBe(4183648)
    expect(galleryNamed('hitomi.la/reader/4183648.html')).toBe(4183648)
    expect(galleryNamed('https://hitomi.la/galleries/4183648.html#3')).toBe(4183648)
    expect(galleryNamed('https://hitomi.la/cg/title-1234567.html?x=1')).toBe(1234567)
  })
})

describe('a query that does not', () => {
  test('is words, however many numbers are in them', () => {
    expect(galleryNamed('korean')).toBeNull()
    expect(galleryNamed('4183648 glasses')).toBeNull()
    expect(galleryNamed('')).toBeNull()
    expect(galleryNamed('   ')).toBeNull()
  })

  test('is a number short enough to be a year or a volume', () => {
    expect(galleryNamed('2024')).toBeNull()
    expect(galleryNamed('3')).toBeNull()
  })

  test('is somebody else"s address', () => {
    expect(galleryNamed('https://example.test/doujinshi/title-4183648.html')).toBeNull()
    expect(galleryNamed('https://nothitomi.la/4183648.html')).toBeNull()
  })
})
