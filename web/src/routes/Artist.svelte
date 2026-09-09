<script lang="ts">
  import * as api from '../lib/api'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'

  let { artist }: { artist: string } = $props()

  const PAGE = 25

  let info = $state<api.ArtistResponse | null>(null)
  let ids = $state<number[]>([])
  let language = $state('all')
  let loading = $state(false)
  let error = $state<unknown>(null)

  $effect(() => {
    const key = `${artist}:${language}`
    void key
    ids = []
    info = null
    void load(0)
  })

  async function load(from: number) {
    loading = true
    error = null
    try {
      const page = await api.artist(artist, from, PAGE, language)
      info = page
      ids = from === 0 ? page.ids : [...ids, ...page.ids]
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }

  async function toggleFollow() {
    if (!info) return
    try {
      const r = info.following
        ? await api.unfollowArtist(info.name)
        : await api.followArtist(info.name)
      info = { ...info, following: r.following }
    } catch (cause) {
      error = cause
    }
  }

  const hasMore = $derived(info !== null && ids.length < info.total)
</script>

<AppHeader active="search" />

<main>
  <header class="artist">
    <div>
      <h1>{artist}</h1>
      {#if info}
        <p class="muted">
          {info.total.toLocaleString()} works
          {#if info.languages.length}
            &middot; {info.languages.map(([l, n]) => `${l} ${n}`).join(' · ')}
          {/if}
        </p>
      {/if}
    </div>
    {#if info}
      <button class:on={info.following} onclick={toggleFollow}>
        {info.following ? '★ Following' : '☆ Follow'}
      </button>
    {/if}
  </header>

  <label class="filter">
    Language
    <select bind:value={language}>
      <option value="all">all</option>
      {#each info?.languages ?? [] as [name, count] (name)}
        <option value={name}>{name} ({count})</option>
      {/each}
    </select>
  </label>

  {#if error}
    <ErrorNote {error} onretry={() => load(0)} />
  {/if}

  <Grid>
    {#each ids as id (id)}
      <Card {id} />
    {/each}
  </Grid>

  {#if loading}
    <p class="muted">Loading...</p>
  {:else if ids.length === 0 && !error}
    <p class="muted">Nothing by that name in the local snapshot.</p>
  {/if}

  {#if hasMore && !loading}
    <button class="more" onclick={() => load(ids.length)}>Load more</button>
  {/if}
</main>

<style>
  main {
    padding: 1rem;
  }
  .artist {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.2rem;
  }
  .muted {
    color: var(--muted);
    margin: 0.25rem 0 0;
  }
  button.on {
    color: var(--accent);
    border-color: var(--accent);
  }
  .filter {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--muted);
    font-size: 0.85rem;
    margin-bottom: 1rem;
  }
  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
  }
  .more {
    margin: 1.5rem auto 0;
    display: block;
  }
</style>
