<script lang="ts">
  import { parse, type Route } from './lib/router'
  import Search from './routes/Search.svelte'
  import Gallery from './routes/Gallery.svelte'
  import Favorites from './routes/Favorites.svelte'
  import History from './routes/History.svelte'
  import Settings from './routes/Settings.svelte'
  import Artist from './routes/Artist.svelte'
  import Downloads from './routes/Downloads.svelte'
  import Keyword from './routes/Keyword.svelte'
  import AgeGate from './lib/AgeGate.svelte'
  import CorpusOffer from './lib/CorpusOffer.svelte'
  import { i18n } from './lib/i18n.svelte'

  let route = $state<Route>(parse(location.hash))

  // Screen readers and hyphenation both go by this.
  $effect(() => {
    document.documentElement.lang = i18n.locale
  })
  let confirmed = $state(localStorage.getItem('tsuburu.age') === 'ok')

  $effect(() => {
    const onHash = () => (route = parse(location.hash))
    addEventListener('hashchange', onHash)
    return () => removeEventListener('hashchange', onHash)
  })
</script>

{#if !confirmed}
  <AgeGate onconfirm={() => (confirmed = true)} />
{:else if route.name === 'gallery'}
  <Gallery id={route.id} startPage={route.page} />
{:else if route.name === 'favorites'}
  <Favorites />
{:else if route.name === 'history'}
  <History />
{:else if route.name === 'settings'}
  <Settings />
{:else if route.name === 'artist'}
  <Artist artist={route.artist} />
{:else if route.name === 'downloads'}
  <Downloads />
{:else if route.name === 'keyword'}
  <Keyword word={route.word} />
{:else}
  <!-- The one time a reader is certain to see it, before they have gone
       looking for anything: the dialogue of a hundred thousand works is a
       button away, and the question is asked once. -->
  <CorpusOffer ask />
  <Search params={route} />
{/if}
