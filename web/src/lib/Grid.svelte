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

  .grid > :global(*) {
    /* 화면 밖 카드는 렌더링을 건너뛴다. 가상 스크롤 라이브러리 대신 쓴다. */
    content-visibility: auto;
    contain-intrinsic-size: auto 270px;
  }
</style>
