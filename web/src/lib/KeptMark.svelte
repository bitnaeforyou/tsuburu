<script lang="ts">
  import { downloaded } from './library.svelte'
  import { t } from './i18n.svelte'

  /// Rides on a cover to say the work is already on this device - with a
  /// tick once all of it is, and how far it got while it is not.
  let { id }: { id: number } = $props()

  const kept = $derived(downloaded.of(id))
  const said = $derived(
    kept?.complete
      ? t('card.downloaded')
      : t('card.downloadedPart', { have: kept?.have ?? 0, pages: kept?.pages ?? 0 }),
  )

  // Asked once for the whole session, whichever cover gets there first.
  $effect(() => {
    void downloaded.load()
  })
</script>

{#if kept && kept.have > 0}
  <span class="kept" class:whole={kept.complete} title={said}>
    <span aria-hidden="true">{kept.complete ? '✓' : `${kept.have}/${kept.pages}`}</span>
    <span class="sr-only">{said}</span>
  </span>
{/if}

<style>
  .kept {
    position: absolute;
    top: 0.3rem;
    inset-inline-start: 0.3rem;
    min-inline-size: 1.5rem;
    block-size: 1.5rem;
    padding-inline: 0.3rem;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 1;
    color: var(--text);
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    pointer-events: none;
  }

  /* All of it: the one state worth seeing from across the page, so it is
     the one that is filled. */
  .kept.whole {
    color: var(--accent-on);
    background: var(--accent-solid);
    font-weight: 600;
  }
</style>
