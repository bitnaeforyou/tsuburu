<script lang="ts">
  import * as api from '../lib/api'
  import { i18n, t, number } from '../lib/i18n.svelte'
  import { defaultSearch, galleryNamed, toGallery, toSearch, type SearchState } from '../lib/router'
  import { remember as rememberSearch } from '../lib/preferences'
  import { library } from '../lib/library.svelte'
  import {
    forget as forgetResults,
    markScroll,
    recall,
    remember,
    type Anchor,
    type Place,
  } from '../lib/results.svelte'
  import { forget as forgetCards } from '../lib/cards.svelte'
  import { keepThisVisit } from '../lib/kept'
  import { keepScroll } from '../lib/keepScroll.svelte'
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
  /// Whether the first screen's tags narrowed what is shown.
  let narrowed = $state(false)
  let controller: AbortController | null = null
  let localAvailable = $state(false)
  let dialogueAvailable = $state(false)
  /// How many works have been read, so a search that finds nothing can say
  /// how small the haystack was rather than implying the needle is not there.
  let readSoFar = $state(0)
  /// An everywhere search draws its sections from a child that fetches on
  /// mount, so the only way to make it ask again is to build it again.
  let generation = $state(0)

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

  // Whatever this screen is showing, so the search tab comes back to it from
  // any other tab rather than to an empty first screen. Not a number, which
  // this screen turns straight into the work it names: coming back to it
  // would open that work again.
  $effect(() => {
    if (galleryNamed(params.query) === null) keepThisVisit('tsuburu.lastSearch', toSearch(params))
  })

  // Every source at once is drawn by sections that fetch for themselves, so
  // it has no list of its own to put back. The page can still be put back
  // where it was, once the sections have come back.
  $effect(() => {
    if (sectioned) return keepScroll(`sections ${key}`, () => true)
  })

  // The same for a search arrived at by its own address, so a pasted
  // ...?q=4183648 opens the work rather than looking for the number.
  $effect(() => {
    const named = galleryNamed(params.query)
    if (named !== null) location.replace(toGallery(named))
  })

  /// What this particular search is, for remembering what it found.
  const key = $derived(
    [
      params.query,
      params.sort,
      params.language,
      params.kind,
      params.scope,
      params.mode,
      // The tags come back said in the interface language, so results found
      // in one are not the results of the other.
      i18n.locale,
    ].join(' '),
  )

  // 검색어나 정렬, 필터가 바뀌면 처음부터 다시 그린다.
  $effect(() => {
    const asked = key
    stopRestore?.()
    stopRestore = undefined
    ids = []
    terms = []
    hits = []
    total = 0
    reachable = 0
    narrowed = false
    offset = 0
    error = null
    if (sectioned) return

    // Coming back from a work - or from the phone taking the window away and
    // building it again - the same question has the same answer, and it was
    // left somewhere particular on the page.
    const before = recall(asked)
    if (before && (before.ids.length > 0 || before.hits.length > 0)) {
      ids = before.ids
      terms = before.terms
      hits = before.hits
      total = before.total
      offset = before.offset
      reachable = before.reachable
      narrowed = before.narrowed
      stopRestore = restoreScroll(before)
      return
    }
    // What was written down carries the ids but not the passages a dialogue
    // search found: those are asked for again, and the store answers from its
    // own cache.
    if (before && params.scope === 'dialogue' && before.offset > 0) {
      void reload(before.offset, before)
      return
    }

    // Local scope needs something to search for; hitomi scope can browse.
    if (params.scope === 'local' && !params.query && params.language === 'all' && params.kind === 'all') return
    if (params.scope === 'dialogue' && !params.query) return
    void load(0)
  })

  /// Cancels the scroll restore that is still settling, if one is.
  let stopRestore: (() => void) | undefined

  // Whatever is still settling when this screen goes has to go with it.
  $effect(() => () => {
    stopRestore?.()
    stopRestore = undefined
  })

  /// Puts the page back where it was.
  ///
  /// Once is not enough: the cards below the fold have no height until they
  /// are drawn, and their covers arrive later still, so an early attempt runs
  /// out of page before it gets there. It keeps asking for a second while the
  /// page settles, and stops as soon as the reader takes over.
  function restoreScroll(want: Place): (() => void) | undefined {
    if (want.scrollY <= 0) return
    let settled = false
    const give = () => (settled = true)
    // Anything the reader does with the page is them saying where they want
    // to be, which is not this any more.
    addEventListener('wheel', give, { passive: true, once: true })
    addEventListener('touchstart', give, { passive: true, once: true })
    addEventListener('keydown', give, { once: true })

    const at = [0, 50, 150, 300, 500, 800, 1200]
    const timers = at.map((after) =>
      setTimeout(() => {
        if (!settled) settle(want)
      }, after),
    )
    const stop = () => {
      removeEventListener('wheel', give)
      removeEventListener('touchstart', give)
      removeEventListener('keydown', give)
      timers.forEach(clearTimeout)
    }
    const last = setTimeout(stop, 1400)
    // Handed back so leaving the screen cancels them. They used to outlive
    // it: tapping a result within the second put the gallery that replaced
    // this screen back to where the search had been scrolled to.
    return () => {
      clearTimeout(last)
      stop()
    }
  }

  /// One step back toward where the page was.
  ///
  /// The result that was at the top of the screen is put back where it sat,
  /// and the bare offset is only for when it cannot be found.
  function settle(want: Place) {
    const link = want.anchor && resultNamed(want.anchor.href)
    if (want.anchor && link) {
      const off = link.getBoundingClientRect().top - want.anchor.top
      if (Math.abs(off) > 8) scrollBy(0, off)
      return
    }
    if (Math.abs(window.scrollY - want.scrollY) > 8) scrollTo(0, want.scrollY)
  }

  const RESULT = 'main a[href^="#/g/"]'

  function resultNamed(href: string): HTMLElement | undefined {
    return [...document.querySelectorAll<HTMLElement>(RESULT)].find(
      (link) => link.getAttribute('href') === href,
    )
  }

  /// The first result still on screen, and where on it.
  function onScreen(): Anchor | undefined {
    for (const link of document.querySelectorAll<HTMLElement>(RESULT)) {
      const { top, bottom } = link.getBoundingClientRect()
      if (bottom > 0) return { href: link.getAttribute('href') ?? '', top }
    }
  }

  // Kept as it changes.
  $effect(() => {
    const asked = key
    if (sectioned || (ids.length === 0 && hits.length === 0)) return
    remember(asked, { ids, terms, hits, total, offset, reachable, narrowed, scrollY: 0 })
  })

  // Where the page is, written down as it moves. Never read on the way out:
  // by the time this screen is torn down its list has left the page, and the
  // browser answers with a position clamped to whatever replaced it - the
  // top, nearly every time, which put a reader coming back from a work
  // somewhere they had never been.
  $effect(() => {
    const asked = key
    if (sectioned) return
    let place: Place | undefined
    let frame = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    const onScroll = () => {
      frame ||= requestAnimationFrame(() => {
        frame = 0
        place = { scrollY: window.scrollY, anchor: onScreen() }
      })
      clearTimeout(timer)
      // On the disk as well, now rather than on the way out: a window the
      // phone takes away is never on its way out.
      timer = setTimeout(() => place && markScroll(asked, place), 300)
    }
    addEventListener('scroll', onScroll, { passive: true })
    return () => {
      cancelAnimationFrame(frame)
      clearTimeout(timer)
      removeEventListener('scroll', onScroll)
      if (place) markScroll(asked, place)
    }
  })

  /// Asks again for everything that had been loaded, in one go, and puts the
  /// page back where it was. For coming back to a list that was written down
  /// without the passages in it.
  async function reload(upTo: number, place: Place) {
    await load(0, Math.max(PAGE, upTo))
    stopRestore = restoreScroll(place)
  }

  async function load(from: number, want = PAGE) {
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
        const found = await api.dialogueSearch(params.query, from, want, controller.signal)
        hits = from === 0 ? found.hits : [...hits, ...found.hits]
        total = found.total
        reachable = found.reachable
        narrowed = found.narrowed
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
        ids = from === 0 ? page.ids : joined(ids, page.ids)
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
      narrowed = page.narrowed
      ids = from === 0 ? page.ids : joined(ids, page.ids)
      offset = from + page.ids.length
    } catch (cause) {
      if ((cause as Error).name !== 'AbortError') error = cause
    } finally {
      loading = false
    }
  }

  /// A page added to what is shown, without what is already there.
  ///
  /// The list can move between pages - hitomi is posted to all the time, and
  /// a program started again reads it afresh - and a work listed twice is
  /// not a repeat in a keyed list but an error that stops it drawing.
  function joined(shown: number[], more: number[]): number[] {
    const already = new Set(shown)
    return [...shown, ...more.filter((id) => !already.has(id))]
  }

  /// Asks hitomi the same question over again.
  ///
  /// Three caches sit between the reader and hitomi - the results on this
  /// page, the cards in this tab, the lists in the server - and together they
  /// are why paging is instant and why a screen left open is an hour out of
  /// date. A button rather than a timer: refetching under someone who is
  /// reading a list moves it.
  async function again() {
    forgetResults()
    forgetCards()
    try {
      await api.refresh()
    } catch {
      // The server not answering is the search's problem to report, not
      // this button's.
    }
    generation += 1
    if (sectioned) return
    await load(0)
    scrollTo(0, 0)
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
  /// What the results add up to, as one sentence - which is also what is
  /// read out when they change.
  const counted = $derived.by(() => {
    if (total > 0) {
      const said = [t('search.results', { n: number(total) })]
      if (params.scope === 'local') said.push(t('search.fromSnapshot'))
      if (narrowed) said.push(t('search.narrowed'))
      else if (!params.query && !filtering) said.push(t('search.browsingAll'))
      return said.join(' · ')
    }
    if (loading || error) return ''
    const said = [
      params.query ? t('search.noResultsFor', { query: params.query }) : t('search.noResults'),
    ]
    // The dialogue is only as big as what has been read, and a reader who has
    // read forty works is not looking at a broken search.
    if (params.scope === 'dialogue') said.push(t('search.readSoFar', { n: number(readSoFar) }))
    return said.join(' · ')
  })

  // 결과가 좁은데 인기순이면 목록을 크게 훑어야 한다(스펙 3.2절).
  const slowSort = $derived(loading && params.sort !== 'date' && total > 0 && total < 500)
</script>

<AppHeader active="search" />
<SearchBar
  {params}
  onchange={go}
  onreload={again}
  reloading={loading}
  {localAvailable}
  {dialogueAvailable}
/>

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
    {#key generation}
      <AllResults {params} {localAvailable} {dialogueAvailable} />
    {/key}
  {:else}
  <Terms {terms} />

  <!-- One paragraph, always in the page: a live region has to be there
       before its text changes, or the change is not announced. Submitting a
       search swapped the whole grid in silence. -->
  <p class="count" class:quiet={!counted} role="status">{counted}</p>

  {#if slowSort}
    <p class="count">{t('search.slowSort')}</p>
  {/if}

  {#if params.scope === 'dialogue'}
    <!-- Where it is worth the most: the results are thin, and the reason is
         that the corpus is not in yet. -->
    <CorpusOffer />
    {#key key}
      <DialogueHitList {hits} />
    {/key}
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
    padding: 1rem var(--gutter);
  }

  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }

  /* Still in the page so it can announce the next change; taking up none of
     it while it has nothing to say. */
  .count.quiet {
    margin: 0;
  }

  .more {
    margin: 1.5rem auto 0;
    display: block;
  }
</style>
