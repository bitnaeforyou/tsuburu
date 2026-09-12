/// The reader as it is actually used: a page in a DOM, keys and taps arriving
/// from outside, and the settings singleton the interface writes to.

import { beforeEach, describe, expect, test, vi } from 'vitest'
import { render } from '@testing-library/svelte'
import { tick } from 'svelte'
import Harness from '../testing/ReaderHarness.svelte'
import { observers } from '../testing/setup'
import { DEFAULTS, reader, type ReaderSettings } from './reader.svelte'

const pages = Array.from({ length: 6 }, (_, i) => ({
  src: `/img/page-${i}.avif`,
  width: 1280,
  height: 1810,
}))

function settings(overrides: Partial<ReaderSettings> = {}) {
  reader.settings = { ...DEFAULTS, ...overrides }
}

/// jsdom lays nothing out, so a tap has no side of the screen to land on
/// until the surface is told how wide it is.
function widen(surface: Element, width = 900) {
  vi.spyOn(surface, 'getBoundingClientRect').mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: width,
    bottom: 600,
    width,
    height: 600,
    toJSON: () => ({}),
  })
}

/// jsdom has no PointerEvent, so a mouse event carries the two fields the
/// reader reads off one. `id` is which finger.
function pointer(target: Element, type: string, x: number, y = 300, buttons = 1, id = 1) {
  const event = new MouseEvent(type, { clientX: x, clientY: y, buttons, button: 0, bubbles: true })
  Object.defineProperty(event, 'pointerId', { value: id })
  Object.defineProperty(event, 'pointerType', { value: 'touch' })
  target.dispatchEvent(event)
}

async function press(key: string) {
  dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
  await tick()
}

const at = (screen: { getByTestId: (id: string) => HTMLElement }) =>
  Number(screen.getByTestId('current').textContent)

beforeEach(() => {
  settings()
  localStorage.clear()
})

describe('what is on screen', () => {
  test('scrolling shows every page', async () => {
    const screen = render(Harness, { pages })
    await tick()
    expect(screen.container.querySelectorAll('img')).toHaveLength(6)
  })

  test('paged shows only the page being read', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const images = screen.container.querySelectorAll('img')
    expect(images).toHaveLength(1)
    expect(images[0]).toHaveAttribute('src', '/img/page-2.avif')
  })

  test('a page number past the end shows the last page, not nothing', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages: pages.slice(0, 3), start: 40 })
    await tick()
    const images = screen.container.querySelectorAll('img')
    expect(images).toHaveLength(1)
    expect(images[0]).toHaveAttribute('src', '/img/page-2.avif')
    expect(screen.container.querySelector('.where')).toHaveTextContent('3 / 3')
  })

  test('a work with no pages renders nothing rather than failing', async () => {
    settings({ layout: 'spread' })
    const screen = render(Harness, { pages: [] })
    await tick()
    expect(screen.container.querySelectorAll('img')).toHaveLength(0)
  })

  test('a spread pairs pages and leaves the cover alone', async () => {
    settings({ layout: 'spread' })
    const screen = render(Harness, { pages })
    await tick()
    expect(screen.container.querySelectorAll('img')).toHaveLength(1)

    await press('ArrowRight')
    const images = [...screen.container.querySelectorAll('img')]
    expect(images.map((image) => image.getAttribute('src'))).toEqual([
      '/img/page-1.avif',
      '/img/page-2.avif',
    ])
  })

  test('right to left turns the spread around', async () => {
    settings({ layout: 'spread', direction: 'rtl' })
    const screen = render(Harness, { pages, start: 1 })
    await tick()
    expect(screen.container.querySelector('.stage')).toHaveClass('rtl')
  })
})

describe('keys', () => {
  test('right goes on and left goes back when read left to right', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()

    await press('ArrowRight')
    expect(at(screen)).toBe(3)
    await press('ArrowLeft')
    expect(at(screen)).toBe(2)
  })

  test('left goes on when read right to left', async () => {
    settings({ layout: 'page', direction: 'rtl' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()

    await press('ArrowLeft')
    expect(at(screen)).toBe(3)
  })

  test('space turns the page only where scrolling does not', async () => {
    const screen = render(Harness, { pages, start: 1 })
    await tick()
    await press(' ')
    expect(at(screen)).toBe(1)

    settings({ layout: 'page' })
    await tick()
    await press(' ')
    expect(at(screen)).toBe(2)
  })

  test('the first and last page are the ends', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages })
    await tick()
    await press('ArrowLeft')
    expect(at(screen)).toBe(0)

    for (let i = 0; i < 10; i++) await press('ArrowRight')
    expect(at(screen)).toBe(5)
  })

  test('escape goes back, and typing into a field never turns a page', async () => {
    settings({ layout: 'page' })
    const onback = vi.fn()
    const screen = render(Harness, { pages, start: 2, onback })
    await tick()

    await press('Escape')
    expect(onback).toHaveBeenCalledOnce()

    const field = document.createElement('input')
    document.body.append(field)
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(at(screen)).toBe(2)
    field.remove()
  })
})

