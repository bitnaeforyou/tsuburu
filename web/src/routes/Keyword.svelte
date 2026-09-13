<script lang="ts">
  import * as api from '../lib/api'
  import { number, t } from '../lib/i18n.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'

  let { word }: { word: string } = $props()

  let ids = $state<number[]>([])
  let tooCommon = $state<number | null>(null)
  let loading = $state(true)
  let error = $state<unknown>(null)

  $effect(() => {
    void load(word)
  })

  async function load(term: string) {
    loading = true
    error = null
    try {
      const found = await api.keywordSearch(term, 50)
      ids = found.works.map((w) => w.id)
      tooCommon = found.too_common ?? null
    } catch (cause) {
      error = cause
    } finally {
      loading = false
    }
  }
</script>

<AppHeader active="search" />

<main>
  <h1>{word}</h1>
  <p class="muted">{t('keyword.about')}</p>

  {#if error}
    <ErrorNote {error} onretry={() => load(word)} />
  {/if}

  <Grid>
    {#each ids as id (id)}
      <Card {id} />
    {/each}
  </Grid>

  {#if loading}
    <p class="muted">{t('common.loading')}</p>
  {:else if ids.length === 0 && !error}
    <p class="muted">
      {tooCommon === null
        ? t('keyword.empty')
        : t('keyword.tooCommon', { n: number(tooCommon) })}
    </p>
  {/if}
</main>

<style>
  main {
    max-width: var(--page);
    margin-inline: auto;
    padding: 1rem;
  }
  h1 {
    margin: 0;

  }
  .muted {
    color: var(--muted);
    margin: 0.25rem 0 1rem;
  }
</style>
