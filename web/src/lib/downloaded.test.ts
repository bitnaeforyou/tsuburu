/// A cover marks what is downloaded, and has to stop marking it when it is not.

import { beforeEach, describe, expect, test } from 'vitest'
import type { DownloadItem } from './api'
import { downloaded } from './library.svelte'

const item = (id: number, have: number, pages: number): DownloadItem => ({
  id,
  title: null,
  language: null,
  pages,
  have,
  bytes: 0,
  added_at: 0,
  complete: have === pages,
  job: { running: false, wanted: 0, fetched: 0, failed: 0, error: null },
})

beforeEach(() => downloaded.replace([]))

describe('what is downloaded', () => {
  test('is known by the work', () => {
    downloaded.replace([item(1, 30, 30), item(2, 12, 30)])
    expect(downloaded.of(1)).toEqual({ have: 30, pages: 30, complete: true })
    expect(downloaded.of(2)?.complete).toBe(false)
    expect(downloaded.of(3)).toBeNull()
  })

  test('follows a download as it goes', () => {
    downloaded.note(item(4, 3, 20))
    downloaded.note(item(4, 20, 20))
    expect(downloaded.of(4)?.complete).toBe(true)
  })

  /// The downloads screen says what is on disk, so a work deleted from it
  /// stops being marked everywhere.
  test('is whatever the downloads screen last saw', () => {
    downloaded.replace([item(1, 30, 30), item(2, 30, 30)])
    downloaded.replace([item(2, 30, 30)])
    expect(downloaded.of(1)).toBeNull()
  })

  test('forgets a work that turns out not to be there', () => {
    downloaded.note(item(5, 10, 10))
    downloaded.forget(5)
    expect(downloaded.of(5)).toBeNull()
  })
})
