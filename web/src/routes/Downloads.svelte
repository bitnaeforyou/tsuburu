<script lang="ts">
  import * as api from '../lib/api'
  import { downloaded } from '../lib/library.svelte'
  import { t, type Key } from '../lib/i18n.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import CopyNumber from '../lib/CopyNumber.svelte'
  import { copy } from '../lib/clipboard'
  import { galleriesNamed } from '../lib/router'
  import Grid from '../lib/Grid.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { keepScroll } from '../lib/keepScroll.svelte'
  import ViewToggle from '../lib/ViewToggle.svelte'
  import Shelves from '../lib/Shelves.svelte'
  import ShelfPicker from '../lib/ShelfPicker.svelte'
  import { onShelf, type Picked, moveTo } from '../lib/folders'

  let items = $state<api.DownloadItem[]>([])
  let bytes = $state(0)
  let error = $state<unknown>(null)
  let loading = $state(true)

  type Order = 'added' | 'title' | 'pages' | 'bytes' | 'idDesc' | 'idAsc'
  let order = $state<Order>('added')
  let shelf = $state<Picked>('all')

  let shelves = $state<api.Folder[]>([])
  const shelfNames = $derived(shelves.map((shelf) => shelf.name))

  const move = (id: number, to: string | null) =>
    moveTo(
      items,
      id,
      to,
      (shown) => (items = shown),
      (found) => (shelves = found),
      (cause) => (error = cause),
    )

  const ORDERS: { value: Order; key: Key }[] = [
    { value: 'added', key: 'sort.added' },
    { value: 'title', key: 'sort.title' },
    { value: 'pages', key: 'sort.pages' },
    { value: 'bytes', key: 'downloads.biggest' },
    { value: 'idDesc', key: 'sort.idDesc' },
    { value: 'idAsc', key: 'sort.idAsc' },
  ]

  /// What copying every number came to, said beside the button.
  let copied = $state('')

  /// The numbers of what is on screen - this shelf, in this order - one to a
  /// line, which is what every box in tsuburu that takes numbers accepts.
  async function copyNumbers() {
    const ids = ordered.map((item) => item.id)
    if (!(await copy(ids.join('\n')))) copied = t('copy.failed')
    else if (ids.length === 1) copied = t('copy.allDoneOne')
    else copied = t('copy.allDone', { n: ids.length })
  }

  const ordered = $derived.by(() => {
    const sorted = [...onShelf(items, shelf)]
    if (order === 'title') sorted.sort((a, b) => (a.title ?? '').localeCompare(b.title ?? ''))
    else if (order === 'pages') sorted.sort((a, b) => b.pages - a.pages)
    else if (order === 'bytes') sorted.sort((a, b) => b.bytes - a.bytes)
    else if (order === 'idDesc') sorted.sort((a, b) => b.id - a.id)
    else if (order === 'idAsc') sorted.sort((a, b) => a.id - b.id)
    else sorted.sort((a, b) => b.added_at - a.added_at)
    return sorted
  })

  // Coming back out of a work should land where the list was left.
  $effect(() => keepScroll('downloads', () => ordered.length > 0))

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
      downloaded.replace(list.items)
      bytes = list.bytes
      shelves = await api.folders().catch(() => [])
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

  let asked = $state('')
  let asking = $state(false)
  /// What the last list of numbers came to, said under the box.
  let started = $state('')

  /// Starts every work a pasted list names, one after another, and says how
  /// that went: started, already here, or refused and why.
  async function downloadNumbers(event: SubmitEvent) {
    event.preventDefault()
    const { ids, unknown } = galleriesNamed(asked)
    if (ids.length === 0) {
      started = t('downloads.queuedNone')
      return
    }
    asking = true
    const had = new Set(items.filter((item) => item.complete).map((item) => item.id))
    // Asked before the server is, which would refuse them in English.
    const busy = new Set(items.filter((item) => item.job.running).map((item) => item.id))
    const said: string[] = []
    const failed: string[] = []
    let begun = 0
    for (const id of ids) {
      if (had.has(id) || busy.has(id)) continue
      try {
        await api.startDownload(id)
        begun += 1
      } catch (cause) {
        failed.push(`#${id} (${cause instanceof Error ? cause.message : String(cause)})`)
      }
    }
    if (begun) said.push(t('downloads.queued', { n: begun }))
    const already = ids.filter((id) => had.has(id)).length
    if (already) said.push(t('downloads.queuedHad', { n: already }))
    const running = ids.filter((id) => busy.has(id)).length
    if (running) said.push(t('downloads.queuedBusy', { n: running }))
    if (failed.length) said.push(t('downloads.queuedFailed', { list: failed.join(', ') }))
    if (unknown.length) said.push(t('downloads.queuedUnknown', { list: unknown.join(', ') }))
    started = said.join(' · ')
    if (begun) asked = ''
    asking = false
    await load()
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
  <!-- Above everything else, and there with nothing downloaded yet: it is
       how a first download can start from here. -->
  <form class="by-number" onsubmit={downloadNumbers}>
    <input
      bind:value={asked}
      placeholder="4183648, 4183650"
      aria-label={t('downloads.byNumber')}
      inputmode="text"
      autocomplete="off"
    />
    <button type="submit" disabled={asking || !asked.trim()}>{t('downloads.byNumberButton')}</button>
  </form>
  <p class="muted started" class:quiet={!started} role="status">{started}</p>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="muted">{t('common.loading')}</p>
  {:else if items.length === 0}
    <p class="muted">{t('downloads.empty')}</p>
  {:else}
    <Shelves works={items} {shelves} bind:picked={shelf} onchange={(next) => (shelves = next)} />
    <div class="tidy">
      <p class="muted">
        {items.length === 1 ? t('downloads.work') : t('downloads.works', { n: items.length })}
        &middot; {t('downloads.onDisk', { size: size(bytes) })}
      </p>
      <div class="how">
        <button onclick={copyNumbers}>{t('copy.all')}</button>
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
    <p class="muted copied" class:quiet={!copied} role="status">{copied}</p>
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
            <CopyNumber id={item.id} label={t('copy.one')} />
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
    padding: 1rem var(--gutter);
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
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  /* Always in the page so the next copy is announced; taking no room while
     it has nothing to say. */
  .copied.quiet,
  .started.quiet {
    margin: 0;
  }
  .by-number {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .by-number input {
    flex: 1;
    min-width: 0;
  }
  .started {
    overflow-wrap: anywhere;
  }
  /* `:global` for the one that is a component of its own: scoped styles
     stop at its edge, and it came out a size larger than its neighbours. */
  .actions :global(button) {
    font-size: var(--text-sm);
    padding: 0.15rem 0.45rem;
  }
</style>
