<script lang="ts">
  import * as api from '../lib/api'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { library } from '../lib/library.svelte'
  import { toSearch } from '../lib/router'

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

  const favorited = $derived(library.has(id))

  $effect(() => {
    void load()
    void library.load().catch(() => {})
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
  <button onclick={back}>&larr; Back</button>
  <h1>{gallery?.title ?? `#${id}`}</h1>
  {#if gallery && !library.unavailable}
    <button
      class="star"
      class:on={favorited}
      onclick={() => library.toggle(id, summary())}
      aria-pressed={favorited}
    >
      {favorited ? '★ Saved' : '☆ Save'}
    </button>
  {/if}
  {#if gallery}
    <span class="counter">{current + 1} / {gallery.pages.length}</span>
  {/if}
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if !gallery}
    <p class="status">Loading...</p>
  {:else}
    {#if resumeAt !== null}
      <div class="resume">
        You stopped on page {resumeAt + 1}.
        <button onclick={resume}>Continue</button>
        <button onclick={() => (resumeAt = null)}>Start over</button>
      </div>
    {/if}

    {#if gallery.tags.length}
      <p class="tags">{gallery.tags.join(' · ')}</p>
    {/if}

    <div class="reader">
      {#each gallery.pages as p, i (p.src)}
        <img
          bind:this={elements[i]}
          data-page={i}
          src={p.src}
          width={p.width}
          height={p.height}
          alt={`Page ${i + 1}`}
          loading={i <= PREFETCH ? 'eager' : 'lazy'}
          decoding="async"
        />
      {/each}
    </div>

    <nav>
      <button onclick={() => go(current - 1)} disabled={current === 0}>Previous</button>
      <button onclick={() => go(current + 1)} disabled={current >= gallery.pages.length - 1}>
        Next
      </button>
    </nav>
    <p class="hint">Arrow keys turn pages &middot; Esc goes back</p>
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
