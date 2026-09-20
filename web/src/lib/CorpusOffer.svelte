<script lang="ts">
  import * as api from './api'
  import { t, number } from './i18n.svelte'

  let {
    /// Whether saying no should be remembered.
    ///
    /// The offer on the way in is asked once; a reader who said no is not
    /// asked again on every start. The one beside dialogue results is a
    /// standing suggestion at the moment it is most use - the results are
    /// thin because the corpus is not in - so it stays.
    ask = false,
  }: { ask?: boolean } = $props()

  const ASKED = 'tsuburu.corpus.asked'

  let corpus = $state<api.CorpusState | null>(null)
  let dismissed = $state(false)

  function remember() {
    if (!ask) return
    try {
      localStorage.setItem(ASKED, 'yes')
    } catch {
      // A private window forgets; being asked twice is not a fault.
    }
  }

  function asked(): boolean {
    if (!ask) return false
    try {
      return localStorage.getItem(ASKED) === 'yes'
    } catch {
      return false
    }
  }

  $effect(() => {
    if (asked()) {
      dismissed = true
      return
    }
    void look()
    const timer = setInterval(() => void look(), 1000)
    return () => clearInterval(timer)
  })

  async function look() {
    try {
      corpus = await api.corpusState()
    } catch {
      dismissed = true
    }
  }

  async function yes() {
    remember()
    try {
      corpus = await api.fetchCorpus()
    } catch (cause) {
      corpus = {
        available: true,
        state: 'failed',
        error: cause instanceof Error ? cause.message : String(cause),
      }
    }
  }

  function no() {
    remember()
    dismissed = true
  }

  // Nothing to offer if the build cannot fetch, or the corpus is already in.
  const offering = $derived(
    !dismissed && corpus?.available === true && corpus.state !== 'ready',
  )
</script>

{#if offering && corpus}
  <div class="offer" role="status">
    <div class="what">
      <strong>{t('corpus.title')}</strong>
      <p>{t('corpus.note')}</p>
      {#if corpus.state === 'failed'}
        <p class="bad">{t('corpus.failed', { error: corpus.error ?? '' })}</p>
      {/if}
    </div>

    {#if corpus.state === 'fetching'}
      <div class="meter" style:--done={`${((corpus.done ?? 0) / Math.max(corpus.total ?? 1, 1)) * 100}%`}>
        <span>
          {t('corpus.fetching', {
            done: corpus.done ?? 0,
            total: corpus.total ?? 0,
            works: number(corpus.works ?? 0),
          })}
        </span>
      </div>
      <button onclick={() => (dismissed = true)}>{t('corpus.background')}</button>
    {:else}
      <div class="answers">
        <button class="primary" onclick={yes}>
          {corpus.state === 'failed' ? t('corpus.again') : t('corpus.get')}
        </button>
        {#if ask}
          <button onclick={no}>{t('corpus.later')}</button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .offer {
    max-width: var(--page);
    margin: 1rem auto 0;
    padding: 0.9rem 1rem;
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    display: grid;
    gap: 0.6rem;
  }
  .what p {
    margin: 0.35rem 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
    max-width: 68ch;
    text-wrap: pretty;
  }
  .bad {
    color: var(--danger);
  }
  .answers {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .meter {
    position: relative;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    padding: 0.2rem 0.5rem;
    overflow: hidden;
  }
  .meter::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--done);
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  @media (prefers-reduced-motion: no-preference) {
    .meter::before {
      transition: width 200ms linear;
    }
  }
  .meter span {
    position: relative;
  }

  @media (max-width: 640px) {
    .offer {
      margin-inline: var(--gutter);
    }
  }
</style>
