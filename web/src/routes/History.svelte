<script lang="ts">
  import * as api from '../lib/api'
  import { library } from '../lib/library.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import Nav from '../lib/Nav.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { toSearch } from '../lib/router'

  let items = $state<api.HistoryEntry[]>([])
  let error = $state<unknown>(null)
  let loading = $state(true)
  let confirming = $state(false)

  $effect(() => {
    void load()
  })

  async function load() {
    loading = true
    error = null
    try {
      const [{ items: entries }] = await Promise.all([api.history(), library.load()])
      items = entries
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }

  async function clear() {
    try {
      await api.clearHistory()
      items = []
    } catch (cause) {
      error = cause
    } finally {
      confirming = false
    }
  }
</script>

<header>
  <a class="brand" href={toSearch()}>tsuburu</a>
  <h1>History</h1>
  {#if items.length > 0}
    {#if confirming}
      <span class="confirm">
        Clear everything?
        <button onclick={clear}>Yes, clear</button>
        <button onclick={() => (confirming = false)}>Cancel</button>
      </span>
    {:else}
      <button onclick={() => (confirming = true)}>Clear</button>
    {/if}
  {/if}
  <Nav active="history" />
</header>

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="count">Loading...</p>
  {:else if items.length === 0}
    <p class="count">Nothing read yet.</p>
  {:else}
    <p class="count">{items.length} recently read</p>
    <Grid>
      {#each items as item (item.id)}
        <Card
          id={item.id}
          preset={item}
          progress={{ page: item.last_page, pages: item.pages }}
        />
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
  .confirm {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    color: var(--muted);
    font-size: 0.85rem;
  }
  main {
    padding: 1rem;
  }
  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }
</style>
