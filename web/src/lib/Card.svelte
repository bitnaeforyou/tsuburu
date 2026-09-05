<script lang="ts">
  import * as api from './api'

  let { id }: { id: number } = $props()

  let card = $state<api.Card | null>(null)
  let failed = $state(false)
  let element = $state<HTMLElement | null>(null)

  // 갤러리 메타데이터는 한 건에 수십~수백 KB다. 화면에 들어온 카드만 받는다.
  $effect(() => {
    if (!element) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        observer.disconnect()
        void fetchCard()
      },
      { rootMargin: '400px' },
    )
    observer.observe(element)
    return () => observer.disconnect()
  })

  async function fetchCard() {
    try {
      const [found] = await api.cards([id])
      if (found) card = found
      else failed = true
    } catch {
      failed = true
    }
  }
</script>

<article bind:this={element}>
  <div class="thumb">
    {#if card?.thumbnail}
      <img src={card.thumbnail} alt="" loading="lazy" decoding="async" />
    {:else if failed}
      <span class="placeholder">unavailable</span>
    {/if}
  </div>
  <h3>{card?.title ?? `#${id}`}</h3>
  {#if card}
    <p class="meta">
      {card.pages} pages{card.language ? ` · ${card.language}` : ''}
    </p>
  {/if}
</article>

<style>
  article { color: var(--text); }

  .thumb {
    aspect-ratio: 3 / 4;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    display: grid;
    place-items: center;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .placeholder {
    color: var(--muted);
    font-size: 0.8rem;
  }

  h3 {
    margin: 0.5rem 0 0.15rem;
    font-size: 0.9rem;
    font-weight: 500;
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .meta {
    margin: 0;
    color: var(--muted);
    font-size: 0.8rem;
  }
</style>
