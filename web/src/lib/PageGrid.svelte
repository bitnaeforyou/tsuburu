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

  $effect(() => {
    if (inline || !onclose) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.preventDefault()
      onclose()
    }
    addEventListener('keydown', onKey, { capture: true })
    return () => removeEventListener('keydown', onKey, { capture: true })
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
    {#each pages as page, at (page.src)}
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
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.7rem 1rem;
    border-bottom: 1px solid var(--border);
  }
  header strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    margin-right: auto;
  }

  .sheet.inline {
    position: static;
    background: none;
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
    font-size: 0.72rem;
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
    border: 1px solid var(--border);
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
