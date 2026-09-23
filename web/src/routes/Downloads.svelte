<script lang="ts">
  import * as api from '../lib/api'
  import { t, type Key } from '../lib/i18n.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import ViewToggle from '../lib/ViewToggle.svelte'
  import Shelves from '../lib/Shelves.svelte'
  import ShelfPicker from '../lib/ShelfPicker.svelte'
  import { onShelf, shelvesOf, type Picked } from '../lib/folders'

  let items = $state<api.DownloadItem[]>([])
  let bytes = $state(0)
  let error = $state<unknown>(null)
  let loading = $state(true)

  type Order = 'added' | 'title' | 'pages' | 'bytes'
  let order = $state<Order>('added')
  let shelf = $state<Picked>('all')

  const shelfNames = $derived(shelvesOf(items).map(([name]) => name))

  async function move(id: number, to: string | null) {
    const was = items.find((each) => each.id === id)?.folder ?? null
    // Shown before the server answers, and put back if it refuses.
    items = items.map((each) => (each.id === id ? { ...each, folder: to } : each))
    try {
      await api.setFolder(id, to)
    } catch (cause) {
      items = items.map((each) => (each.id === id ? { ...each, folder: was } : each))
      error = cause
    }
  }

  const ORDERS: { value: Order; key: Key }[] = [
    { value: 'added', key: 'sort.added' },
    { value: 'title', key: 'sort.title' },
    { value: 'pages', key: 'sort.pages' },
    { value: 'bytes', key: 'downloads.biggest' },
  ]

  const ordered = $derived.by(() => {
    const sorted = [...onShelf(items, shelf)]
    if (order === 'title') sorted.sort((a, b) => (a.title ?? '').localeCompare(b.title ?? ''))
    else if (order === 'pages') sorted.sort((a, b) => b.pages - a.pages)
    else if (order === 'bytes') sorted.sort((a, b) => b.bytes - a.bytes)
    else sorted.sort((a, b) => b.added_at - a.added_at)
    return sorted
  })

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
  <h1>{t('nav.downloads')}</h1>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="muted">{t('common.loading')}</p>
  {:else if items.length === 0}
    <p class="muted">{t('downloads.empty')}</p>
  {:else}
    <Shelves works={items} bind:picked={shelf} />
    <div class="tidy">
      <p class="muted">
        {items.length === 1 ? t('downloads.work') : t('downloads.works', { n: items.length })}
        &middot; {t('downloads.onDisk', { size: size(bytes) })}
      </p>
      <div class="how">
        <ViewToggle />
        {#if items.length > 1}
          <label class="order">
            <span>{t('sort.by')}</span>
            <select bind:value={order}>
              {#each ORDERS as option (option.value)}
                <option value={option.value}>{t(option.key)}</option>
              {/each}
            </select>
          </label>
        {/if}
      </div>
    </div>
    <Grid>
      {#each ordered as item (item.id)}
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
              {#if item.job.running}&middot; {t('gallery.downloading')}{/if}
              {#if item.job.failed}&middot; {t('downloads.failed', { n: item.job.failed })}{/if}
            </span>
          </div>
          <ShelfPicker
            id={item.id}
            folder={item.folder}
            title={item.title ?? `#${item.id}`}
            shelves={shelfNames}
            onmove={(id, to) => void move(id, to)}
          />
          <div class="actions">
            {#if !item.complete && !item.job.running}
              <button onclick={() => resume(item.id)}>{t('gallery.getRest')}</button>
            {/if}
            <button onclick={() => remove(item.id)}>{t('downloads.delete')}</button>
          </div>
        </div>
      {/each}
    </Grid>
  {/if}
</main>

<style>
  .tidy {
    margin-top: 0.6rem;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.4rem 1rem;
  }
  .how {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .order {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--muted);
    font-size: var(--text-md);
  }

  h1 {
    margin: 0.25rem 0 1rem;
  }
  main {
    max-width: var(--page);
    margin-inline: auto;
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
    border: 1px solid var(--line);
    border-radius: var(--radius);
    font-size: var(--text-xs);
    /* The count climbs while the job runs; digits that change width make
       the bar twitch. */
    font-variant-numeric: tabular-nums;
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
    font-size: var(--text-sm);
    padding: 0.15rem 0.45rem;
  }
</style>