describe('taps and swipes', () => {
  test('the far side turns forward and the near side back', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 800)
    pointer(surface, 'pointerup', 800, 300, 0)
    await tick()
    expect(at(screen)).toBe(3)

    pointer(surface, 'pointerdown', 100)
    pointer(surface, 'pointerup', 100, 300, 0)
    await tick()
    expect(at(screen)).toBe(2)
  })

  test('the middle is for the controls, not for turning', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 450)
    pointer(surface, 'pointerup', 450, 300, 0)
    await tick()
    expect(at(screen)).toBe(2)
  })

  test('the middle asks for everything else to get out of the way', async () => {
    settings({ layout: 'page' })
    const onchrome = vi.fn()
    const screen = render(Harness, { pages, start: 2, onchrome })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 450)
    pointer(surface, 'pointerup', 450, 300, 0)
    await tick()

    expect(onchrome).toHaveBeenCalledOnce()
    expect(at(screen)).toBe(2)
  })

  test('a swipe through the middle turns the page instead', async () => {
    settings({ layout: 'page' })
    const onchrome = vi.fn()
    const screen = render(Harness, { pages, start: 2, onchrome })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 600)
    pointer(surface, 'pointerup', 400, 300, 0)
    await tick()

    expect(onchrome).not.toHaveBeenCalled()
    expect(at(screen)).toBe(3)
  })

  test('right to left swaps the sides', async () => {
    settings({ layout: 'page', direction: 'rtl' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 100)
    pointer(surface, 'pointerup', 100, 300, 0)
    await tick()
    expect(at(screen)).toBe(3)
  })

  test('a swipe carries its own direction', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 700)
    pointer(surface, 'pointerup', 500, 300, 0)
    await tick()
    expect(at(screen)).toBe(3)

    pointer(surface, 'pointerdown', 500)
    pointer(surface, 'pointerup', 700, 300, 0)
    await tick()
    expect(at(screen)).toBe(2)
  })

  test('reaching for a control does not turn the page', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)
    const corner = screen.container.querySelector('.corner.full') as HTMLButtonElement

    pointer(corner, 'pointerdown', 860, 560)
    pointer(corner, 'pointerup', 860, 560, 0)
    corner.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()

    expect(at(screen)).toBe(2)
    expect((screen.container.querySelector('.stage') as HTMLElement).style.transform).toBe('')
  })

  test('a drag down the page is not a swipe', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 500, 100)
    pointer(surface, 'pointerup', 430, 500, 0)
    await tick()
    expect(at(screen)).toBe(2)
  })
})

describe('two fingers', () => {
  test('pinching magnifies the page', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 400, 300, 1, 1)
    pointer(surface, 'pointerdown', 500, 300, 1, 2)
    pointer(surface, 'pointermove', 300, 300, 1, 1)
    pointer(surface, 'pointermove', 600, 300, 1, 2)
    await tick()

    const stage = screen.container.querySelector('.stage') as HTMLElement
    expect(stage.style.transform).toContain('scale(3)')
  })

  test('pinching apart and back again leaves the page as it was', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 400, 300, 1, 1)
    pointer(surface, 'pointerdown', 500, 300, 1, 2)
    pointer(surface, 'pointermove', 300, 300, 1, 1)
    pointer(surface, 'pointermove', 600, 300, 1, 2)
    pointer(surface, 'pointermove', 400, 300, 1, 1)
    pointer(surface, 'pointermove', 500, 300, 1, 2)
    await tick()

    const stage = screen.container.querySelector('.stage') as HTMLElement
    expect(stage.style.transform).toBe('')
  })

  test('lifting the fingers after a pinch turns no page', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    pointer(surface, 'pointerdown', 800, 300, 1, 1)
    pointer(surface, 'pointerdown', 820, 300, 1, 2)
    pointer(surface, 'pointerup', 820, 300, 0, 2)
    pointer(surface, 'pointerup', 800, 300, 0, 1)
    await tick()

    expect(at(screen)).toBe(2)
  })
})

