<script lang="ts">
  import * as api from '../lib/api'
  import { t } from '../lib/i18n.svelte'
  import { library } from '../lib/library.svelte'
  import Card from '../lib/Card.svelte'
  import Grid from '../lib/Grid.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'

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

<AppHeader active="history">
  {#snippet actions()}
    {#if items.length > 0}
      {#if confirming}
        <span class="confirm">
          {t('history.confirm')}
          <button onclick={clear}>{t('history.confirmYes')}</button>
          <button onclick={() => (confirming = false)}>{t('common.cancel')}</button>
        </span>
      {:else}
        <button onclick={() => (confirming = true)}>{t('history.clear')}</button>
      {/if}
    {/if}
  {/snippet}
</AppHeader>

<main>
  <h1>{t('nav.history')}</h1>
  {#if error}
    <ErrorNote {error} onretry={load} />
  {:else if loading}
    <p class="count">{t('common.loading')}</p>
  {:else if items.length === 0}
    <p class="count">{t('history.empty')}</p>
  {:else}
    <p class="count">{t('history.count', { n: items.length })}</p>
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
  h1 {
    margin: 0.25rem 0 1rem;
  }
  .confirm {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    color: var(--muted);
    font-size: var(--text-md);
  }
  main {
    max-width: var(--page);
    margin-inline: auto;
    padding: 1rem;
  }
  .count {
    color: var(--muted);
    margin: 0 0 1rem;
  }
</style>
