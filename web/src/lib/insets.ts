/// What the phone draws over the page.
///
/// `env(safe-area-inset-*)` is the browser's own answer and is right
/// everywhere but one place: Android WebView measures the bottom one as zero
/// however the gesture bar is drawn, so a bar pinned to the bottom of the
/// window ends up underneath it. The phone app knows the real numbers and
/// offers them on `window.tsuburuInsets`; this copies them into the
/// variables the stylesheet reads, where the larger of the two wins.
///
/// Read here when the page loads, which covers the numbers arriving before
/// it did; the app sets them itself whenever they change, which covers the
/// ones that arrive after. Either half alone loses a race, and losing it at
/// the top of the window puts the first row of the page under the status
/// bar.

type Native = {
  top(): number
  bottom(): number
  left(): number
  right(): number
}

const SIDES = ['top', 'bottom', 'left', 'right'] as const

function native(): Native | null {
  const held = (window as unknown as { tsuburuInsets?: Native }).tsuburuInsets
  return held && typeof held.bottom === 'function' ? held : null
}

function apply(from: Native) {
  const style = document.documentElement.style
  for (const side of SIDES) {
    const px = Number(from[side]())
    if (Number.isFinite(px)) style.setProperty(`--inset-${side}`, `${px}px`)
  }
}

/// Reads them now and again whenever they can have changed: rotating the
/// phone moves them, and so does switching between gestures and buttons.
export function followTheSystemBars() {
  const from = native()
  if (!from) return
  apply(from)
  // The window is resized after the change lands, so the numbers are read
  // on the frame after it rather than during.
  const again = () => requestAnimationFrame(() => apply(from))
  window.addEventListener('resize', again)
  window.addEventListener('orientationchange', again)
}
