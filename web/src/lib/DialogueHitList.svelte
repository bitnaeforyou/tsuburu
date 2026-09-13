<script lang="ts">
  import * as api from './api'
  import * as cards from './cards.svelte'
  import type { DialogueHit } from './api'
  import { t } from './i18n.svelte'
  import { toGallery } from './router'

  /// A line that was found, and the work it was found in. The line is the
  /// answer, so it is what the row is mostly made of; the cover and the title
  /// are there to say which book it came out of.
  let { hits }: { hits: DialogueHit[] } = $props()

  let known = $state(new Map<number, api.Card | null>())

  const keyOf = (hit: DialogueHit) => `${hit.gallery_id}:${hit.page}`

  /// hitomi carries the same doujin many times over, under a different number
  /// each time. Two hits with the same title and the same number of pages are
  /// the same book, and one row is what the reader wants to see.
  const shown = $derived.by(() => {
    const seen = new Map<string, { hit: DialogueHit; copies: number }>()
    for (const hit of hits) {
      const card = known.get(hit.gallery_id)
      // Until the card arrives there is nothing to compare it by, so it
      // stands on its own rather than being folded into the wrong book.
      const same = card?.title ? `${card.title}|${card.pages}` : `id:${hit.gallery_id}`
      const already = seen.get(same)
      if (already) already.copies += 1
      else seen.set(same, { hit, copies: 1 })
    }
    return [...seen.values()]
  })

  // One request for the page of results rather than one per row.
  $effect(() => {
    for (const hit of hits) {
      if (known.has(hit.gallery_id)) continue
      known.set(hit.gallery_id, null)
      void cards
        .card(hit.gallery_id)
        .then((card) => (known = new Map(known).set(hit.gallery_id, card)))
        .catch(() => {})
    }
  })

</script>

<ul class="hits">
  {#each shown as { hit, copies } (keyOf(hit))}
    {@const card = known.get(hit.gallery_id)}
    <li>
      <a class="row" href={toGallery(hit.gallery_id, hit.page)}>
        <span class="thumb">
          {#if card?.thumbnail}
            <img src={card.thumbnail} alt="" loading="lazy" decoding="async" />
          {/if}
        </span>

        <span class="body">
          <strong class="title">{card?.title ?? `#${hit.gallery_id}`}</strong>

          {#if card}
            <span class="meta">
              {#if card.artists.length}{card.artists.join(', ')} &middot; {/if}
              {t('common.pages', { n: card.pages })}
              {#if card.language}&middot; {card.language}{/if}
              {#if card.kind}&middot; {t(`kind.${card.kind}` as 'kind.manga')}{/if}
              {#if copies > 1}
                <span class="badge" title={t('dialogue.uploads')}>&times;{copies}</span>
              {/if}
            </span>
          {/if}

          <!-- The line is why this work is in the list, not what the list is
               of: it goes underneath, in its own voice. -->
          <span class="found">
            <span class="at">
              {t('common.page', { n: hit.page + 1 })}
              {#if hit.exact}
                <span class="badge">{t('dialogue.exact')}</span>
              {:else}
                <span class="badge fuzzy">{Math.round(hit.score * 100)}%</span>
              {/if}
              {#if hit.also?.length}
                <span class="badge fuzzy" title={hit.also.join(', ')}>
                  {hit.also.length > 1
                    ? t('dialogue.copies', { n: hit.also.length })
                    : t('dialogue.copy', { n: hit.also.length })}
                </span>
              {/if}
            </span>
            <q>{hit.snippet.join(' / ')}</q>
          </span>
        </span>
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
    gap: 0.5rem;
  }

  .hits > li {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
    /* A page of results is a lot of covers; only the visible ones are drawn. */
    content-visibility: auto;
    contain-intrinsic-size: auto 96px;
  }

  .row {
    display: grid;
    grid-template-columns: 64px 1fr;
    gap: 0.8rem;
    padding: 0.6rem 0.75rem;
    text-decoration: none;
    color: inherit;
  }
  .row:hover {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }

  .thumb {
    display: block;
    aspect-ratio: 3 / 4;
    background: var(--bg);
    border: 1px solid var(--image-edge);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .body {
    min-width: 0;
    display: grid;
    gap: 0.25rem;
    align-content: start;
  }

  /* The work is what the row is about, so it is what the row says first. */
  .title {
    font-weight: 600;
    font-size: var(--text-md);
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: var(--text-sm);
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .found {
    display: block;
    margin-top: 0.15rem;
    padding-inline-start: 0.6rem;
    border-inline-start: 2px solid var(--line);
    /* A quotation that runs the width of an ultrawide stops being one. */
    max-width: 72ch;
  }
  .at {
    font-size: var(--text-xs);
    color: var(--muted);
  }
  .found q {
    display: block;
    font-size: var(--text-sm);
    line-height: 1.4;
    color: var(--muted);
    quotes: '“' '”';
    overflow-wrap: anywhere;
    /* A found line is a sentence, not a label: no single word left alone
       on the last line of it. */
    text-wrap: pretty;
  }
  .row:hover .found q {
    color: var(--text);
  }

  .badge {
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0 0.4rem;
    margin-inline-start: 0.3rem;
    font-variant-numeric: tabular-nums;
  }
  .badge.fuzzy {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }


  @media (max-width: 640px) {
    .row {
      grid-template-columns: 52px 1fr;
      gap: 0.6rem;
      padding: 0.55rem 0.6rem;
    }
    .title {
      white-space: normal;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      -webkit-box-orient: vertical;
    }
    .meta {
      white-space: normal;
    }
  }
</style>
