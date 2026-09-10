<script lang="ts">
  import * as api from '../lib/api'
  import { t } from '../lib/i18n.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { library } from '../lib/library.svelte'
  import { toArtist, toKeyword, toSearch } from '../lib/router'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import LocalePicker from '../lib/LocalePicker.svelte'

  let { id, startPage = null }: { id: number; startPage?: number | null } = $props()

  /** 미리 디코드해둘 다음 장 수. 넘길 때 흰 화면이 보이지 않을 만큼만. */
  const PREFETCH = 2
  /** 진행 상황을 쓰기 전에 기다리는 시간. 페이지마다 쓰면 디스크가 시끄럽다. */
  const SAVE_DELAY = 1000

  let gallery = $state<api.Gallery | null>(null)
  let error = $state<unknown>(null)
  let current = $state(0)
  let resumeAt = $state<number | null>(null)
  let elements = $state<(HTMLImageElement | null)[]>([])
  let keywords = $state<api.Keyword[]>([])
  let near = $state<api.NearWork[]>([])
  let download = $state<api.DownloadItem | null>(null)
  let downloadError = $state<string | null>(null)

  const favorited = $derived(library.has(id))

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
    if (startPage !== null) {
      requestAnimationFrame(() => go(startPage))
    }
    // 마지막으로 본 위치를 알린다. 자동으로 뛰지는 않는다. 처음부터 보려는
    // 경우를 빼앗지 않기 위해서다.
    try {
      const { items } = await api.history()
      const previous = items.find((item) => item.id === id)
      if (previous && previous.last_page > 0) resumeAt = previous.last_page
    } catch {
      // 기록을 못 읽어도 읽기는 계속된다.
    }
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

  // 스크롤을 따라 현재 페이지를 갱신한다. 세로 뷰어라 스크롤이 곧 페이지 이동이다.
  $effect(() => {
    const pages = gallery?.pages
    if (!pages) return
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .map((entry) => Number((entry.target as HTMLElement).dataset.page))
        if (visible.length) current = Math.min(...visible)
      },
      { rootMargin: '-45% 0px -45% 0px' },
    )
    for (const element of elements) if (element) observer.observe(element)
    return () => observer.disconnect()
  })

  // 다음 장을 미리 받아 디코드해둔다. 디코드까지 해야 넘길 때 끊기지 않는다.
  $effect(() => {
    const pages = gallery?.pages
    if (!pages) return
    for (let i = current + 1; i <= current + PREFETCH && i < pages.length; i++) {
      const img = new Image()
      img.src = pages[i].src
      void img.decode().catch(() => {})
    }
  })

  // 진행 상황 저장. 페이지를 넘길 때마다 쓰지 않고 잠잠해지면 한 번 쓴다.
  $effect(() => {
    const page = current
    if (!gallery || library.unavailable) return
    const timer = setTimeout(() => {
      void api.recordProgress(id, page, summary()).catch(() => {})
    }, SAVE_DELAY)
    return () => clearTimeout(timer)
  })

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!gallery) return
      if (event.key === 'ArrowRight' || event.key === ' ') {
        event.preventDefault()
        go(current + 1)
      } else if (event.key === 'ArrowLeft') {
        event.preventDefault()
        go(current - 1)
      } else if (event.key === 'Escape') {
        back()
      }
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
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

  function go(next: number) {
    if (!gallery) return
    current = Math.min(Math.max(next, 0), gallery.pages.length - 1)
    elements[current]?.scrollIntoView({ block: 'start' })
  }

  function resume() {
    if (resumeAt === null) return
    go(resumeAt)
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
    {#if resumeAt !== null}
      <div class="resume">
        {t('gallery.resume', { n: resumeAt + 1 })}
        <button onclick={resume}>{t('gallery.continue')}</button>
        <button onclick={() => (resumeAt = null)}>{t('gallery.startOver')}</button>
      </div>
    {/if}

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

    <div class="reader">
      {#each gallery.pages as p, i (p.src)}
        <img
          bind:this={elements[i]}
          data-page={i}
          src={p.src}
          width={p.width}
          height={p.height}
          alt={t('common.page', { n: i + 1 })}
          loading={i <= PREFETCH ? 'eager' : 'lazy'}
          decoding="async"
        />
      {/each}
    </div>

    <nav>
      <button onclick={() => go(current - 1)} disabled={current === 0}>
        {t('gallery.previous')}
      </button>
      <button onclick={() => go(current + 1)} disabled={current >= gallery.pages.length - 1}>
        {t('gallery.next')}
      </button>
    </nav>
    <p class="hint">{t('gallery.hint')}</p>

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

  .reader {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }

  .reader img {
    max-width: min(100%, 1000px);
    height: auto;
    background: var(--surface);
    /* 화면 밖 페이지는 레이아웃만 잡고 렌더링하지 않는다. */
    content-visibility: auto;
    contain-intrinsic-size: auto 1200px;
  }

  nav {
    display: flex;
    justify-content: center;
    gap: 0.75rem;
    margin: 1.5rem 0 0.5rem;
  }

  .status,
  .hint {
    color: var(--muted);
    text-align: center;
  }
  .hint {
    font-size: 0.85rem;
  }
</style>
