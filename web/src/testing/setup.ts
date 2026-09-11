/// What jsdom does not implement but the reader leans on. Each stub is the
/// smallest thing that lets the component run; behaviour worth asserting is
/// asserted in the tests themselves, not here.

import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/svelte'
import { afterEach, vi } from 'vitest'

// Each test renders into the same document; what the last one left behind
// would otherwise be found by the next one's queries.
afterEach(cleanup)

class Observer implements IntersectionObserver {
  readonly root = null
  readonly rootMargin = ''
  readonly thresholds: readonly number[] = []
  private readonly targets = new Set<Element>()

  constructor(private readonly callback: IntersectionObserverCallback) {
    observers.push(this)
  }

  observe(target: Element) {
    this.targets.add(target)
  }
  unobserve(target: Element) {
    this.targets.delete(target)
  }
  disconnect() {
    this.targets.clear()
    const at = observers.indexOf(this)
    if (at >= 0) observers.splice(at, 1)
  }
  takeRecords(): IntersectionObserverEntry[] {
    return []
  }

  /// Reports the given elements as the ones on screen, the way scrolling would.
  show(elements: Element[]) {
    const entries = [...this.targets].map((target) => ({
      target,
      isIntersecting: elements.includes(target),
    })) as IntersectionObserverEntry[]
    this.callback(entries, this)
  }
}

export const observers: Observer[] = []

vi.stubGlobal('IntersectionObserver', Observer)

// jsdom has no decoder, and no layout, so images never load and never measure.
Object.defineProperty(HTMLImageElement.prototype, 'decode', {
  configurable: true,
  value: () => Promise.resolve(),
})

Element.prototype.scrollIntoView = vi.fn()
Element.prototype.requestFullscreen = vi.fn(() => Promise.resolve())
Object.defineProperty(document, 'exitFullscreen', {
  configurable: true,
  value: vi.fn(() => Promise.resolve()),
})
