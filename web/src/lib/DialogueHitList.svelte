<script lang="ts">
  import type { DialogueHit } from './api'
  import { t } from './i18n.svelte'
  import { toGallery } from './router'
  import Card from './Card.svelte'

  let { hits }: { hits: DialogueHit[] } = $props()
</script>

<ul class="hits">
  {#each hits as hit (hit.gallery_id)}
    <li>
      <div class="thumb"><Card id={hit.gallery_id} /></div>
      <a class="line" href={toGallery(hit.gallery_id, hit.page)}>
        <span class="where">
          {t('common.page', { n: hit.page + 1 })}
          {#if hit.exact}
            <span class="badge">{t('dialogue.exact')}</span>
          {:else}
            <span class="badge fuzzy">~{Math.round(hit.score * 100)}%</span>
          {/if}
          {#if hit.also?.length}
            <span class="badge fuzzy" title={hit.also.join(', ')}>
              {hit.also.length > 1
                ? t('dialogue.copies', { n: hit.also.length })
                : t('dialogue.copy', { n: hit.also.length })}
            </span>
          {/if}
        </span>
        <blockquote>{hit.snippet.join(' / ')}</blockquote>
      </a>
    </li>
  {/each}
</ul>

<style>
  .hits {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 0.6rem;
  }
  .hits li {
    display: grid;
    grid-template-columns: 72px 1fr;
    gap: 0.8rem;
    align-items: start;
  }
  /* The card is here to be recognised, not read: one line of title is
     enough, and shorter rows keep the sections below in view. */
  .thumb :global(h3) {
    font-size: 0.8rem;
    display: -webkit-box;
    -webkit-line-clamp: 1;
    line-clamp: 1;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .thumb :global(p) {
    font-size: 0.7rem;
  }
  .line {
    text-decoration: none;
    color: inherit;
  }
  .where {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .badge {
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0 0.4rem;
    margin-left: 0.3rem;
  }
  .badge.fuzzy {
    color: var(--accent);
  }
  blockquote {
    margin: 0.25rem 0 0;
    font-size: 0.9rem;
  }
</style>
