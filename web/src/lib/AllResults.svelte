<script lang="ts">
  import * as api from './api'
  import { t, number } from './i18n.svelte'
  import { toSearch, type SearchState } from './router'
  import Card from './Card.svelte'
  import DialogueHitList from './DialogueHitList.svelte'
  import Grid from './Grid.svelte'

  // 한 검색어를 세 곳에 동시에 던지고, 도착하는 대로 구획을 채운다. 태그
  // 교집합과 대사 유사도는 같은 자로 잴 수 없으므로 한 줄로 섞지 않는다.
  let {
    params,
    localAvailable,
    dialogueAvailable,
  }: {
    params: SearchState
    localAvailable: boolean
    dialogueAvailable: boolean
  } = $props()

  const PREVIEW = 12
  // Dialogue rows are tall - a card beside a quote - so a long preview would
  // push the other sections off the screen.
  const DIALOGUE_PREVIEW = 5

  type Section<T> = {
    state: 'loading' | 'done' | 'failed'
    total: number
    /** The source gave as many as were asked for, so there are likely more. */
    capped?: boolean
    items: T[]
  }

  let dialogue = $state<Section<api.DialogueHit>>({ state: 'loading', total: 0, items: [] })
  let titles = $state<Section<number>>({ state: 'loading', total: 0, items: [] })
  let tags = $state<Section<number>>({ state: 'loading', total: 0, items: [] })

  let controller: AbortController | null = null

  $effect(() => {
    const key = [params.query, params.language, params.kind].join(' ')
    void key
    controller?.abort()
    controller = new AbortController()
    const signal = controller.signal
    dialogue = { state: dialogueAvailable ? 'loading' : 'done', total: 0, items: [] }
    titles = { state: localAvailable ? 'loading' : 'done', total: 0, items: [] }
    tags = { state: 'loading', total: 0, items: [] }

    if (dialogueAvailable) void loadDialogue(signal)
    if (localAvailable) void loadTitles(signal)
    void loadTags(signal)
    return () => controller?.abort()
  })

  async function loadDialogue(signal: AbortSignal) {
    try {
      // One more than shown, purely to know whether to say "at least".
      const found = await api.dialogueSearch(params.query, DIALOGUE_PREVIEW + 1, signal)
      dialogue = {
        state: 'done',
        total: Math.min(found.hits.length, DIALOGUE_PREVIEW),
        capped: found.hits.length > DIALOGUE_PREVIEW,
        items: found.hits.slice(0, DIALOGUE_PREVIEW),
      }
    } catch {
      dialogue = { state: 'failed', total: 0, items: [] }
    }
  }

  async function loadTitles(signal: AbortSignal) {
    try {
      const page = await api.metaSearch(
        {
          q: params.query,
          offset: 0,
          limit: PREVIEW,
          language: params.language,
          kind: params.kind,
        },
        signal,
      )
      titles = { state: 'done', total: page.total, items: page.ids }
    } catch {
      titles = { state: 'failed', total: 0, items: [] }
    }
  }

  async function loadTags(signal: AbortSignal) {
    try {
      const page = await api.search(
        {
          q: params.query,
          offset: 0,
          limit: PREVIEW,
          sort: params.sort,
          language: params.language,
          kind: params.kind,
        },
        signal,
      )
      tags = { state: 'done', total: page.total, items: page.ids }
    } catch {
      tags = { state: 'failed', total: 0, items: [] }
    }
  }

  const empty = $derived(
    dialogue.state !== 'loading' &&
      titles.state !== 'loading' &&
      tags.state !== 'loading' &&
      dialogue.items.length === 0 &&
      titles.items.length === 0 &&
      tags.items.length === 0,
  )
</script>

{#snippet heading(
  label: string,
  section: { state: string; total: number; capped?: boolean },
  href: string,
)}
  <div class="head">
    <strong>{label}</strong>
    {#if section.state === 'loading'}
      <span class="muted">{t('common.loading')}</span>
    {:else if section.state === 'failed'}
      <span class="muted">{t('search.sectionFailed')}</span>
    {:else if section.total > 0}
      <span class="muted">
        {section.capped
          ? t('search.atLeast', { n: number(section.total) })
          : t('search.results', { n: number(section.total) })}
      </span>
      <a class="more" {href}>{t('search.narrow')}</a>
    {/if}
  </div>
{/snippet}

{#if dialogueAvailable && (dialogue.state === 'loading' || dialogue.items.length)}
  <section>
    {@render heading(t('nav.dialogue'), dialogue, toSearch({ ...params, scope: 'dialogue' }))}
    <DialogueHitList hits={dialogue.items} />
  </section>
{/if}

{#if localAvailable && (titles.state === 'loading' || titles.items.length)}
  <section>
    {@render heading(t('search.scopeLocal'), titles, toSearch({ ...params, scope: 'local' }))}
    <Grid>
      {#each titles.items as id (id)}
        <Card {id} />
      {/each}
    </Grid>
  </section>
{/if}

<section>
  {@render heading(t('search.scopeHitomi'), tags, toSearch({ ...params, scope: 'hitomi' }))}
  <Grid>
    {#each tags.items as id (id)}
      <Card {id} />
    {/each}
  </Grid>
</section>

{#if empty}
  <p class="muted">{t('search.noResultsFor', { query: params.query })}</p>
{/if}

<style>
  section {
    margin-bottom: 2rem;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    margin-bottom: 0.7rem;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .more {
    margin-left: auto;
    font-size: 0.85rem;
    text-decoration: none;
  }
</style>
