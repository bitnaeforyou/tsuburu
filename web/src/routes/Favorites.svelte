<script lang="ts">
  import * as api from '../lib/api'
  import { t, number, type Key } from '../lib/i18n.svelte'
  import { library } from '../lib/library.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { keepScroll } from '../lib/keepScroll.svelte'
  import ViewToggle from '../lib/ViewToggle.svelte'
  import { toArtist } from '../lib/router'
  import Shelves from '../lib/Shelves.svelte'
  import ShelfPicker from '../lib/ShelfPicker.svelte'
  import { onShelf, type Picked } from '../lib/folders'

  let items = $state<api.Favorite[]>([])
  let artists = $state<api.FollowedArtist[]>([])
  let error = $state<unknown>(null)
  let loading = $state(true)

  type Order = 'added' | 'title' | 'pages'
  let shelf = $state<Picked>('all')
  let order = $state<Order>('added')

  const ORDERS: { value: Order; key: Key }[] = [
    { value: 'added', key: 'sort.added' },
    { value: 'title', key: 'sort.title' },
    { value: 'pages', key: 'sort.pages' },
  ]

  // Coming back out of a work should land where the list was left.
  $effect(() => keepScroll('favorites', () => visible.length > 0))

  $effect(() => {
    void load()
  })

  async function load() {
    loading = true
    error = null
    try {
      const [{ items: favorites }] = await Promise.all([api.favorites(), library.load()])
      items = favorites
      // The shelves are the library's, not this list's: one with nothing on
      // it is still a shelf.
      shelves = await api.folders().catch(() => [])
      // Following is disabled without a snapshot; an empty list is fine.
      artists = await api.followedArtists().catch(() => [])
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }

  // 별을 끄면 목록에서 바로 사라져야 한다. 서버를 다시 묻지 않고 화면에서 뺀다.
  const starred = $derived(items.filter((item) => library.has(item.id)))

  let shelves = $state<api.Folder[]>([])
  const shelfNames = $derived(shelves.map((shelf) => shelf.name))

  const visible = $derived.by(() => {
    const sorted = [...onShelf(starred, shelf)]
    if (order === 'title') {
      sorted.sort((a, b) => (a.title ?? '').localeCompare(b.title ?? ''))
    } else if (order === 'pages') {
      sorted.sort((a, b) => b.pages - a.pages)
    }
    // `added` is the order the server already sends: newest first.
    return sorted
  })

  async function move(id: number, to: string | null) {
    const was = items.find((each) => each.id === id)?.folder ?? null
    // Shown before the server answers, and put back if it refuses.
    items = items.map((each) => (each.id === id ? { ...each, folder: to } : each))
    try {
      await api.setFolder(id, to)
      // Filing on a shelf nobody made makes the shelf; the counts move too.
      shelves = await api.folders().catch(() => shelves)
    } catch (cause) {
      items = items.map((each) => (each.id === id ? { ...each, folder: was } : each))
      error = cause
    }
  }
</script>

<AppHeader active="favorites" />

<main>
  <h1>{t('nav.favorites')}</h1>
  {#if artists.length}
    <section class="artists">
      <h2>{t('favorites.artists')}</h2>
      <ul>
        {#each artists as artist (artist.name)}
          <li>
            <a href={toArtist(artist.name)}>{artist.name}</a>
            <span class="count">{number(artist.works)}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="count">{t('common.loading')}</p>
  {:else if visible.length === 0}
    <p class="count">{t('favorites.empty')}</p>
  {:else}
    <div class="tidy">
      <Shelves works={starred} {shelves} bind:picked={shelf} onchange={(next) => (shelves = next)} />
      <div class="how">
        <ViewToggle />
        <label class="order">
        <span>{t('sort.by')}</span>
        <select bind:value={order}>
          {#each ORDERS as option (option.value)}
            <option value={option.value}>{t(option.key)}</option>
          {/each}
          </select>
        </label>
      </div>
    </div>

    <p class="count">{t('favorites.count', { n: visible.length })}</p>
    <Grid>
      {#each visible as item (item.id)}
        <div class="filed">
          <Card id={item.id} preset={item} />
          <ShelfPicker
            id={item.id}
            folder={item.folder}
            title={item.title ?? `#${item.id}`}
            shelves={shelfNames}
            onmove={(id, to) => void move(id, to)}
          />
        </div>
      {/each}
    </Grid>
  {/if}
</main>

<style>
  h1 {
    margin: 0.25rem 0 1rem;
  }
  main {
    max-width: var(--page);
    margin-inline: auto;
    padding: 1rem;
  }
  .artists {
    margin-bottom: 1.25rem;
  }
  .artists h2 {
    font-size: var(--text-base);
    margin: 0;
  }
  .artists ul {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    padding: 0;
    margin: 0.5rem 0 0;
  }
  .artists li {
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0.2rem 0.7rem;
    font-size: var(--text-sm);
  }
  .artists a {
    text-decoration: none;
  }
  .artists .count {
    color: var(--muted);
    margin-inline-start: 0.4rem;
  }
  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }

  .tidy {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem 1rem;
    margin-bottom: 0.9rem;
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

  .filed {
    display: grid;
    gap: 0.35rem;
  }
  </style>
