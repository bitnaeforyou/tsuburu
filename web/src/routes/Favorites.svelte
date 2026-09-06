<script lang="ts">
  import * as api from '../lib/api'
  import { library } from '../lib/library.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import Nav from '../lib/Nav.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { toSearch } from '../lib/router'

  let items = $state<api.Favorite[]>([])
  let error = $state<unknown>(null)
  let loading = $state(true)

  $effect(() => {
    void load()
  })

  async function load() {
    loading = true
    error = null
    try {
      const [{ items: favorites }] = await Promise.all([api.favorites(), library.load()])
      items = favorites
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }

  // 별을 끄면 목록에서 바로 사라져야 한다. 서버를 다시 묻지 않고 화면에서 뺀다.
  const visible = $derived(items.filter((item) => library.has(item.id)))
</script>

<header>
  <a class="brand" href={toSearch()}>tsuburu</a>
  <h1>Favorites</h1>
  <Nav active="favorites" />
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="count">Loading...</p>
  {:else if visible.length === 0}
    <p class="count">
      No favorites yet. Tap the star on any result to keep it here.
    </p>
  {:else}
    <p class="count">{visible.length} saved</p>
    <Grid>
      {#each visible as item (item.id)}
        <Card id={item.id} preset={item} />
      {/each}
    </Grid>
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
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 1rem;
    font-weight: 500;
  }
  main {
    padding: 1rem;
  }
  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }
</style>
