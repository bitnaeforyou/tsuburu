<script lang="ts">
  import { parse, type Route } from './lib/router'
  import Search from './routes/Search.svelte'
  import Gallery from './routes/Gallery.svelte'
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
  <Gallery id={route.id} />
{:else}
  <Search query={route.query} />
{/if}
