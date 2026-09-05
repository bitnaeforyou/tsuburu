<script lang="ts">
  import * as api from '../lib/api'
  import { toGallery, toSearch } from '../lib/router'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import Card from '../lib/Card.svelte'

  let { query }: { query: string } = $props()

  const PAGE = 25

  let input = $state(query)
  let ids = $state<number[]>([])
  let total = $state(0)
  let loading = $state(false)
  let error = $state<unknown>(null)
  let offset = $state(0)

  let controller: AbortController | null = null

  // 검색어가 바뀌면 처음부터 다시 그린다.
  $effect(() => {
    input = query
    ids = []
    total = 0
    offset = 0
    error = null
    if (query) void load(0)
  })

  async function load(from: number) {
    controller?.abort()
    controller = new AbortController()
    loading = true
    error = null
    try {
      const page = await api.search(query, from, PAGE, controller.signal)
      total = page.total
      ids = from === 0 ? page.ids : [...ids, ...page.ids]
      offset = from + page.ids.length
    } catch (cause) {
      if ((cause as Error).name !== 'AbortError') error = cause
    } finally {
      loading = false
    }
  }

  function submit(event: SubmitEvent) {
    event.preventDefault()
    location.hash = toSearch(input.trim())
  }

  const hasMore = $derived(ids.length < total)
</script>

<header>
  <a class="brand" href={toSearch('')}>tsuburu</a>
  <form onsubmit={submit}>
    <input
      bind:value={input}
      placeholder="Search tags and terms — use -term to exclude"
      aria-label="Search"
      autocomplete="off"
    />
    <button type="submit" disabled={!input.trim()}>Search</button>
  </form>
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={() => load(0)} />
  {/if}

  {#if query && total > 0}
    <p class="count">{total.toLocaleString()} results</p>
  {:else if query && !loading && !error}
    <p class="count">No results for “{query}”.</p>
  {:else if !query}
    <p class="count">Search for a tag, artist, series, or character.</p>
  {/if}

  <div class="grid">
    {#each ids as id (id)}
      <a href={toGallery(id)} class="cell">
        <Card {id} />
      </a>
    {/each}
  </div>

  {#if loading}
    <p class="count">Loading…</p>
  {/if}

  {#if hasMore && !loading}
    <button class="more" onclick={() => load(offset)}>Load more</button>
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
  .brand {
    font-weight: 600;
    text-decoration: none;
    letter-spacing: 0.02em;
  }
  form {
    display: flex;
    gap: 0.5rem;
    flex: 1;
    max-width: 40rem;
  }
  form input { flex: 1; }

  main { padding: 1rem; }

  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 1rem;
  }

  .cell {
    text-decoration: none;
    /* 화면 밖 카드는 렌더링을 건너뛴다. 가상 스크롤 라이브러리 대신 쓴다. */
    content-visibility: auto;
    contain-intrinsic-size: auto 260px;
  }

  .more { margin: 1.5rem auto 0; display: block; }
</style>
