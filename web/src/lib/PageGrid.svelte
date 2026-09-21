<script lang="ts">
  import type { Page } from './api'
  import { t } from './i18n.svelte'
  import { thumbnailOf } from './reader.svelte'

  /// Every page of a work at once. A slider is no way to find page 900 of
  /// 1,537; a wall of covers is.
  let {
    pages,
    current,
    onpick,
    onclose,
    inline = false,
  }: {
    pages: Page[]
    current: number
    onpick: (page: number) => void
    onclose?: () => void
    /// Shown in the flow of a page rather than over everything on it.
    inline?: boolean
  } = $props()

  let sheet = $state<HTMLElement | null>(null)

  // Opening a thousand pages at the top of the work would hide the one being
  // read somewhere below the fold.
  $effect(() => {
    if (!inline) sheet?.querySelector('.page.on')?.scrollIntoView({ block: 'center' })
  })

  // Over the reader this is a dialogue: the keyboard belongs to it while it is
  // open, everything behind it is out of reach, and whatever was focused when
  // it opened gets the focus back when it closes.
  $effect(() => {
    if (inline || !onclose) return
    const opener = document.activeElement as HTMLElement | null
    const behind = [...document.body.children].filter((el) => !el.contains(sheet))
    for (const el of behind) el.setAttribute('inert', '')

    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      onclose()
    }
    addEventListener('keydown', onKey, { capture: true })

    sheet?.querySelector<HTMLElement>('.page.on, .page')?.focus({ preventScroll: true })

    return () => {
      removeEventListener('keydown', onKey, { capture: true })
      for (const el of behind) el.removeAttribute('inert')
      opener?.focus?.()
    }
  })
</script>

<div
  class="sheet"
  class:inline
  role={inline ? undefined : 'dialog'}
  aria-modal={inline ? undefined : 'true'}
  aria-label={t('reader.pages')}
  bind:this={sheet}
>
  {#if !inline}
    <header>
      <strong>{t('reader.pages')}</strong>
      <span class="muted">{current + 1} / {pages.length}</span>
      <button onclick={onclose}>{t('reader.pagesClose')}</button>
    </header>
  {/if}

  <div class="wall">
    {#each pages as page, at (at)}
      <button
        class="page"
        class:on={at === current}
        onclick={() => onpick(at)}
        aria-current={at === current ? 'true' : undefined}
      >
        <img
          src={thumbnailOf(page.src)}
          alt=""
          loading="lazy"
          decoding="async"
        />
        <span>{at + 1}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .sheet {
    position: fixed;
    inset: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overscroll-behavior: contain;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    /* Covers the whole window, so it meets the clock strip too. */
    padding: calc(0.7rem + var(--safe-top)) 1rem 0.7rem;
    border-bottom: 1px solid var(--line);
  }
  header strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    margin-inline-end: auto;
  }

  .sheet.inline {
    position: static;
    background: none;
  }
  /* In the page rather than over it, so the clock strip is not its problem. */
  .sheet.inline header {
    padding-top: 0.7rem;
  }
  .sheet.inline .wall {
    overflow: visible;
    padding: 0;
  }

  .wall {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(88px, 1fr));
    gap: 0.5rem;
    padding: 0.75rem 1rem calc(1rem + env(safe-area-inset-bottom));
  }

  .page {
    display: grid;
    gap: 0.15rem;
    padding: 0;
    background: none;
    border: none;
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    /* A wall of a thousand pages only paints the part being looked at. */
    content-visibility: auto;
    contain-intrinsic-size: auto 140px;
  }

  .page img {
    width: 100%;
    aspect-ratio: 3 / 4;
    object-fit: cover;
    background: var(--surface);
    border: 1px solid var(--image-edge);
    border-radius: var(--radius);
  }

  .page.on {
    color: var(--accent);
  }
  .page.on img {
    border-color: var(--accent);
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .page:hover img {
    border-color: var(--accent);
  }

  @media (max-width: 640px) {
    .wall {
      grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
      padding: 0.6rem 0.75rem calc(1rem + env(safe-area-inset-bottom));
    }
  }
</style>
