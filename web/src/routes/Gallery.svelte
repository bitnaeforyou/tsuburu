<script lang="ts">
  import * as api from '../lib/api'
  import { untrack } from 'svelte'
  import { i18n, t, number } from '../lib/i18n.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { library, read as readSoFar } from '../lib/library.svelte'
  import { toArtist, toGallery, toKeyword, toSearch, toSeries } from '../lib/router'
  import { split as splitTag } from '../lib/tags'
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

  let loaded = $state<api.Gallery | null>(null)
  /// The work this address asks for, and only that one. Fetching never blanks
  /// what is on screen; a result for another id simply is not this screen's.
  const gallery = $derived(loaded?.id === id ? loaded : null)
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
  /// The cover, at the size it was drawn. Tapping it used to start reading,
  /// which is what the button beside it is for - and left no way to simply
  /// look at the picture.
  let showingCover = $state(false)



  const favorited = $derived(library.has(id))
  const paged = $derived(reader.settings.layout !== 'scroll')

  // Scrolling has no middle to tap, so there would be no way back out of
  // `bare` there; the chrome gets out of the way by itself instead.
  $effect(() => {
    if (!paged) bare = false
  })

  /// Whether the bars are out of the way while scrolling down.
  ///
  /// A scrolling reader kept the header and the controls pinned to the top
  /// for the whole work, which on a phone is a third of the window spent
  /// saying what you are already looking at. They go as you read down and
  /// come back the moment you go up - no tap target to find, and nothing to
  /// get stuck in.
  let tucked = $state(false)

  $effect(() => {
    if (paged || !reading) {
      tucked = false
      return
    }
    let last = scrollY
    const onScroll = () => {
      const y = scrollY
      // A drift of a few pixels is not a decision, and the top of a work is
      // where its title belongs.
      if (y < 96) tucked = false
      else if (y - last > 8) tucked = true
      else if (last - y > 8) tucked = false
      last = y
    }
    addEventListener('scroll', onScroll, { passive: true })
    return () => removeEventListener('scroll', onScroll)
  })

  // Favorites belong to every screen rather than to this work. Loading them
  // here put them in the same effect as the gallery, so the moment they
  // arrived it ran again and started a second fetch on top of the first.
  $effect(() => {
    void library.load().catch(() => {})
  })

  /// Which fetch is the current one. An older one that comes back late must
  /// not put its answer on the screen, and must not take it off either.
  let asked = 0

  $effect(() => {
    // Both are read here, so that arriving at another work - or at another page
    // of this one - starts again instead of leaving the last one on screen.
    void id
    void startPage
    // The tags come back said in the interface language, so the work on
    // screen is the right work but the wrong words once it changes.
    void load(i18n.locale)
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

  /// The language the work on screen was fetched in, so a change of it is
  /// not mistaken for "already have this one".
  let said = $state<string | null>(null)

  async function load(lang: string) {
    const mine = ++asked
    // Read without being watched: an effect that depends on what it is about
    // to replace runs again in the middle of its own work.
    const have = untrack(() => loaded)
    const spoken = untrack(() => said)
    // Only the page changed: the work on screen is already the right one, and
    // refetching it would arrive at the same place a second later.
    if (have?.id === id && spoken === lang) {
      if (startPage !== null) current = clamp(startPage)
      return
    }

    error = null
    // A page number belongs to the work it was read in, not to the next one.
    current = 0
    lastPage = null
    let found: api.Gallery
    try {
      found = await api.gallery(id)
      said = lang
    } catch (cause) {
      if (mine === asked) error = cause
      return
    }
    if (mine !== asked) return
    loaded = found
    if (startPage !== null) current = clamp(startPage)

    try {
      const { items } = await api.history()
      if (mine !== asked) return
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
      const pages = gallery?.pages.length ?? 0
      void api
        .recordProgress(id, page, summary())
        .then(() => readSoFar.note(id, page, pages))
        .catch(() => {})
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

<header class:bare class:tucked>
  <button onclick={back}>&larr; {t('gallery.back')}</button>
  {#if reading}
    <!-- While the pages have the screen, the bar is the only thing left
         saying which work they belong to. -->
    <h1>{gallery?.title ?? `#${id}`}</h1>
  {:else}
    <span class="rest"></span>
  {/if}
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
    <ErrorNote {error} onretry={() => load(i18n.locale)} />
  {:else if !gallery}
    <p class="status">{t('common.loading')}</p>
  {:else if reading}
    <!-- Turning pages is a screenful at a time, so the controls, the pages and
         the hint share the window and everything else waits below it. -->
    <div class="screen" class:paged class:bare>
      {#if !bare}
        <ReaderBar
          count={gallery.pages.length}
          bind:current
          {tucked}
          onpages={() => (picking = true)}
        />
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
        <button
          class="cover"
          onclick={() => (showingCover = true)}
          aria-label={t('gallery.cover')}
          title={t('gallery.cover')}
        >
          <img src={thumbnailOf(gallery.pages[0].src)} alt="" decoding="async" />
        </button>
      {/if}

      <div class="facts">
        <!-- The bar truncates a long title to one line; its own page is where
             a work gets to be called by its whole name. -->
        <h1 class="title">{gallery.title ?? `#${id}`}</h1>
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

        <!-- Beside the cover, where the decision to read is made. These used
             to sit under the page preview, which on a phone is a screen and a
             half of scrolling away. -->
        {#if gallery.tags.length}
          <ul class="tags">
            {#each gallery.tags as tag (tag)}
              {@const parsed = splitTag(tag)}
              <li class={parsed.who ?? 'plain'}>{parsed.word}</li>
            {/each}
          </ul>
        {/if}

        <div class="start">
          {#if lastPage !== null}
            <button class="primary" onclick={() => read(lastPage ?? 0)}>
              {t('gallery.resumeAt', { n: lastPage + 1 })}
            </button>
            <button onclick={() => read(0)}>{t('gallery.startOver')}</button>
          {:else}
            <button class="primary" onclick={() => read(0)}>{t('gallery.read')}</button>
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
        <h2>{t('keyword.near')}</h2>
        <Grid>
          {#each near as other (other.id)}
            <div>
              <Card id={other.id} level={3} />
              <p class="shared">{other.shared.join(' · ')}</p>
            </div>
          {/each}
        </Grid>
      </section>
    {/if}
  {/if}

  {#if gallery?.pages[0] && showingCover}
    <!-- The whole picture, not the three-by-four the card crops it to. -->
    <div
      class="lightbox"
      role="button"
      tabindex="0"
      aria-label={t('gallery.coverClose')}
      onclick={() => (showingCover = false)}
      onkeydown={(e) => {
        if (e.key === 'Escape' || e.key === 'Enter' || e.key === ' ') showingCover = false
      }}
    >
      <img src={gallery.pages[0].src} alt={gallery.title ?? `#${id}`} />
    </div>
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
    height: calc(var(--chrome) + var(--safe-top));
    padding: var(--safe-top) var(--gutter) 0;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }
  header.bare {
    display: none;
  }
  /* Slid rather than removed: taking it out of the flow would pull the page
     up by its own height on every scroll. */
  header.tucked {
    transform: translateY(-100%);
  }
  @media (prefers-reduced-motion: no-preference) {
    header {
      transition: transform 160ms ease-out;
    }
  }

  /* A label on a bar, not the name of the page: it stays a step under the
     title the work carries on its own screen. */
  header h1 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 500;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rest {
    flex: 1;
  }

  .star.on {
    color: var(--accent);
    border-color: var(--accent);
  }

  .counter {
    color: var(--muted);
    /* Chrome, like everything else on this bar - it was inheriting the body
       size and out-shouting the title beside it. */
    font-size: var(--text-md);
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

  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 6;
    display: grid;
    place-items: center;
    padding: calc(1rem + var(--safe-top)) 1rem 1rem;
    background: color-mix(in srgb, var(--bg) 92%, transparent);
    cursor: zoom-out;
    overscroll-behavior: contain;
  }
  .lightbox img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius);
    border: 1px solid var(--image-edge);
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
    border: 1px solid var(--image-edge);
    border-radius: var(--radius);
  }
  .cover {
    cursor: zoom-in;
  }
  .cover:hover img {
    border-color: var(--accent);
  }

  .facts {
    min-width: 0;
    /* A title and a tag list do not get to be two thousand pixels wide just
       because the window is. */
    max-width: 68ch;
  }
  .title {
    margin: 0 0 0.35rem;
    line-height: 1.3;
    /* hitomi titles are often one unbroken run of underscores. */
    overflow-wrap: anywhere;
  }
  .other {
    margin: 0 0 0.35rem;
    color: var(--muted);
    font-size: var(--text-md);
  }
  .line {
    margin: 0 0 0.5rem;
    color: var(--muted);
    font-size: var(--text-sm);
  }
  .id {
    font-variant-numeric: tabular-nums;
    user-select: all;
  }

  .credits {
    margin: 0 0 0.9rem;
    font-size: var(--text-md);
  }
  .credits a {
    margin-inline-end: 0.5rem;
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
  /* Naming a part of the work's page, not the page. */
  .preview h2 {
    font-size: var(--text-md);
    margin: 0 0 0.6rem;
  }

  .start {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  .tags {
    list-style: none;
    padding: 0;
    margin: 0 0 1rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .tags li {
    font-size: var(--text-xs);
    padding: 0.12rem 0.45rem;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--muted);
  }
  .tags li.female {
    background: var(--tag-female-quiet);
    color: var(--tag-female);
  }
  .tags li.male {
    background: var(--tag-male-quiet);
    color: var(--tag-male);
  }

  .keywords {
    margin: 0 0 1rem;
    font-size: var(--text-sm);
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: baseline;
  }
  .keywords a {
    text-decoration: none;
    border: 1px solid var(--edge);
    border-radius: 999px;
    padding: 0.1rem 0.55rem;
  }

  .near {
    margin: 2rem 0 1rem;
  }
  .near h2 {
    font-size: var(--text-md);
    margin: 0 0 0.6rem;
  }
  .shared {
    margin: 0.25rem 0 0;
    font-size: var(--text-xs);
    color: var(--muted);
  }

  .keep {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    font-size: var(--text-sm);
  }
  .keep button {
    font-size: var(--text-sm);
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
    font-size: var(--text-sm);
    margin: 0.6rem 0 0;
  }

  /* A phone spends a third of its screen on the things around the pages
     unless they are told to be smaller. */
  @media (max-width: 640px) {
    header {
      gap: 0.5rem;
      padding: 0 var(--gutter);
    }
    header h1 {
      font-size: var(--text-sm);
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
      font-size: var(--text-lg);
    }
    .hint {
      font-size: var(--text-xs);
      margin-top: 0.4rem;
    }
    .keep button {
      font-size: var(--text-md);
      padding: 0.45rem 0.7rem;
    }
  }
</style>
