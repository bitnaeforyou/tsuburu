<script lang="ts">
  import { t } from './i18n.svelte'
  import { fitKey, fitOf, reader, step, type Fit, type Layout } from './reader.svelte'

  /// Everything the reader can be told to do, in one row: where you are, where
  /// to go, and how the pages should sit while you get there.
  let {
    count,
    current = $bindable(0),
    onpages,
  }: {
    count: number
    current: number
    /// Asked for the wall of pages. Which page comes back is the caller's.
    onpages?: () => void
  } = $props()

  const settings = $derived(reader.settings)
  const rtl = $derived(settings.direction === 'rtl')

  const LAYOUTS: { value: Layout; key: 'reader.scroll' | 'reader.paged' | 'reader.spread' }[] = [
    { value: 'scroll', key: 'reader.scroll' },
    { value: 'page', key: 'reader.paged' },
    { value: 'spread', key: 'reader.spread' },
  ]

  const FITS: { value: Fit; key: 'reader.fitWidth' | 'reader.fitHeight' | 'reader.fitContain' | 'reader.fitOriginal' }[] = [
    { value: 'width', key: 'reader.fitWidth' },
    { value: 'height', key: 'reader.fitHeight' },
    { value: 'contain', key: 'reader.fitContain' },
    { value: 'original', key: 'reader.fitOriginal' },
  ]

  function move(forward: boolean) {
    current = step(current, forward, count, settings.layout, settings.coverAlone)
  }
</script>

<div class="bar">
  <div class="turn" class:rtl>
    <button onclick={() => move(false)} disabled={current === 0}>
      {rtl ? '→' : '←'}
      {t('gallery.previous')}
    </button>
    <input
      class="scrub"
      type="range"
      min="0"
      max={Math.max(count - 1, 0)}
      value={current}
      dir={rtl ? 'rtl' : 'ltr'}
      aria-label={t('common.page', { n: current + 1 })}
      oninput={(event) => (current = Number(event.currentTarget.value))}
    />
    <button onclick={() => move(true)} disabled={current >= count - 1}>
      {t('gallery.next')}
      {rtl ? '←' : '→'}
    </button>
  </div>

  <div class="set">
    {#if onpages}
      <button class="wide" onclick={onpages}>{t('reader.pages')}</button>
    {/if}

    <div class="group" role="group" aria-label={t('reader.layout')}>
      {#each LAYOUTS as option (option.value)}
        <button
          class:on={settings.layout === option.value}
          aria-pressed={settings.layout === option.value}
          onclick={() => reader.set('layout', option.value)}
        >
          {t(option.key)}
        </button>
      {/each}
    </div>

    <button
      class="wide"
      onclick={() => reader.set('direction', rtl ? 'ltr' : 'rtl')}
      title={t('reader.direction')}
      aria-label={t('reader.direction')}
    >
      {rtl ? t('reader.rtl') : t('reader.ltr')}
    </button>

    <label class="pick">
      <span class="sr">{t('reader.fit')}</span>
      <select
        value={fitOf(settings)}
        onchange={(event) =>
          reader.set(fitKey(settings.layout), event.currentTarget.value as Fit)}
      >
        {#each FITS as option (option.value)}
          <option value={option.value}>{t(option.key)}</option>
        {/each}
      </select>
    </label>

    {#if settings.layout === 'spread'}
      <label class="check">
        <input
          type="checkbox"
          checked={settings.coverAlone}
          onchange={(event) => reader.set('coverAlone', event.currentTarget.checked)}
        />
        {t('reader.coverAlone')}
      </label>
    {/if}
  </div>
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem 1rem;
    /* Scrolling reading is all scroll, so the controls have to come with you;
       `--chrome` is how tall the header above them is. */
    position: sticky;
    top: var(--chrome, 0);
    z-index: 1;
    background: var(--bg);
    padding: 0.5rem 0;
    margin-bottom: 0.1rem;
  }

  .turn {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1 1 18rem;
    /* On a wide screen a scrubber the width of the window puts the two
       buttons an arm apart. */
    max-width: 34rem;
    min-width: 0;
  }
  .turn.rtl {
    flex-direction: row-reverse;
  }
  .turn button {
    white-space: nowrap;
    font-size: var(--text-sm);
    padding: 0.25rem 0.6rem;
  }

  .scrub {
    flex: 1;
    min-width: 4rem;
    accent-color: var(--accent);
  }

  .set {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }

  .group {
    display: flex;
  }
  .group button {
    font-size: var(--text-sm);
    padding: 0.25rem 0.6rem;
    border-radius: 0;
    margin-inline-start: -1px;
  }
  .group button:first-child {
    border-start-start-radius: var(--radius);
    border-end-start-radius: var(--radius);
    margin-inline-start: 0;
  }
  .group button:last-child {
    border-start-end-radius: var(--radius);
    border-end-end-radius: var(--radius);
  }
  .group button.on {
    color: var(--accent);
    border-color: var(--accent);
    z-index: 1;
  }

  .wide,
  select {
    font-size: var(--text-sm);
    padding: 0.25rem 0.6rem;
  }
  select {
    font-family: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--edge);
    border-radius: var(--radius);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  /* Everything here is pressed with a thumb on a phone, so it is bigger, and
     the scrubber gets the whole width rather than sharing it. */
  @media (max-width: 640px) {
    .bar {
      gap: 0.45rem 0.6rem;
    }
    .turn {
      flex: 1 1 100%;
      max-width: none;
    }
    .turn button,
    .group button,
    .wide {
      font-size: var(--text-md);
      padding: 0.45rem 0.7rem;
    }
    /* Anything below 16px zooms the page when iOS opens it. */
    select {
      font-size: var(--text-base);
      padding: 0.45rem 0.7rem;
    }
    .scrub {
      height: 1.75rem;
    }
    .check {
      font-size: var(--text-md);
    }
  }
</style>
