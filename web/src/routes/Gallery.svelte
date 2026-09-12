<script lang="ts">
  import * as api from '../lib/api'
  import { t, number } from '../lib/i18n.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { library } from '../lib/library.svelte'
  import { toArtist, toGallery, toKeyword, toSearch, toSeries } from '../lib/router'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import LocalePicker from '../lib/LocalePicker.svelte'
  import ReaderView from '../lib/ReaderView.svelte'
  import ReaderBar from '../lib/ReaderBar.svelte'
  import PageGrid from '../lib/PageGrid.svelte'
  import { byTouch, reader, thumbnailOf } from '../lib/reader.svelte'

  let { id, startPage = null }: { id: number; startPage?: number | null } = $props()

  /** 진행 상황을 쓰기 전에 기다리는 시간. 페이지마다 쓰면 디스크가 시끄럽다. */
  const SAVE_DELAY = 1000

  /// A page in the address means reading; without one this is the work itself,
  /// which is what you want when you have just found it and are deciding.
  const reading = $derived(startPage !== null)

  let gallery = $state<api.Gallery | null>(null)
  let error = $state<unknown>(null)
  let current = $state(0)
  /// How far the work was read before, from the history. Null until asked.
  let lastPage = $state<number | null>(null)
  let keywords = $state<api.Keyword[]>([])
  let near = $state<api.NearWork[]>([])
  let download = $state<api.DownloadItem | null>(null)
  let downloadError = $state<string | null>(null)
  let bare = $state(false)
  let picking = $state(false)

  const favorited = $derived(library.has(id))
  const paged = $derived(reader.settings.layout !== 'scroll')

  // Scrolling has no middle to tap, so there would be no way back out.
  $effect(() => {
    if (!paged) bare = false
  })

  $effect(() => {
    // Both are read here, so that arriving at another work - or at another page
    // of this one - starts again instead of leaving the last one on screen.
    void id
    void startPage
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
    // Only the page changed: the work on screen is already the right one, and
    // refetching it would blank the screen to arrive at the same place.
    if (gallery?.id === id) {
      if (startPage !== null) current = clamp(startPage)
      return
    }

    error = null
    // A page number belongs to the work it was read in, not to the next one.
    gallery = null
    current = 0
    lastPage = null
    try {
      gallery = await api.gallery(id)
    } catch (cause) {
      error = cause
      return
    }
    if (startPage !== null) current = clamp(startPage)

    try {
      const { items } = await api.history()
      const previous = items.find((item) => item.id === id)
      if (previous && previous.last_page > 0) lastPage = previous.last_page
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
    if (!gallery || !reading || library.unavailable) return
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

  /// Opening the reader is a step of its own, so closing it comes back here
  /// rather than throwing you out of the work altogether.
  function read(page: number) {
    location.hash = toGallery(id, clamp(page))
  }

  const kindOf = (kind: string | null) =>
    kind ? t(`kind.${kind}` as 'kind.manga', {}) : null
</script>

<header class:bare>
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
  {#if gallery && reading}
    <button onclick={() => (location.hash = toGallery(id))}>{t('gallery.info')}</button>
    <span class="counter">{current + 1} / {gallery.pages.length}</span>
  {/if}
  <LocalePicker />
</header>

<main class:bare class:reading>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if !gallery}
    <p class="status">{t('common.loading')}</p>
  {:else if reading}
    <!-- Turning pages is a screenful at a time, so the controls, the pages and
         the hint share the window and everything else waits below it. -->
    <div class="screen" class:paged class:bare>
      {#if !bare}
        <ReaderBar count={gallery.pages.length} bind:current onpages={() => (picking = true)} />
      {/if}
      <ReaderView
        pages={gallery.pages}
        bind:current
        onback={() => (bare ? (bare = false) : back())}
        onchrome={() => (bare = !bare)}
        onpages={() => (picking = true)}
      />
      <!-- Telling a phone about Esc and the arrow keys is noise. -->
      {#if !bare}
        <p class="hint">{byTouch.is ? t('reader.hintTouch') : t('reader.hintPaged')}</p>
      {/if}
    </div>
  {:else}
    <!-- The work before the pages: what it is, who made it, and whether you
         have been here before. -->
    <section class="work">
      {#if gallery.pages[0]}
        <button class="cover" onclick={() => read(lastPage ?? 0)}>
          <img src={thumbnailOf(gallery.pages[0].src)} alt="" decoding="async" />
        </button>
      {/if}

      <div class="facts">
        <!-- The header truncates a long title to one line; its own page is
             where a work gets to be called by its whole name. -->
        <h2 class="title">{gallery.title ?? `#${id}`}</h2>
        {#if gallery.japanese_title && gallery.japanese_title !== gallery.title}
          <p class="other">{gallery.japanese_title}</p>
        {/if}

        <p class="line">
          <!-- The number is the work's address on hitomi, and what someone
               pasting it into the search box is pasting. -->
          <span class="id">#{id}</span>
          &middot; {t('common.pages', { n: number(gallery.pages.length) })}
          {#if gallery.language}&middot; {gallery.language}{/if}
          {#if kindOf(gallery.kind)}&middot; {kindOf(gallery.kind)}{/if}
          {#if gallery.date}&middot; {gallery.date.slice(0, 10)}{/if}
        </p>

        {#if gallery.artists.length || gallery.series.length}
          <p class="credits">
            {#each gallery.artists as name (name)}
              <a href={toArtist(name)}>{name}</a>
            {/each}
            {#each gallery.series as name (name)}
              <a class="series" href={toSeries(name)}>{name}</a>
            {/each}
          </p>
        {/if}

        <div class="start">
          {#if lastPage !== null}
            <button class="go" onclick={() => read(lastPage ?? 0)}>
              {t('gallery.resumeAt', { n: lastPage + 1 })}
            </button>
            <button onclick={() => read(0)}>{t('gallery.startOver')}</button>
          {:else}
            <button class="go" onclick={() => read(0)}>{t('gallery.read')}</button>
          {/if}
        </div>

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
          {#if downloadError}<span class="muted">{downloadError}</span>{/if}
        </div>
      </div>
    </section>

    <section class="preview">
      <h2>{t('gallery.preview', { n: gallery.pages.length })}</h2>
      <PageGrid
        inline
        pages={gallery.pages}
        current={lastPage ?? -1}
        onpick={(page) => read(page)}
      />
    </section>

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

  {#if gallery && picking}
    <PageGrid
      pages={gallery.pages}
      {current}
      onpick={(page) => {
        picking = false
        if (reading) current = page
        else read(page)
      }}
      onclose={() => (picking = false)}
    />
  {/if}
</main>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    gap: 1rem;
    align-items: center;
    height: var(--chrome);
    padding: 0 var(--gutter);
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }
  header.bare {
    display: none;
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
    max-width: var(--page);
    margin-inline: auto;
    padding: 1rem;
  }
  /* Pages are read edge to edge: a reader is not a page of a document. */
  main.reading {
    max-width: none;
    padding-inline: var(--gutter);
  }
  main.bare {
    max-width: none;
    padding: 0;
  }

  .screen.paged {
    display: flex;
    flex-direction: column;
    /* The window, less the sticky header and this padding. */
    min-height: calc(100dvh - var(--chrome) - 2rem);
  }
  .screen.paged.bare {
    min-height: 100dvh;
  }

  /* --- the work --- */

  .work {
    display: flex;
    gap: 1.25rem;
    align-items: flex-start;
    margin-bottom: 1.25rem;
  }

  .cover {
    flex: none;
    width: 210px;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    line-height: 0;
  }
  .cover img {
    width: 100%;
    aspect-ratio: 3 / 4;
    object-fit: cover;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .cover:hover img {
    border-color: var(--accent);
  }

  .facts {
    min-width: 0;
  }
  .title {
    margin: 0 0 0.35rem;
    font-size: 1.15rem;
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .other {
    margin: 0 0 0.35rem;
    color: var(--muted);
    font-size: 0.95rem;
  }
  .line {
    margin: 0 0 0.5rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .id {
    font-variant-numeric: tabular-nums;
    user-select: all;
  }

  .credits {
    margin: 0 0 0.9rem;
    font-size: 0.9rem;
  }
  .credits a {
    margin-right: 0.5rem;
  }
  .series {
    color: var(--muted);
  }
  .series::before {
    content: '· ';
  }

  .preview {
    margin: 1.5rem 0;
  }
  .preview h2 {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0 0 0.6rem;
  }

  .start {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  .go {
    color: var(--accent);
    border-color: var(--accent);
    font-weight: 500;
  }

  .tags {
    color: var(--muted);
    font-size: 0.85rem;
    margin: 0 0 1rem;
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

  /* A phone spends a third of its screen on the things around the pages
     unless they are told to be smaller. */
  @media (max-width: 640px) {
    header {
      gap: 0.5rem;
      padding: 0 var(--gutter);
    }
    h1 {
      font-size: 0.9rem;
    }
    main {
      padding: 0.75rem;
    }
    .work {
      gap: 0.9rem;
    }
    .cover {
      width: 38%;
      max-width: 140px;
    }
    .title {
      font-size: 1rem;
    }
    .hint {
      font-size: 0.72rem;
      margin-top: 0.4rem;
    }
    .keep button {
      font-size: 0.85rem;
      padding: 0.45rem 0.7rem;
    }
  }
</style>
