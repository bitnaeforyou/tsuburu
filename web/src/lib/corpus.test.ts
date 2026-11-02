/// The corpus takes the better part of half an hour; the screen has to say
/// how much of that is left.

import { describe, expect, test } from 'vitest'
import { megabytes, minutesLeft, secondsLeft } from './corpus'

describe('time left on the corpus', () => {
  test('follows from how long the part already in took', () => {
    // A quarter in after five minutes: fifteen more.
    expect(secondsLeft(100, 400, 300)).toBe(900)
  })

  test('is not guessed before anything has arrived', () => {
    expect(secondsLeft(0, 400, 12)).toBeNull()
    expect(secondsLeft(400, 400, 600)).toBeNull()
  })

  test('is said in whole minutes, and never as none while some is left', () => {
    expect(minutesLeft(900)).toBe(15)
    expect(minutesLeft(20)).toBe(1)
  })

  test('sizes are whole megabytes', () => {
    expect(megabytes(503_316_480)).toBe(480)
  })
})
