/// Copying has to work inside a phone's webview, which may refuse the
/// clipboard that a browser would offer.

import { afterEach, describe, expect, test, vi } from 'vitest'
import { copy } from './clipboard'

afterEach(() => vi.unstubAllGlobals())

describe('copying a number', () => {
  test('uses the clipboard when it is there', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    expect(await copy('4275623')).toBe(true)
    expect(writeText).toHaveBeenCalledWith('4275623')
  })

  test('falls back to copying a selection when the clipboard refuses', async () => {
    vi.stubGlobal('navigator', {
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
    })
    const execCommand = vi.fn().mockReturnValue(true)
    document.execCommand = execCommand
    expect(await copy('1\n2')).toBe(true)
    expect(execCommand).toHaveBeenCalledWith('copy')
    // Nothing is left behind in the page.
    expect(document.querySelector('textarea')).toBeNull()
  })

  test('says so when neither way works', async () => {
    vi.stubGlobal('navigator', {})
    document.execCommand = vi.fn().mockReturnValue(false)
    expect(await copy('1')).toBe(false)
  })
})
