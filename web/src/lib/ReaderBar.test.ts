/// The row of controls: that it moves the reader the way it says it does, and
/// that a choice made here is still the choice on the next visit.

import { beforeEach, describe, expect, test } from 'vitest'
import { render } from '@testing-library/svelte'
import { tick } from 'svelte'
import Harness from '../testing/ReaderBarHarness.svelte'
import { DEFAULTS, reader, type ReaderSettings } from './reader.svelte'

function settings(overrides: Partial<ReaderSettings> = {}) {
  reader.settings = { ...DEFAULTS, ...overrides }
}

const at = (screen: { getByTestId: (id: string) => HTMLElement }) =>
  Number(screen.getByTestId('current').textContent)

beforeEach(() => {
  localStorage.clear()
  settings()
})

describe('turning', () => {
  test('the buttons move a page at a time and stop at the ends', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    const [previous, next] = [...screen.container.querySelectorAll('.turn button')] as HTMLButtonElement[]

    expect(previous).toBeDisabled()
    next.click()
    await tick()
    expect(at(screen)).toBe(1)
    expect(previous).not.toBeDisabled()

    next.click()
    next.click()
    await tick()
    expect(at(screen)).toBe(3)
    expect(next).toBeDisabled()
  })

  test('a spread moves two pages at a time once past the cover', async () => {
    settings({ layout: 'spread' })
    const screen = render(Harness, { count: 8 })
    await tick()
    const next = screen.container.querySelectorAll('.turn button')[1] as HTMLButtonElement

    next.click()
    await tick()
    expect(at(screen)).toBe(1)

    next.click()
    await tick()
    expect(at(screen)).toBe(3)
  })

  test('the scrubber goes straight to a page', async () => {
    const screen = render(Harness, { count: 40 })
    await tick()
    const scrub = screen.container.querySelector('.scrub') as HTMLInputElement

    expect(scrub.max).toBe('39')
    scrub.value = '27'
    scrub.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(at(screen)).toBe(27)
  })

  test('reading right to left puts onward on the left', async () => {
    settings({ direction: 'rtl' })
    const screen = render(Harness, { count: 4 })
    await tick()
    expect(screen.container.querySelector('.turn')).toHaveClass('rtl')
    expect(screen.container.querySelector('.scrub')).toHaveAttribute('dir', 'rtl')
  })
})

describe('settings', () => {
  test('a layout is chosen, marked, and kept', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    const buttons = [...screen.container.querySelectorAll('.group button')] as HTMLButtonElement[]

    expect(buttons[0]).toHaveAttribute('aria-pressed', 'true')
    buttons[2].click()
    await tick()

    expect(reader.settings.layout).toBe('spread')
    expect(buttons[2]).toHaveAttribute('aria-pressed', 'true')
    expect(JSON.parse(localStorage.getItem('tsuburu.reader')!).layout).toBe('spread')
  })

  test('the direction is one button, both ways', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    const toggle = screen.container.querySelector('.wide') as HTMLButtonElement

    toggle.click()
    await tick()
    expect(reader.settings.direction).toBe('rtl')

    toggle.click()
    await tick()
    expect(reader.settings.direction).toBe('ltr')
  })

  test('the fit is chosen from the list', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    const select = screen.container.querySelector('select') as HTMLSelectElement

    select.value = 'height'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(reader.settings.fit).toBe('height')
  })

  test('scrolling and turning keep their own fit', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    const select = screen.container.querySelector('select') as HTMLSelectElement
    expect(select.value).toBe(DEFAULTS.fit)

    settings({ layout: 'page' })
    await tick()
    expect(select.value).toBe(DEFAULTS.pageFit)

    select.value = 'original'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(reader.settings.pageFit).toBe('original')
    expect(reader.settings.fit).toBe(DEFAULTS.fit)
  })

  test('the cover question is asked only where it means something', async () => {
    const screen = render(Harness, { count: 4 })
    await tick()
    expect(screen.container.querySelector('.check')).toBeNull()

    settings({ layout: 'spread' })
    await tick()
    const box = screen.container.querySelector('.check input') as HTMLInputElement
    expect(box).toBeChecked()

    box.click()
    await tick()
    expect(reader.settings.coverAlone).toBe(false)
  })
})
