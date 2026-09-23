/// Puts a screen back where it was left.
///
/// The browser restores a scroll position on its own, but only for pages it
/// laid out itself: here the list arrives after the screen does, so by the
/// time there is anything to scroll to, the restoring is over. Pressing back
/// out of a work therefore landed at the top of a list the reader had spent a
/// while getting down.

const STORED = 'tsuburu.scroll'

function all(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(STORED) ?? '{}') as Record<string, number>
  } catch {
    return {}
  }
}

function save(rows: Record<string, number>) {
  try {
    localStorage.setItem(STORED, JSON.stringify(rows))
  } catch {
    // A browser with storage switched off still gets to scroll.
  }
}

/// Records where `key` is left, and puts it back when `ready` says there is
/// something to put it back to. Returns a teardown.
export function keepScroll(key: string, ready: () => boolean) {
  const want = all()[key] ?? 0
  let settled = false
  const give = () => (settled = true)

  let timer: ReturnType<typeof setTimeout> | undefined
  const onScroll = () => {
    clearTimeout(timer)
    timer = setTimeout(() => {
      const rows = all()
      rows[key] = window.scrollY
      save(rows)
    }, 300)
  }
  addEventListener('scroll', onScroll, { passive: true })

  const tries: ReturnType<typeof setTimeout>[] = []
  if (want > 0) {
    for (const ev of ['wheel', 'touchstart', 'keydown'] as const) {
      addEventListener(ev, give, { passive: true, once: true })
    }
    // The covers arrive after the cards they belong to and each one moves
    // everything below it, so this keeps asking while the page settles.
    for (const after of [0, 50, 150, 300, 500, 800, 1200]) {
      tries.push(
        setTimeout(() => {
          if (settled || !ready() || Math.abs(window.scrollY - want) <= 8) return
          scrollTo(0, want)
        }, after),
      )
    }
  }

  return () => {
    clearTimeout(timer)
    tries.forEach(clearTimeout)
    removeEventListener('scroll', onScroll)
    for (const ev of ['wheel', 'touchstart', 'keydown'] as const) {
      removeEventListener(ev, give)
    }
  }
}
