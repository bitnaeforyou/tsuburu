<script lang="ts">
  import * as api from '../lib/api'
  import ErrorNote from '../lib/ErrorNote.svelte'

  let { id }: { id: number } = $props()

  /** 미리 디코드해둘 다음 장 수. 넘길 때 흰 화면이 보이지 않을 만큼만. */
  const PREFETCH = 2

  let gallery = $state<api.Gallery | null>(null)
  let error = $state<unknown>(null)
  let current = $state(0)

  $effect(() => {
    void load()
  })

  async function load() {
    error = null
    try {
      gallery = await api.gallery(id)
    } catch (cause) {
      error = cause
    }
  }

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
        history.back()
      }
    }
    addEventListener('keydown', onKey)
    return () => removeEventListener('keydown', onKey)
  })

  function go(next: number) {
    if (!gallery) return
    current = Math.min(Math.max(next, 0), gallery.pages.length - 1)
    document.getElementById(`page-${current}`)?.scrollIntoView({ block: 'start' })
  }

  const page = $derived(gallery?.pages[current])
</script>

<header>
  <button onclick={() => history.back()}>← Back</button>
  <h1>{gallery?.title ?? `#${id}`}</h1>
  {#if gallery}
    <span class="counter">{current + 1} / {gallery.pages.length}</span>
  {/if}
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if !gallery}
    <p class="status">Loading…</p>
  {:else}
    {#if gallery.tags.length}
      <p class="tags">{gallery.tags.join(' · ')}</p>
    {/if}

    <div class="reader">
      {#each gallery.pages as p, i (p.src)}
        <img
          id={`page-${i}`}
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
  {/if}
  {#if page}
    <p class="hint">Arrow keys turn pages · Esc goes back</p>
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

  .counter {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  main { padding: 1rem; }

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

  .status, .hint {
    color: var(--muted);
    text-align: center;
  }
  .hint { font-size: 0.85rem; }
</style>
