<script lang="ts">
  import type { Term } from './api'
  import { t } from './i18n.svelte'

  let {
    terms,
    onpick,
  }: {
    terms: Term[]
    /// Searching again with `to` where `from` was typed.
    onpick: (from: string, to: string) => void
  } = $props()

  const HANGUL = /[가-힣]/

  /// A Korean word the dictionary does not have is searched as written,
  /// which only finds works with it in a Korean title. That is the one
  /// untranslated word worth saying something about: a reader searching
  /// `가축` got thirty works and no sign that the tag had two thousand.
  const missed = (term: Term) => !term.translated && HANGUL.test(term.used)

  const worthShowing = $derived(terms.some((term) => term.translated || missed(term)))
</script>

{#if worthShowing}
  <div class="terms">
    {#each terms as term (term.input)}
      <span class="chip" class:untranslated={!term.translated}>
        {#if term.excluded}<span class="minus">−</span>{/if}
        {term.input}
        {#if term.translated}
          <span class="arrow">→</span>{term.used}
          {#if term.alternatives.length}
            <span class="alt" title={term.alternatives.join(', ')}>
              +{term.alternatives.length}
            </span>
          {/if}
        {:else if missed(term)}
          <span class="note">{t('terms.titlesOnly')}</span>
        {:else}
          <span class="note">{t('terms.noTranslation')}</span>
        {/if}
      </span>
      {#if missed(term) && term.nearby?.length}
        <span class="nearby">
          {t('terms.meant')}
          {#each term.nearby as near (near.used)}
            <button
              class="near"
              title={near.used}
              onclick={() => onpick(term.input, `${term.excluded ? '-' : ''}${near.korean}`)}
            >
              {near.korean}
            </button>
          {/each}
        </span>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .terms {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
    margin: 0 0 1rem;
  }
  .chip {
    font-size: var(--text-sm);
    background: var(--surface);
    border: 1px solid var(--edge);
    border-radius: 999px;
    padding: 0.2rem 0.6rem;
    color: var(--text);
  }
  .untranslated { color: var(--muted); }
  .arrow { color: var(--muted); margin: 0 0.3rem; }
  .minus { color: var(--danger); margin-inline-end: 0.15rem; }
  /* No italic: neither Hangul nor kana has one, so the browser slants the
     upright face instead and the word comes out bent rather than emphasised.
     Muted already says the same thing. */
  .note { color: var(--muted); margin-inline-start: 0.35rem; }
  .alt {
    margin-inline-start: 0.3rem;
    color: var(--accent);
    cursor: help;
  }
  .nearby {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.3rem;
    font-size: var(--text-sm);
    color: var(--muted);
  }
  .near {
    font-size: var(--text-sm);
    padding: 0.15rem 0.55rem;
    border-radius: 999px;
  }
</style>