describe('a page taller than the frame', () => {
  test('moves under the finger instead of turning', async () => {
    settings({ layout: 'page', pageFit: 'width' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    const stage = screen.container.querySelector('.stage') as HTMLElement
    widen(surface)

    pointer(surface, 'pointerdown', 450, 600)
    pointer(surface, 'pointermove', 450, 300)
    pointer(surface, 'pointerup', 450, 300, 0)
    await tick()

    expect(stage.scrollTop).toBe(300)
    expect(at(screen)).toBe(2)
  })

  test('a sideways drag still turns the page', async () => {
    settings({ layout: 'page', pageFit: 'width' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    const stage = screen.container.querySelector('.stage') as HTMLElement
    widen(surface)

    pointer(surface, 'pointerdown', 700, 300)
    pointer(surface, 'pointermove', 500, 320)
    pointer(surface, 'pointerup', 500, 320, 0)
    await tick()

    expect(stage.scrollTop).toBe(0)
    expect(at(screen)).toBe(3)
  })
})

describe('zoom', () => {
  test('double click magnifies and turning the page lets it go', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    const stage = screen.container.querySelector('.stage') as HTMLElement

    surface.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()
    expect(stage.style.transform).toContain('scale(2)')

    await press('ArrowRight')
    expect((screen.container.querySelector('.stage') as HTMLElement).style.transform).toBe('')
  })

  test('a zoomed page pans instead of turning', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages, start: 2 })
    await tick()
    const surface = screen.container.querySelector('.surface')!
    widen(surface)

    surface.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()

    pointer(surface, 'pointerdown', 800)
    pointer(surface, 'pointermove', 700, 250)
    await tick()
    const stage = screen.container.querySelector('.stage') as HTMLElement
    expect(stage.style.transform).toContain('translate(-100px, -50px)')

    pointer(surface, 'pointerup', 700, 250, 0)
    await tick()
    expect(at(screen)).toBe(2)
  })
})

describe('full screen', () => {
  test('the corner button asks for it, and asks to leave again', async () => {
    settings({ layout: 'page' })
    const screen = render(Harness, { pages })
    await tick()
    const corner = screen.container.querySelector('.corner.full') as HTMLButtonElement
    const surface = screen.container.querySelector('.surface')!

    corner.click()
    await tick()
    expect(surface.requestFullscreen).toHaveBeenCalledOnce()

    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: surface })
    corner.click()
    await tick()
    expect(document.exitFullscreen).toHaveBeenCalledOnce()
    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: null })
  })

  test('an older browser is asked the older way', async () => {
    settings({ layout: 'page' })
    const asked = vi.fn(() => Promise.resolve())
    const standard = Element.prototype.requestFullscreen
    // @ts-expect-error - standing in for a browser that has only the prefix
    delete Element.prototype.requestFullscreen
    Object.defineProperty(Element.prototype, 'webkitRequestFullscreen', {
      configurable: true,
      value: asked,
    })

    const screen = render(Harness, { pages })
    await tick()
    ;(screen.container.querySelector('.corner.full') as HTMLButtonElement).click()
    await tick()

    expect(asked).toHaveBeenCalledOnce()
    Element.prototype.requestFullscreen = standard
    // @ts-expect-error - and putting the browser back the way it was
    delete Element.prototype.webkitRequestFullscreen
  })

  test('escape leaves full screen alone and does not go back', async () => {
    settings({ layout: 'page' })
    const onback = vi.fn()
    render(Harness, { pages, onback })
    await tick()

    const surface = document.querySelector('.surface')!
    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: surface })
    await press('Escape')
    expect(onback).not.toHaveBeenCalled()
    Object.defineProperty(document, 'fullscreenElement', { configurable: true, value: null })
  })
})

describe('scrolling', () => {
  test('the page in the middle of the screen is the page being read', async () => {
    const screen = render(Harness, { pages })
    await tick()
    const images = [...screen.container.querySelectorAll('img')]
    const scrolled = vi.mocked(Element.prototype.scrollIntoView)
    scrolled.mockClear()

    observers[0].show([images[3], images[4]])
    await tick()

    expect(at(screen)).toBe(3)
    // Following the reader must not also shove the reader around.
    expect(scrolled).not.toHaveBeenCalled()
  })

  test('opening a work leaves the window where it is', async () => {
    render(Harness, { pages })
    await tick()
    expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled()
  })

  test('opening on a page jumps to it', async () => {
    render(Harness, { pages, start: 3 })
    await tick()
    expect(Element.prototype.scrollIntoView).toHaveBeenCalledOnce()
  })

  test('a page chosen from outside is brought into view', async () => {
    const screen = render(Harness, { pages })
    await tick()
    const scrolled = vi.mocked(Element.prototype.scrollIntoView)
    scrolled.mockClear()

    screen.component.goto(4)
    await tick()

    expect(at(screen)).toBe(4)
    expect(scrolled).toHaveBeenCalledOnce()
  })
})
