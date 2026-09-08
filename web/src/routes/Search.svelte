<script lang="ts">
  import * as api from '../lib/api'
  import { defaultSearch, toSearch, type SearchState } from '../lib/router'
  import { library } from '../lib/library.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import SearchBar from '../lib/SearchBar.svelte'
  import Terms from '../lib/Terms.svelte'

  let { params }: { params: SearchState } = $props()

  const PAGE = 25

  let ids = $state<number[]>([])
  let terms = $state<api.Term[]>([])
  let total = $state(0)
  let loading = $state(false)
  let error = $state<unknown>(null)
  let offset = $state(0)

  let controller: AbortController | null = null
  let localAvailable = $state(false)

  $effect(() => {
    void library.load().catch(() => {})
    void api.metaStatus().then((s) => (localAvailable = s.available)).catch(() => {})
  })

  $effect(() => {
    if (params.query) sessionStorage.setItem('tsuburu.lastSearch', location.hash)
  })

  // 검색어나 정렬, 필터가 바뀌면 처음부터 다시 그린다.
  $effect(() => {
    const key = [params.query, params.sort, params.language, params.kind, params.scope].join(' ')
    void key
    ids = []
    terms = []
    total = 0
    offset = 0
    error = null
    // Local scope needs something to search for; hitomi scope can browse.
    if (params.scope === 'local' && !params.query && params.language === 'all' && params.kind === 'all') return
    void load(0)
  })

  async function load(from: number) {
    controller?.abort()
    controller = new AbortController()
    loading = true
    error = null
    try {
      if (params.scope === 'local') {
        // The snapshot has no popularity data; results are newest first.
        const page = await api.metaSearch(
          { q: params.query, offset: from, limit: PAGE, language: params.language, kind: params.kind },
          controller.signal,
        )
        total = page.total
        terms = []
        ids = from === 0 ? page.ids : [...ids, ...page.ids]
        offset = from + page.ids.length
        return
      }
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

  const hasMore = $derived(ids.length < total)
  const filtering = $derived(
    params.language !== defaultSearch.language || params.kind !== defaultSearch.kind,
  )
  // 결과가 좁은데 인기순이면 목록을 크게 훑어야 한다(스펙 3.2절).
  const slowSort = $derived(loading && params.sort !== 'date' && total > 0 && total < 500)
</script>

<AppHeader active="search" />
<SearchBar {params} onchange={go} {localAvailable} />

<main>
  {#if error}
    <ErrorNote {error} onretry={() => load(0)} />
  {/if}

  <Terms {terms} />

  {#if total > 0}
    <p class="count">
      {total.toLocaleString()} results
      {#if params.scope === 'local'}&middot; from the local snapshot{/if}
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
