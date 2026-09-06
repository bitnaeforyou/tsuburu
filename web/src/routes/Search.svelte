<script lang="ts">
  import * as api from '../lib/api'
  import { defaultSearch, toSearch, type Sort, type SearchState } from '../lib/router'
  import { library } from '../lib/library.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import Nav from '../lib/Nav.svelte'
  import Terms from '../lib/Terms.svelte'

  let { params }: { params: SearchState } = $props()

  const PAGE = 25

  const SORTS: { value: Sort; label: string }[] = [
    { value: 'date', label: 'Newest' },
    { value: 'today', label: 'Popular today' },
    { value: 'week', label: 'Popular this week' },
    { value: 'month', label: 'Popular this month' },
    { value: 'year', label: 'Popular this year' },
  ]
  const LANGUAGES = ['all', 'korean', 'japanese', 'english', 'chinese', 'spanish']
  const KINDS = ['all', 'doujinshi', 'manga', 'artistcg', 'gamecg', 'imageset']

  let input = $state(params.query)
  let ids = $state<number[]>([])
  let terms = $state<api.Term[]>([])
  let total = $state(0)
  let loading = $state(false)
  let error = $state<unknown>(null)
  let offset = $state(0)

  let controller: AbortController | null = null

  $effect(() => {
    void library.load().catch(() => {})
  })

  $effect(() => {
    if (params.query) sessionStorage.setItem('tsuburu.lastSearch', location.hash)
  })

  // 검색어나 정렬, 필터가 바뀌면 처음부터 다시 그린다.
  $effect(() => {
    const key = [params.query, params.sort, params.language, params.kind].join(' ')
    void key
    input = params.query
    ids = []
    terms = []
    total = 0
    offset = 0
    error = null
    void load(0)
  })

  async function load(from: number) {
    controller?.abort()
    controller = new AbortController()
    loading = true
    error = null
    try {
      const page = await api.search(
        {
          q: params.query,
          offset: from,
          limit: PAGE,
          sort: params.sort,
          language: params.language,
          kind: params.kind,
        },
        controller.signal,
      )
      total = page.total
      terms = page.terms
      ids = from === 0 ? page.ids : [...ids, ...page.ids]
      offset = from + page.ids.length
    } catch (cause) {
      if ((cause as Error).name !== 'AbortError') error = cause
    } finally {
      loading = false
    }
  }

  function go(changes: Partial<SearchState>) {
    location.hash = toSearch({ ...params, ...changes })
  }

  function submit(event: SubmitEvent) {
    event.preventDefault()
    go({ query: input.trim() })
  }

  const hasMore = $derived(ids.length < total)
  const filtering = $derived(
    params.language !== defaultSearch.language || params.kind !== defaultSearch.kind,
  )
  // 결과가 좁은데 인기순이면 목록을 크게 훑어야 한다(스펙 3.2절).
  const slowSort = $derived(loading && params.sort !== 'date' && total > 0 && total < 500)
</script>

<header>
  <a class="brand" href={toSearch()}>tsuburu</a>
  <form onsubmit={submit}>
    <input
      bind:value={input}
      placeholder="Search in Korean or English, use -term to exclude"
      aria-label="Search"
      autocomplete="off"
    />
    <button type="submit">Search</button>
  </form>
  <Nav active="search" />
</header>

<div class="controls">
  <label>
    Sort
    <select value={params.sort} onchange={(e) => go({ sort: e.currentTarget.value as Sort })}>
      {#each SORTS as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
  <label>
    Language
    <select value={params.language} onchange={(e) => go({ language: e.currentTarget.value })}>
      {#each LANGUAGES as value (value)}
        <option {value}>{value}</option>
      {/each}
    </select>
  </label>
  <label>
    Type
    <select value={params.kind} onchange={(e) => go({ kind: e.currentTarget.value })}>
      {#each KINDS as value (value)}
        <option {value}>{value}</option>
      {/each}
    </select>
  </label>
</div>

<main>
  {#if error}
    <ErrorNote {error} onretry={() => load(0)} />
  {/if}

  <Terms {terms} />

  {#if total > 0}
    <p class="count">
      {total.toLocaleString()} results
      {#if !params.query && !filtering}&middot; browsing everything{/if}
    </p>
  {:else if !loading && !error}
    <p class="count">
      {params.query ? `No results for ${params.query}.` : 'Nothing matched these filters.'}
    </p>
  {/if}

  {#if slowSort}
    <p class="count">Few results to sort by popularity, so this may take a moment.</p>
  {/if}

  <Grid>
    {#each ids as id (id)}
      <Card {id} />
    {/each}
  </Grid>

  {#if loading}
    <p class="count">Loading...</p>
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
  form input {
    flex: 1;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--border);
    font-size: 0.85rem;
    color: var(--muted);
  }
  .controls label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
  }

  main {
    padding: 1rem;
  }

  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }

  .more {
    margin: 1.5rem auto 0;
    display: block;
  }
</style>
