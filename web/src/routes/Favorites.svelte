<script lang="ts">
  import * as api from '../lib/api'
  import { library } from '../lib/library.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'

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

<AppHeader active="favorites" />

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
  main {
    padding: 1rem;
  }
  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }
</style>
