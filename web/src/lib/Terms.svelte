<script lang="ts">
  import type { Term } from './api'
  import { t } from './i18n.svelte'

  let { terms }: { terms: Term[] } = $props()

  // 번역된 것이 하나도 없으면 굳이 보여줄 필요가 없다.
  const worthShowing = $derived(terms.some((t) => t.translated))
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
        {:else}
          <span class="note">{t('terms.noTranslation')}</span>
        {/if}
      </span>
    {/each}
  </div>
{/if}

<style>
  .terms {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0 0 1rem;
  }
  .chip {
    font-size: 0.8rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.2rem 0.6rem;
    color: var(--text);
  }
  .untranslated { color: var(--muted); }
  .arrow { color: var(--muted); margin: 0 0.3rem; }
  .minus { color: var(--danger); margin-right: 0.15rem; }
  .note { color: var(--muted); margin-left: 0.35rem; font-style: italic; }
  .alt {
    margin-left: 0.3rem;
    color: var(--accent);
    cursor: help;
  }
</style>
