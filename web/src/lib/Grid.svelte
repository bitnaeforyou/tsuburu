<script lang="ts">
  import type { Snippet } from 'svelte'
  import { view } from './view.svelte'
  let { children }: { children: Snippet } = $props()
</script>

<!-- Keyed on the mode so switching builds the list again.
     `contain-intrinsic-size: auto` remembers how tall each card was the last
     time it was drawn and reserves that much while it is off screen. The
     remembered size survives the switch, so a covers grid kept reserving the
     109px a row had taken and the cells collapsed into each other; the same
     the other way. Nothing here is expensive to build twice - the cards come
     from a cache that outlives them. -->
{#key view.mode}
  <div class="grid" class:rows={view.rows}>{@render children()}</div>
{/key}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 1rem;
  }

  /* A phone at 320px fits exactly one 160px column, which turns a wall of
     covers into a column of posters. Two narrower ones is how a shelf of
     these is read. */
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
      gap: 0.75rem;
    }
    .grid.rows {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.5rem;
    }
  }

  /* One work per line, its cover small beside what it is. The cards lay
     themselves out from this; see Card. */
  .grid.rows {
    grid-template-columns: minmax(0, 1fr);
    gap: 0.5rem;
  }

  .grid > :global(*) {
    /* 화면 밖 카드는 렌더링을 건너뛴다. 가상 스크롤 라이브러리 대신 쓴다. */
    content-visibility: auto;
    contain-intrinsic-size: auto 315px;
  }
  .grid.rows > :global(*) {
    contain-intrinsic-size: auto 110px;
  }
</style>
