<script lang="ts">
  import type { Snippet } from 'svelte'
  let { children }: { children: Snippet } = $props()
</script>

<div class="grid">{@render children()}</div>

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
  }

  .grid > :global(*) {
    /* 화면 밖 카드는 렌더링을 건너뛴다. 가상 스크롤 라이브러리 대신 쓴다. */
    content-visibility: auto;
    contain-intrinsic-size: auto 270px;
  }
</style>
