<script lang="ts">
  import * as api from '../lib/api'
  import { t } from '../lib/i18n.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { library } from '../lib/library.svelte'
  import { toArtist, toKeyword, toSearch } from '../lib/router'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import LocalePicker from '../lib/LocalePicker.svelte'
  import ReaderView from '../lib/ReaderView.svelte'
  import ReaderBar from '../lib/ReaderBar.svelte'
  import { reader } from '../lib/reader.svelte'

  let { id, startPage = null }: { id: number; startPage?: number | null } = $props()

  /** 진행 상황을 쓰기 전에 기다리는 시간. 페이지마다 쓰면 디스크가 시끄럽다. */
  const SAVE_DELAY = 1000

  let gallery = $state<api.Gallery | null>(null)
  let error = $state<unknown>(null)
  let current = $state(0)
  let resumeAt = $state<number | null>(null)
  let keywords = $state<api.Keyword[]>([])
  let near = $state<api.NearWork[]>([])
  let download = $state<api.DownloadItem | null>(null)
  let downloadError = $state<string | null>(null)

  const favorited = $derived(library.has(id))
  const paged = $derived(reader.settings.layout !== 'scroll')

  $effect(() => {
    void load()
    void library.load().catch(() => {})
  })

  // The bar has to keep moving while pages arrive, and stop when they stop.
  $effect(() => {
    if (!gallery) return
    void refreshDownload()
    const timer = setInterval(() => {
      if (download?.job.running) void refreshDownload()
    }, 1500)
    return () => clearInterval(timer)
  })

  async function refreshDownload() {
    try {
      download = await api.downloadStatus(id)
    } catch {
      // Never downloaded: not an error, just nothing to show.
      download = null
    }
  }

  async function keepWork() {
    downloadError = null
    try {
      await api.startDownload(id)
      await refreshDownload()
    } catch (cause) {
      downloadError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function keepPage(page: number) {
    downloadError = null
    try {
      await api.startDownload(id, [page])
      await refreshDownload()
    } catch (cause) {
      downloadError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  // Keywords come from an optional import; their absence is not an error.
  $effect(() => {
    void id
    keywords = []
    near = []
    void api
      .keywords(id)
      .then((found) => (keywords = found.words.slice(0, 12)))
      .catch(() => {})
    void api
      .nearWorks(id, 12)
      .then((found) => (near = found))
      .catch(() => {})
  })

  async function load() {
    error = null
    try {
      gallery = await api.gallery(id)
    } catch (cause) {
      error = cause
      return
    }
    // A dialogue hit links straight to its page.
    if (startPage !== null) current = clamp(startPage)
    // 마지막으로 본 위치를 알린다. 자동으로 뛰지는 않는다. 처음부터 보려는
    // 경우를 빼앗지 않기 위해서다.
    try {
      const { items } = await api.history()
      const previous = items.find((item) => item.id === id)
      if (previous && previous.last_page > 0 && startPage === null) resumeAt = previous.last_page
    } catch {
      // 기록을 못 읽어도 읽기는 계속된다.
    }
  }

  function clamp(page: number): number {
    const count = gallery?.pages.length ?? 0
    return Math.min(Math.max(page, 0), Math.max(count - 1, 0))
  }

  function summary(): Omit<api.Summary, 'id'> {
    const hash = gallery?.pages[0]?.src.replace(/^\/img\/|\.(avif|webp)$/g, '') ?? null
    return {
      title: gallery?.title ?? null,
      language: gallery?.language ?? null,
      kind: gallery?.kind ?? null,
      pages: gallery?.pages.length ?? 0,
      thumbnail_hash: hash,
    }
  }

  // 진행 상황 저장. 페이지를 넘길 때마다 쓰지 않고 잠잠해지면 한 번 쓴다.
  $effect(() => {
    const page = current
    if (!gallery || library.unavailable) return
    const timer = setTimeout(() => {
      void api.recordProgress(id, page, summary()).catch(() => {})
    }, SAVE_DELAY)
    return () => clearTimeout(timer)
  })

  // 갤러리 URL로 바로 들어오면 돌아갈 히스토리가 없다. 그럴 때는 마지막
  // 검색으로, 그것도 없으면 검색 화면으로 보낸다.
  function back() {
    if (history.length > 1) {
      history.back()
      return
    }
    location.hash = sessionStorage.getItem('tsuburu.lastSearch') ?? toSearch()
  }

  function resume() {
    if (resumeAt === null) return
    current = clamp(resumeAt)
    resumeAt = null
  }
</script>

<header>
  <button onclick={back}>&larr; {t('gallery.back')}</button>
  <h1>{gallery?.title ?? `#${id}`}</h1>
  {#if gallery && !library.unavailable}
    <button
      class="star"
      class:on={favorited}
      onclick={() => library.toggle(id, summary())}
      aria-pressed={favorited}
    >
      {favorited ? `★ ${t('gallery.saved')}` : `☆ ${t('gallery.save')}`}
    </button>
  {/if}
  {#if gallery}
    <span class="counter">{current + 1} / {gallery.pages.length}</span>
  {/if}
  <LocalePicker />
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if !gallery}
    <p class="status">{t('common.loading')}</p>
  {:else}
    <!-- Turning pages is a screenful at a time, so the note, the controls, the
         pages and the hint share the window and everything else waits below
         it. -->
    <div class="screen" class:paged>
      {#if resumeAt !== null}
        <div class="resume">
          {t('gallery.resume', { n: resumeAt + 1 })}
          <button onclick={resume}>{t('gallery.continue')}</button>
          <button onclick={() => (resumeAt = null)}>{t('gallery.startOver')}</button>
        </div>
      {/if}

      <ReaderBar count={gallery.pages.length} bind:current />
      <ReaderView pages={gallery.pages} bind:current onback={back} />
      <p class="hint">{paged ? t('reader.hintPaged') : t('gallery.hint')}</p>
    </div>

    <section class="about">
      {#if gallery.artists.length || gallery.series.length}
        <p class="credits">
          {#each gallery.artists as name (name)}
            <a href={toArtist(name)}>{name}</a>
          {/each}
          {#if gallery.series.length}
            <span class="muted">&middot; {gallery.series.join(', ')}</span>
          {/if}
        </p>
      {/if}

      <div class="keep">
        {#if download}
          <span class="muted">
            {t('gallery.onDisk', { have: download.have, pages: download.pages })}
            {#if download.job.running}&middot; {t('gallery.downloading')}{/if}
          </span>
        {/if}
        <button onclick={keepWork}>
          {download?.complete
            ? t('gallery.downloaded')
            : download
              ? t('gallery.getRest')
              : t('gallery.download')}
        </button>
        <button onclick={() => keepPage(current)}>{t('gallery.downloadPage')}</button>
        {#if downloadError}<span class="muted">{downloadError}</span>{/if}
      </div>

      {#if gallery.tags.length}
        <p class="tags">{gallery.tags.join(' · ')}</p>
      {/if}

      {#if keywords.length}
        <p class="keywords">
          <span class="muted">{t('keyword.title')}</span>
          {#each keywords as keyword (keyword.word)}
            <a href={toKeyword(keyword.word)}>{keyword.word}</a>
          {/each}
        </p>
      {/if}
    </section>

    {#if near.length}
      <section class="near">
        <strong>{t('keyword.near')}</strong>
        <Grid>
          {#each near as other (other.id)}
            <div>
              <Card id={other.id} />
              <p class="shared">{other.shared.join(' · ')}</p>
            </div>
          {/each}
        </Grid>
      </section>
    {/if}
  {/if}
</main>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    gap: 1rem;
    align-items: center;
    padding: 0.75rem 1rem;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font-size: 1rem;
    font-weight: 500;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .star.on {
    color: var(--accent);
    border-color: var(--accent);
  }

  .counter {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  main {
    padding: 1rem;
  }

  .screen.paged {
    display: flex;
    flex-direction: column;
    /* The window, less the sticky header and this padding. */
    min-height: calc(100dvh - 6rem);
  }

  .resume {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.6rem 0.9rem;
    margin-bottom: 1rem;
    font-size: 0.9rem;
  }

  .about {
    margin-top: 1.25rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }

  .tags {
    color: var(--muted);
    font-size: 0.85rem;
    margin: 0 0 1rem;
  }

  .credits {
    margin: 0 0 0.5rem;
    font-size: 0.9rem;
  }
  .credits a {
    margin-right: 0.5rem;
  }

  .keywords {
    margin: 0 0 1rem;
    font-size: 0.85rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: baseline;
  }
  .keywords a {
    text-decoration: none;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.1rem 0.55rem;
  }

  .near {
    margin: 2rem 0 1rem;
  }
  .near strong {
    display: block;
    margin-bottom: 0.6rem;
  }
  .shared {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    color: var(--muted);
  }

  .keep {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 1rem;
    font-size: 0.85rem;
  }
  .keep button {
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
  }
  .muted {
    color: var(--muted);
  }

  .status,
  .hint {
    color: var(--muted);
    text-align: center;
  }
  .hint {
    font-size: 0.85rem;
    margin: 0.6rem 0 0;
  }
</style>
