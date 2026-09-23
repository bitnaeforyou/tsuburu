<script lang="ts">
  import * as api from '../lib/api'
  import { t, number } from '../lib/i18n.svelte'
  import { defaultSearch, galleryNamed, toGallery, toSearch, type SearchState } from '../lib/router'
  import { remember as rememberSearch } from '../lib/preferences'
  import { library } from '../lib/library.svelte'
  import { markScroll, recall, remember } from '../lib/results.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import SearchBar from '../lib/SearchBar.svelte'
  import Terms from '../lib/Terms.svelte'
  import AllResults from '../lib/AllResults.svelte'
  import DialogueHitList from '../lib/DialogueHitList.svelte'
  import CorpusOffer from '../lib/CorpusOffer.svelte'

  let { params }: { params: SearchState } = $props()

  const PAGE = 25

  let ids = $state<number[]>([])
  let terms = $state<api.Term[]>([])
  let total = $state(0)
  let loading = $state(false)
  let error = $state<unknown>(null)
  let offset = $state(0)

  let hits = $state<api.DialogueHit[]>([])
  /// How far into the dialogue's total a reader can actually page.
  let reachable = $state(0)
  let controller: AbortController | null = null
  let localAvailable = $state(false)
  let dialogueAvailable = $state(false)
  /// How many works have been read, so a search that finds nothing can say
  /// how small the haystack was rather than implying the needle is not there.
  let readSoFar = $state(0)

  $effect(() => {
    void library.load().catch(() => {})
    void api.metaStatus().then((s) => (localAvailable = s.available)).catch(() => {})
    void api
      .dialogueStatus()
      .then((s) => {
        readSoFar = s.counts?.done ?? 0
        dialogueAvailable = s.supported && readSoFar > 0
      })
      .catch(() => {})
  })

  // Every source at once needs something to look for; with an empty box the
  // tag index is the one that can still browse.
  const sectioned = $derived(params.scope === 'all' && params.query.trim().length > 0)

  $effect(() => {
    if (params.query) sessionStorage.setItem('tsuburu.lastSearch', location.hash)
  })

  // The same for a search arrived at by its own address, so a pasted
  // ...?q=4183648 opens the work rather than looking for the number.
  $effect(() => {
    const named = galleryNamed(params.query)
    if (named !== null) location.replace(toGallery(named))
  })

  /// What this particular search is, for remembering what it found.
  const key = $derived(
    [params.query, params.sort, params.language, params.kind, params.scope, params.mode].join(' '),
  )

  // 검색어나 정렬, 필터가 바뀌면 처음부터 다시 그린다.
  $effect(() => {
    const asked = key
    ids = []
    terms = []
    hits = []
    total = 0
    reachable = 0
    offset = 0
    error = null
    if (sectioned) return

    // Coming back from a work: the same question has the same answer, and it
    // was left somewhere particular on the page.
    const before = recall(asked)
    if (before) {
      ids = before.ids
      terms = before.terms
      hits = before.hits
      total = before.total
      offset = before.offset
      restoreScroll(before.scrollY)
      return
    }

    // Local scope needs something to search for; hitomi scope can browse.
    if (params.scope === 'local' && !params.query && params.language === 'all' && params.kind === 'all') return
    if (params.scope === 'dialogue' && !params.query) return
    void load(0)
  })

  /// Puts the page back where it was.
  ///
  /// Once is not enough: the cards below the fold have no height until they
  /// are drawn, so the first attempt can run out of page before it gets
  /// there. The second one lands.
  function restoreScroll(want: number) {
    if (want <= 0) return
    requestAnimationFrame(() => scrollTo(0, want))
    setTimeout(() => {
      if (Math.abs(window.scrollY - want) > 8) scrollTo(0, want)
    }, 200)
  }

  // Kept as it changes, and the place on the page kept as it is left.
  $effect(() => {
    const asked = key
    if (sectioned || (ids.length === 0 && hits.length === 0)) return
    remember(asked, { ids, terms, hits, total, offset, scrollY: 0 })
    return () => markScroll(asked, window.scrollY)
  })

  async function load(from: number) {
    controller?.abort()
    controller = new AbortController()
    loading = true
    error = null
    try {
      if (params.scope === 'dialogue' && params.mode === 'meaning') {
        // A phrase is turned into a vector by the model tsuburu runs, and the
        // answer is the passages nearest it - so it comes back whole, not in
        // pages.
        const found = await api.phraseScenes(params.query)
        hits = found.map((hit) => ({ ...hit, exact: false, also: [] }))
        total = hits.length
        return
      } else if (params.scope === 'dialogue') {
        const found = await api.dialogueSearch(params.query, from, PAGE, controller.signal)
        hits = from === 0 ? found.hits : [...hits, ...found.hits]
        total = found.total
        reachable = found.reachable
        offset = from + found.hits.length
        return
      }
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
    const next = { ...params, ...changes }
    // A number or a hitomi address is the work itself, not something to look
    // for. Searching for it could only ever find nothing.
    const named = galleryNamed(next.query)
    const to = named === null ? toSearch(next) : toGallery(named)

    // Choosing a language or a kind says what to read from now on, not just
    // what to read now. Remembered *after* the address is built: an address
    // leaves out whatever matches the preference, so writing it first made
    // the new choice the thing being left out - the address came out
    // identical to the one already showing, nothing navigated, and the
    // change only appeared after going to another screen and back.
    if (changes.language !== undefined || changes.kind !== undefined) {
      rememberSearch({ language: changes.language, kind: changes.kind })
    }
    location.hash = to
  }

  const hasMore = $derived(
    params.scope === 'dialogue' ? hits.length < reachable : ids.length < total,
  )
  /// Said when there are more than paging can reach: the phrase is the thing
  /// to change, not the page.
  const beyondPaging = $derived(
    params.scope === 'dialogue' && hits.length >= reachable && total > reachable,
  )
  const filtering = $derived(
    params.language !== defaultSearch.language || params.kind !== defaultSearch.kind,
  )
  // 결과가 좁은데 인기순이면 목록을 크게 훑어야 한다(스펙 3.2절).
  const slowSort = $derived(loading && params.sort !== 'date' && total > 0 && total < 500)
</script>

<AppHeader active="search" />
<SearchBar {params} onchange={go} {localAvailable} {dialogueAvailable} />

<main>
  <h1 class="sr-only">{t('nav.search')}</h1>

  <!-- The one time a reader is certain to see it, before they have gone
       looking for anything: the dialogue of a hundred thousand works is a
       button away, and the question is asked once. -->
  <CorpusOffer ask />
  {#if error}
    <ErrorNote {error} onretry={() => load(0)} />
  {/if}

  {#if sectioned}
    <AllResults {params} {localAvailable} {dialogueAvailable} />
  {:else}
  <Terms {terms} />

  {#if total > 0}
    <p class="count">
      {t('search.results', { n: number(total) })}
      {#if params.scope === 'local'}&middot; {t('search.fromSnapshot')}{/if}
      {#if !params.query && !filtering}&middot; {t('search.browsingAll')}{/if}
    </p>
  {:else if !loading && !error}
    <p class="count">
      {params.query ? t('search.noResultsFor', { query: params.query }) : t('search.noResults')}
      <!-- The dialogue is only as big as what has been read, and a reader who
           has read forty works is not looking at a broken search. -->
      {#if params.scope === 'dialogue'}
        &middot; {t('search.readSoFar', { n: number(readSoFar) })}
      {/if}
    </p>
  {/if}

  {#if slowSort}
    <p class="count">{t('search.slowSort')}</p>
  {/if}

  {#if params.scope === 'dialogue'}
    <!-- Where it is worth the most: the results are thin, and the reason is
         that the corpus is not in yet. -->
    <CorpusOffer />
    <DialogueHitList {hits} />
    {#if beyondPaging}
      <p class="count">{t('search.beyondPaging', { n: number(reachable), total: number(total) })}</p>
    {/if}
  {:else}
    <Grid>
      {#each ids as id (id)}
        <Card {id} />
      {/each}
    </Grid>
  {/if}

  {#if loading}
    <p class="count">{t('common.loading')}</p>
  {/if}

  {#if hasMore && !loading}
    <button class="more" onclick={() => load(offset)}>{t('common.loadMore')}</button>
  {/if}
  {/if}
</main>

<style>
  main {
    max-width: var(--page);
    margin-inline: auto;
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
