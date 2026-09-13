<script lang="ts">
  import type { Page } from './api'
  import { t } from './i18n.svelte'
  import {
    fitOf,
    forwardFor,
    forwardForTap,
    reader,
    spreadOf,
    spreads,
    step,
    toPrefetch,
  } from './reader.svelte'

  /// The viewing surface. It knows how to lay pages out and where a key, a tap
  /// or a swipe lands; which work this is, and what to remember about it, is
  /// the caller's business.
  let {
    pages,
    current = $bindable(0),
    onback,
    onchrome,
    onpages,
  }: {
    pages: Page[]
    current: number
    onback?: () => void
    /// The middle of the page was tapped, which is how a reader asks for
    /// everything around the pages to get out of the way, and back again.
    onchrome?: () => void
    /// Asked for the wall of pages. The corner counter is the only way to it
    /// once everything else has been cleared away.
    onpages?: () => void
  } = $props()

  /// Pages to have decoded ahead of the reader, so a turn never shows white.
  const PREFETCH = 3
  /// Shorter than this along the page is a tap, not a turn.
  const SWIPE = 60
  /// A pointer that wandered further than this was not aiming at a tap zone.
  const SLOP = 10

  const settings = $derived(reader.settings)
  const fit = $derived(fitOf(settings))
  const paged = $derived(settings.layout !== 'scroll')
  const groups = $derived(spreads(pages.length, settings.coverAlone))
  /// Where the reader is, as a page this work actually has: a number left over
  /// from a longer work must not be rendered as a page that is not there.
  const where = $derived(Math.min(Math.max(current, 0), Math.max(pages.length - 1, 0)))
  const showing = $derived(
    pages.length === 0
      ? []
      : settings.layout === 'spread'
        ? (groups[spreadOf(where, groups)] ?? [where])
        : [where],
  )
  /// Filling the width or showing pixels one for one means the page is taller
  /// than the frame; it has to be scrollable or the bottom is unreachable.
  const scrollable = $derived(paged && (fit === 'width' || fit === 'original'))

  let surface = $state<HTMLElement | null>(null)
  let stage = $state<HTMLElement | null>(null)
  let elements = $state<(HTMLImageElement | null)[]>([])
  let zoom = $state(1)
  let pan = $state({ x: 0, y: 0 })
  let fullscreen = $state(false)

  /// Every finger or button currently down on the page. One is a tap, a swipe
  /// or a drag; two are a pinch.
  const touching = new Map<number, { x: number; y: number }>()
  /// The single-pointer gesture in progress, before it is known which it is.
  let press: { x: number; y: number; at: number; top: number } | null = null
  /// Dragging a magnified page around.
  let dragging: { x: number; y: number; panX: number; panY: number } | null = null
  /// What the two fingers were doing when they landed.
  let pinching: {
    span: number
    zoom: number
    x: number
    y: number
    panX: number
    panY: number
  } | null = null
  /// Set when the observer moved `current`, so the effect that follows the
  /// page does not scroll the reader back to where it already is.
  let followed = false
  /// The page the reader was last put in front of, so a rerun for some other
  /// reason does not count as a move. -1 until the first one.
  let shown = -1

  // In scrolling layout the page in the middle of the screen is the page being
  // read, and scrolling is how you turn.
  $effect(() => {
    if (paged || !pages.length) return
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .map((entry) => Number((entry.target as HTMLElement).dataset.page))
        if (!visible.length) return
        const seen = Math.min(...visible)
        if (seen === current) return
        followed = true
        current = seen
      },
      { rootMargin: '-45% 0px -45% 0px' },
    )
    for (const element of elements) if (element) observer.observe(element)
    return () => observer.disconnect()
  })

  // A page set from outside - resumed, scrubbed, jumped to from a dialogue hit
  // - has to be brought into view; one the observer reported already is.
  $effect(() => {
    const page = current
    // Paged reading scrolls nothing, but leaving `shown` behind is what makes
    // switching back to scrolling land on the page that was being read.
    if (paged) return
    if (shown === page) return
    const arriving = shown === -1
    shown = page
    if (followed) {
      followed = false
      return
    }
    // Arriving at the first page means the top of the whole thing. Bringing
    // the image itself into view would push the controls off the top; coming
    // from the screen before would leave it wherever that was scrolled to.
    if (arriving && page === 0) {
      scrollTo(0, 0)
      return
    }
    elements[page]?.scrollIntoView({ block: 'start' })
  })

  // Decoding ahead, not just fetching: a decode on the turn is what stutters.
  $effect(() => {
    for (const at of toPrefetch(where, pages.length, PREFETCH)) {
      const image = new Image()
      image.src = pages[at].src
      void image.decode().catch(() => {})
    }
  })

  // Turning the page drops the zoom. Carrying it over lands you on a corner of
  // the next page you never chose to look at.
  $effect(() => {
    void current
    void settings.layout
    zoom = 1
    pan = { x: 0, y: 0 }
    if (stage) stage.scrollTop = 0
  })

  // Safari only grew the unprefixed calls in 16.4, and this is the one feature
  // where the prefixed pair is the difference between working and not.
  type Prefixed = HTMLElement & { webkitRequestFullscreen?: () => Promise<void> }
  type PrefixedDocument = Document & {
    webkitFullscreenElement?: Element | null
    webkitExitFullscreen?: () => Promise<void>
  }

  function shownFull(): Element | null {
    return document.fullscreenElement ?? (document as PrefixedDocument).webkitFullscreenElement ?? null
  }

  $effect(() => {
    const onChange = () => (fullscreen = shownFull() !== null)
    document.addEventListener('fullscreenchange', onChange)
    document.addEventListener('webkitfullscreenchange', onChange)
    return () => {
      document.removeEventListener('fullscreenchange', onChange)
      document.removeEventListener('webkitfullscreenchange', onChange)
    }
  })

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return
      const target = event.target as HTMLElement | null
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) {
        return
      }
      if (event.key === 'Escape') {
        if (!shownFull()) onback?.()
        return
      }
      if (event.key === 'f' || event.key === 'F') {
        event.preventDefault()
        void toggleFullscreen()
        return
      }
      const forward = forwardFor(event.key, settings.direction)
      if (forward === null) return
      // Space keeps scrolling in the scrolling layout, the way it does
      // everywhere else; the arrows are what turn pages there.
      if (!paged && event.key === ' ') return
      event.preventDefault()
      move(forward)
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  })

  function move(forward: boolean) {
    current = step(current, forward, pages.length, settings.layout, settings.coverAlone)
  }

  /// The controls sitting on top of the page are not part of the page: a
  /// press that lands on one must not also turn it.
  function onChrome(event: Event): boolean {
    return (event.target as HTMLElement | null)?.closest('button') !== null
  }

  /// The distance between the two fingers, and the point between them.
  function spread(): { span: number; x: number; y: number } {
    const [a, b] = [...touching.values()]
    return {
      span: Math.max(Math.hypot(a.x - b.x, a.y - b.y), 1),
      x: (a.x + b.x) / 2,
      y: (a.y + b.y) / 2,
    }
  }

  function onPointerDown(event: PointerEvent) {
    if (!paged || (event.pointerType === 'mouse' && event.button !== 0) || onChrome(event)) return
    // Capture keeps a finger that wanders off the page still ours. A browser
    // that will not give it is no reason to stop reading the gesture.
    try {
      surface?.setPointerCapture(event.pointerId)
    } catch {
      // Events still arrive while the pointer is over the page.
    }
    touching.set(event.pointerId, { x: event.clientX, y: event.clientY })

    if (touching.size === 1) {
      press = { x: event.clientX, y: event.clientY, at: Date.now(), top: stage?.scrollTop ?? 0 }
      dragging =
        zoom > 1 ? { x: event.clientX, y: event.clientY, panX: pan.x, panY: pan.y } : null
      return
    }
    if (touching.size === 2) {
      // Two fingers is never a page turn, whatever the first one was doing.
      press = null
      dragging = null
      const now = spread()
      pinching = { span: now.span, zoom, x: now.x, y: now.y, panX: pan.x, panY: pan.y }
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (!touching.has(event.pointerId)) return
    touching.set(event.pointerId, { x: event.clientX, y: event.clientY })

    if (pinching && touching.size >= 2) {
      const now = spread()
      zoom = Math.min(Math.max((pinching.zoom * now.span) / pinching.span, 1), 4)
      pan =
        zoom === 1
          ? { x: 0, y: 0 }
          : { x: pinching.panX + (now.x - pinching.x), y: pinching.panY + (now.y - pinching.y) }
      return
    }

    if (dragging) {
      pan = {
        x: dragging.panX + (event.clientX - dragging.x),
        y: dragging.panY + (event.clientY - dragging.y),
      }
      return
    }

    // A page taller than the frame is moved by the finger that is on it: the
    // surface takes every touch, so nothing scrolls it otherwise.
    if (press && stage && scrollable) {
      const dx = event.clientX - press.x
      const dy = event.clientY - press.y
      if (Math.abs(dy) > Math.abs(dx)) stage.scrollTop = press.top - dy
    }
  }

  function onPointerUp(event: PointerEvent) {
    touching.delete(event.pointerId)
    try {
      surface?.releasePointerCapture(event.pointerId)
    } catch {
      // It was never captured, which is the state we wanted anyway.
    }

    if (pinching) {
      // A pinch ends when it stops being a pinch; the finger still down is
      // not the start of anything.
      if (touching.size < 2) {
        pinching = null
        press = null
        dragging = null
      }
      return
    }
    if (dragging) {
      dragging = null
      press = null
      return
    }

    const start = press
    press = null
    if (!start || !paged || zoom > 1) return

    const dx = event.clientX - start.x
    const dy = event.clientY - start.y
    // A swipe carries its own direction; a tap takes it from where it landed.
    if (Math.abs(dx) > SWIPE && Math.abs(dx) > Math.abs(dy)) {
      move(dx < 0 === (settings.direction === 'ltr'))
      return
    }
    if (Math.abs(dx) > SLOP || Math.abs(dy) > SLOP || Date.now() - start.at > 400) return
    const box = surface?.getBoundingClientRect()
    if (!box) return
    const forward = forwardForTap(event.clientX - box.left, box.width, settings.direction)
    if (forward === null) onchrome?.()
    else move(forward)
  }

  function onPointerCancel(event: PointerEvent) {
    touching.delete(event.pointerId)
    press = null
    dragging = null
    if (touching.size < 2) pinching = null
  }

  function onDoubleClick(event: MouseEvent) {
    if (!paged || onChrome(event)) return
    event.preventDefault()
    zoom = zoom > 1 ? 1 : 2
    pan = { x: 0, y: 0 }
  }

  function onWheel(event: WheelEvent) {
    if (!paged || !event.ctrlKey) return
    event.preventDefault()
    zoom = Math.min(Math.max(zoom - event.deltaY / 400, 1), 4)
    if (zoom === 1) pan = { x: 0, y: 0 }
  }

  async function toggleFullscreen() {
    const doc = document as PrefixedDocument
    const target = surface as Prefixed | null
    try {
      if (shownFull()) await (doc.exitFullscreen?.() ?? doc.webkitExitFullscreen?.())
      else await (target?.requestFullscreen?.() ?? target?.webkitRequestFullscreen?.())
    } catch {
      // Embedded browsers refuse it outright. The reader works either way, so
      // there is nothing worth telling the reader about.
    }
  }
</script>

{#if paged}
  <div
    class="surface"
    class:zoomed={zoom > 1}
    role="group"
    aria-label={t('reader.surface')}
    bind:this={surface}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerCancel}
    ondblclick={onDoubleClick}
    onwheel={onWheel}
  >
    <div
      class="stage {fit}"
      class:rtl={settings.direction === 'rtl'}
      class:scrollable
      bind:this={stage}
      style:transform={zoom === 1 ? null : `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`}
    >
      {#each showing as at (at)}
        <img
          src={pages[at].src}
          alt={t('common.page', { n: at + 1 })}
          width={pages[at].width}
          height={pages[at].height}
          draggable="false"
          decoding="async"
          fetchpriority="high"
        />
      {/each}
    </div>

    {#if onpages}
      <button class="corner where" onclick={onpages} title={t('reader.pages')}>
        {where + 1} / {pages.length}
      </button>
    {:else}
      <span class="corner where">{where + 1} / {pages.length}</span>
    {/if}

    <button class="corner full" onclick={toggleFullscreen} title={t('reader.fullscreen')}>
      {fullscreen ? '⤡' : '⤢'}
    </button>
  </div>
{:else}
  <div class="flow">
    {#each pages as page, at (at)}
      <img
        bind:this={elements[at]}
        data-page={at}
        class={fit}
        src={page.src}
        alt={t('common.page', { n: at + 1 })}
        width={page.width}
        height={page.height}
        loading={at <= PREFETCH ? 'eager' : 'lazy'}
        decoding="async"
      />
    {/each}
  </div>
{/if}

<style>
  /* Paged reading owns the viewport: one turn, one screenful, and no page of
     the surrounding document creeping in under the image. */
  .surface {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    /* The caller lays the reading screen out as a column that fills the
       window; the pages take whatever the controls above them left over. */
    flex: 1 1 0;
    min-height: 12rem;
    overflow: hidden;
    background: var(--surface);
    border-radius: var(--radius);
    /* Every touch belongs to the reader: one finger turns, swipes or moves a
       tall page, two pinch. Leaving any of it to the browser would turn one
       of those into a scroll or a zoom of the whole page instead. */
    touch-action: none;
    user-select: none;
  }
  /* Two rules, not one: a browser that does not know one of these pseudo
     classes throws the whole rule away. */
  .surface:fullscreen {
    height: 100dvh;
    border-radius: 0;
    background: #000;
  }
  .surface:-webkit-full-screen {
    height: 100dvh;
    border-radius: 0;
    background: #000;
  }
  .surface.zoomed {
    cursor: grab;
  }

  .stage {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    transform-origin: center center;
  }
  /* A page that slides under the finger is the turn; someone who has asked
     for less motion gets the same turn, arrived at. */
  @media (prefers-reduced-motion: no-preference) {
    .stage {
      transition: transform 120ms ease-out;
    }
  }
  .stage.rtl {
    flex-direction: row-reverse;
  }
  .stage.scrollable {
    align-items: flex-start;
    overflow: auto;
    scrollbar-width: thin;
  }

  .stage img {
    display: block;
    min-width: 0;
    min-height: 0;
    background: var(--surface);
  }
  .stage.width img {
    width: 100%;
    height: auto;
  }
  .stage.height img,
  .stage.contain img {
    max-width: 100%;
    max-height: 100%;
    width: auto;
    height: auto;
    object-fit: contain;
  }
  .stage.original img {
    width: auto;
    height: auto;
    max-width: none;
    max-height: none;
  }

  .flow {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }
  .flow img {
    display: block;
    background: var(--surface);
    /* Pages off screen keep their space but are not rendered. */
    content-visibility: auto;
    contain-intrinsic-size: auto 1200px;
  }
  .flow img.width {
    width: min(100%, 1000px);
    height: auto;
  }
  .flow img.height {
    max-height: 100dvh;
    width: auto;
    max-width: 100%;
  }
  .flow img.contain {
    max-width: min(100%, 1000px);
    max-height: 100dvh;
    width: auto;
  }
  .flow img.original {
    width: auto;
    max-width: none;
  }

  .corner {
    position: absolute;
    bottom: 0.6rem;
    opacity: 0.4;
    font-size: var(--text-sm);
    line-height: 1;
    padding: 0.3rem 0.5rem;
  }
  .corner:hover,
  .corner:focus-visible {
    opacity: 1;
  }
  .where {
    inset-inline-start: 0.6rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  span.where {
    pointer-events: none;
  }
  button.where {
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border-color: transparent;
  }
  .full {
    inset-inline-end: 0.6rem;
    font-size: var(--text-md);
  }
</style>
