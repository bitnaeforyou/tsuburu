/// How a work is laid out while it is read, and where a keypress or a tap
/// takes you. The choices live here rather than in the component so that the
/// parts worth being sure about - which pages share a spread, which way a
/// key moves - can be tested without a browser.

export type Layout = 'scroll' | 'page' | 'spread'
export type Direction = 'ltr' | 'rtl'
export type Fit = 'width' | 'height' | 'contain' | 'original'

export type ReaderSettings = {
  layout: Layout
  /// Manga is drawn right to left; everything else is not. This flips the
  /// keys, the taps and the order pages sit in a spread.
  direction: Direction
  /// Scrolling wants the width filled; a page turn wants the whole page on
  /// screen. The answer differs, so the two are remembered apart.
  fit: Fit
  pageFit: Fit
  /// A cover is a single page, so a spread that starts at page 0 would pair
  /// it with page 1 and put every later pair on the wrong side.
  coverAlone: boolean
}

export const DEFAULTS: ReaderSettings = {
  layout: 'scroll',
  direction: 'ltr',
  fit: 'width',
  pageFit: 'contain',
  coverAlone: true,
}

const STORED = 'tsuburu.reader'

function load(): ReaderSettings {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return { ...DEFAULTS }
    return sanitise({ ...DEFAULTS, ...JSON.parse(raw) })
  } catch {
    return { ...DEFAULTS }
  }
}

/// A stored setting from an older version, or a hand-edited one, must not be
/// able to put the reader in a state it cannot render.
export function sanitise(value: Partial<ReaderSettings>): ReaderSettings {
  const layouts: Layout[] = ['scroll', 'page', 'spread']
  const fits: Fit[] = ['width', 'height', 'contain', 'original']
  const fit = (given: Fit | undefined, fallback: Fit) =>
    fits.includes(given as Fit) ? (given as Fit) : fallback
  return {
    layout: layouts.includes(value.layout as Layout) ? (value.layout as Layout) : DEFAULTS.layout,
    direction: value.direction === 'rtl' ? 'rtl' : 'ltr',
    fit: fit(value.fit, DEFAULTS.fit),
    pageFit: fit(value.pageFit, DEFAULTS.pageFit),
    coverAlone: value.coverAlone !== false,
  }
}

/// Which of the two fits is in force, and which one a change should be
/// written to.
export function fitKey(layout: Layout): 'fit' | 'pageFit' {
  return layout === 'scroll' ? 'fit' : 'pageFit'
}

export function fitOf(settings: ReaderSettings): Fit {
  return settings[fitKey(settings.layout)]
}

class Reader {
  settings = $state<ReaderSettings>(load())

  set<K extends keyof ReaderSettings>(key: K, value: ReaderSettings[K]) {
    this.settings = { ...this.settings, [key]: value }
    try {
      localStorage.setItem(STORED, JSON.stringify(this.settings))
    } catch {
      // Private windows refuse storage; the choice lasts for this session.
    }
  }
}

export const reader = new Reader()

/// Groups page indices the way they are shown side by side.
///
/// With `coverAlone` the first page stands by itself, which is what puts the
/// rest on the sides they were drawn for.
export function spreads(count: number, coverAlone: boolean): number[][] {
  if (count <= 0) return []
  const out: number[][] = []
  let at = 0
  if (coverAlone) {
    out.push([0])
    at = 1
  }
  for (; at < count; at += 2) {
    out.push(at + 1 < count ? [at, at + 1] : [at])
  }
  return out
}

/// Which spread a page is in.
export function spreadOf(page: number, groups: number[][]): number {
  const at = groups.findIndex((group) => group.includes(page))
  return at < 0 ? 0 : at
}

/// The page a step lands on, in reading order.
///
/// `forward` means "further into the work" whichever way it is read; the
/// caller decides that from the key or the side of the screen that was hit.
export function step(
  page: number,
  forward: boolean,
  count: number,
  layout: Layout,
  coverAlone: boolean,
): number {
  if (count <= 0) return 0
  if (layout !== 'spread') {
    return clamp(page + (forward ? 1 : -1), count)
  }
  const groups = spreads(count, coverAlone)
  const at = spreadOf(page, groups)
  const next = groups[clamp(at + (forward ? 1 : -1), groups.length)]
  return next?.[0] ?? page
}

/// Whether a key means "further into the work".
///
/// Right-to-left swaps the arrows: in a manga, left is onward. Space and the
/// page keys always mean onward, the way they do in every reader.
export function forwardFor(key: string, direction: Direction): boolean | null {
  switch (key) {
    case 'ArrowRight':
      return direction === 'ltr'
    case 'ArrowLeft':
      return direction === 'rtl'
    case ' ':
    case 'PageDown':
      return true
    case 'PageUp':
      return false
    default:
      return null
  }
}

/// Whether a tap at `x` across a viewport `width` wide means onward.
///
/// The middle is neither: that is where the controls are reached, so a reader
/// aiming at them does not turn a page by accident.
export function forwardForTap(x: number, width: number, direction: Direction): boolean | null {
  if (width <= 0) return null
  const third = width / 3
  if (x < third) return direction === 'rtl'
  if (x > width - third) return direction === 'ltr'
  return null
}

/// The pages worth having decoded already.
///
/// Ahead of the reader, and one behind, because turning back is common and a
/// blank frame is as jarring in that direction.
export function toPrefetch(page: number, count: number, ahead: number): number[] {
  const out: number[] = []
  for (let i = page + 1; i <= page + ahead && i < count; i++) out.push(i)
  if (page > 0 && page - 1 < count) out.push(page - 1)
  return out
}

function clamp(value: number, count: number): number {
  return Math.min(Math.max(value, 0), Math.max(count - 1, 0))
}
