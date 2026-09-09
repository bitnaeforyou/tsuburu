<script lang="ts">
  import { parse, type Route } from './lib/router'
  import Search from './routes/Search.svelte'
  import Gallery from './routes/Gallery.svelte'
  import Favorites from './routes/Favorites.svelte'
  import History from './routes/History.svelte'
  import Dialogue from './routes/Dialogue.svelte'
  import Artist from './routes/Artist.svelte'
  import Downloads from './routes/Downloads.svelte'
  import AgeGate from './lib/AgeGate.svelte'

  let route = $state<Route>(parse(location.hash))
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
{:else if route.name === 'dialogue'}
  <Dialogue query={route.query} />
{:else if route.name === 'artist'}
  <Artist artist={route.artist} />
{:else if route.name === 'downloads'}
  <Downloads />
{:else}
  <Search params={route} />
{/if}
