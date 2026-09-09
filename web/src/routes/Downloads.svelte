<script lang="ts">
  import * as api from '../lib/api'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'

  let items = $state<api.DownloadItem[]>([])
  let bytes = $state(0)
  let error = $state<unknown>(null)
  let loading = $state(true)

  $effect(() => {
    void load()
    // A running job changes what is on disk; keep the bars honest.
    const timer = setInterval(() => {
      if (items.some((i) => i.job.running)) void load()
    }, 2000)
    return () => clearInterval(timer)
  })

  async function load() {
    try {
      const list = await api.downloads()
      items = list.items
      bytes = list.bytes
      error = null
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }

  async function remove(id: number) {
    try {
      await api.removeDownload(id)
      await load()
    } catch (cause) {
      error = cause
    }
  }

  async function resume(id: number) {
    try {
      await api.startDownload(id)
      await load()
    } catch (cause) {
      error = cause
    }
  }

  const size = (n: number) =>
    n >= 1073741824
      ? `${(n / 1073741824).toFixed(1)} GB`
      : n >= 1048576
        ? `${(n / 1048576).toFixed(1)} MB`
        : `${Math.max(1, Math.round(n / 1024))} KB`
</script>

<AppHeader active="downloads" />

<main>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="muted">Loading...</p>
  {:else if items.length === 0}
    <p class="muted">
      Nothing downloaded. Open a work and use <strong>Download</strong> to keep it on disk;
      downloaded pages are read without touching hitomi.
    </p>
  {:else}
    <p class="muted">
      {items.length}
      {items.length === 1 ? 'work' : 'works'} &middot; {size(bytes)} on disk
    </p>
    <Grid>
      {#each items as item (item.id)}
        <div class="entry">
          <Card
            id={item.id}
            preset={{
              id: item.id,
              title: item.title,
              language: item.language,
              kind: null,
              pages: item.pages,
              thumbnail_hash: null,
            }}
          />
          <div class="bar" style:--done={`${item.pages ? (item.have / item.pages) * 100 : 0}%`}>
            <span>
              {item.have} / {item.pages}
              {#if item.job.running}&middot; downloading{/if}
              {#if item.job.failed}&middot; {item.job.failed} failed{/if}
            </span>
          </div>
          <div class="actions">
            {#if !item.complete && !item.job.running}
              <button onclick={() => resume(item.id)}>Get the rest</button>
            {/if}
            <button onclick={() => remove(item.id)}>Delete</button>
          </div>
        </div>
      {/each}
    </Grid>
  {/if}
</main>

<style>
  main {
    padding: 1rem;
  }
  .muted {
    color: var(--muted);
    margin: 0 0 1rem;
  }
  .entry {
    display: grid;
    gap: 0.35rem;
  }
  .bar {
    position: relative;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 0.75rem;
    padding: 0.15rem 0.4rem;
    overflow: hidden;
  }
  .bar::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--done);
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .bar span {
    position: relative;
  }
  .actions {
    display: flex;
    gap: 0.35rem;
  }
  .actions button {
    font-size: 0.75rem;
    padding: 0.15rem 0.45rem;
  }
</style>
