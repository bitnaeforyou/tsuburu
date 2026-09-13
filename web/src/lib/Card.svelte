<script lang="ts">
  import * as api from './api'
  import * as cards from './cards.svelte'
  import { library } from './library.svelte'
  import { toGallery } from './router'
  import { t } from './i18n.svelte'

  let {
    id,
    preset = null,
    progress = null,
    level = 2,
  }: {
    id: number
    /** 라이브러리 목록은 요약을 이미 갖고 있어 네트워크를 탈 이유가 없다. */
    preset?: api.Summary | null
    progress?: { page: number; pages: number } | null
    /// Which heading level the title is, counting from the screen's own.
    level?: 2 | 3
  } = $props()

  let card = $state<api.Card | null>(null)
  let failed = $state(false)
  let element = $state<HTMLElement | null>(null)

  const thumbnail = $derived(
    card?.thumbnail ??
      (preset?.thumbnail_hash ? `/tn/${preset.thumbnail_hash}.avif` : null),
  )
  const title = $derived(card?.title ?? preset?.title ?? `#${id}`)
  const pages = $derived(card?.pages ?? preset?.pages ?? 0)
  const language = $derived(card?.language ?? preset?.language ?? null)
  const favorited = $derived(library.has(id))
  /// What the work is, in the three words a card has room for. Namespaced
  /// tags say female:/male:; the part after the colon is the word.
  const topTags = $derived(
    (card?.tags ?? [])
      .map((tag) => tag.split(':').pop() ?? tag)
      .filter((tag) => tag.length > 0)
      .slice(0, 3),
  )

  // 갤러리 메타데이터는 한 건에 수십~수백 KB다. 화면에 들어온 카드만 받는다.
  $effect(() => {
    if ((preset && preset.thumbnail_hash) || card || !element) return
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
      const found = await cards.card(id)
      if (found) card = found
      else failed = true
    } catch {
      failed = true
    }
  }

  function summary(): Omit<api.Summary, 'id'> {
    return {
      title,
      language,
      kind: card?.kind ?? preset?.kind ?? null,
      pages,
      thumbnail_hash: thumbnail?.replace(/^\/tn\/|\.avif$/g, '') ?? null,
    }
  }

  async function toggleFavorite(event: MouseEvent) {
    // 카드 전체가 링크이므로 별을 눌렀을 때 갤러리로 넘어가면 안 된다.
    event.preventDefault()
    event.stopPropagation()
    await library.toggle(id, summary())
  }
</script>

<article bind:this={element}>
  <a href={toGallery(id)} class="link">
    <div class="thumb">
      {#if thumbnail}
        <img src={thumbnail} alt="" loading="lazy" decoding="async" />
      {:else if failed}
        <span class="placeholder">{t('card.unavailable')}</span>
      {/if}
      {#if progress && progress.pages > 0}
        <div class="progress" style:--read={`${((progress.page + 1) / progress.pages) * 100}%`}>
          <span>{progress.page + 1} / {progress.pages}</span>
        </div>
      {/if}
    </div>
    <svelte:element this={`h${level}`} class="title" title={title}>{title}</svelte:element>
    <p class="meta">
      <span class="id">#{id}</span>
      &middot; {t('common.pages', { n: pages })}{language ? ` · ${language}` : ''}
    </p>
    {#if card?.listed === false}
      <p class="unlisted" title={t('card.unlistedNote')}>{t('card.unlisted')}</p>
    {/if}
    {#if topTags.length}
      <p class="tags" title={topTags.join(' · ')}>{topTags.join(' · ')}</p>
    {/if}
  </a>

  {#if !library.unavailable}
    <button
      class="star"
      class:on={favorited}
      onclick={toggleFavorite}
      aria-pressed={favorited}
      aria-label={favorited ? t('card.favoriteRemove') : t('card.favoriteAdd')}
      title={favorited ? t('card.favoriteRemove') : t('card.favoriteAdd')}
    >
      {favorited ? '★' : '☆'}
    </button>
  {/if}
</article>

<style>
  article {
    position: relative;
    color: var(--text);
  }

  .link {
    text-decoration: none;
    color: inherit;
    display: block;
  }

  .thumb {
    position: relative;
    aspect-ratio: 3 / 4;
    background: var(--surface);
    border: 1px solid var(--image-edge);
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
    font-size: var(--text-sm);
  }

  .progress {
    position: absolute;
    inset: auto 0 0 0;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    padding: 0.15rem 0.35rem;
  }
  .progress::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--read);
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .progress span { position: relative; }

  .star {
    position: absolute;
    top: 0.3rem;
    inset-inline-end: 0.3rem;
    padding: 0.1rem 0.35rem;
    line-height: 1.2;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    border-color: transparent;
    color: var(--muted);
  }
  .star.on { color: var(--accent); }

  /* A star meant for a mouse is too small for a thumb. */
  @media (max-width: 640px) {
    .star {
      top: 0.25rem;
      inset-inline-end: 0.25rem;
      padding: 0.35rem 0.55rem;
    }
  }

  .title {
    margin: 0.5rem 0 0.15rem;
    font-size: var(--text-md);
    font-weight: 500;
    line-height: 1.3;
    /* Exactly the two lines it is allowed, so the line under it lands on the
       same edge across the row. */
    min-height: 2.6em;
    /* Titles here are often one unbroken token of underscores, which has no
       place to wrap and so spills out of the clamp instead of ending in an
       ellipsis. */
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .meta {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }
  .id {
    font-variant-numeric: tabular-nums;
    user-select: all;
  }

  /* Not an error: the work is here and readable. It is a reason to keep a
     copy, because the site has stopped pointing at it. */
  .unlisted {
    margin: 0.15rem 0 0;
    color: var(--accent);
    font-size: var(--text-xs);
  }

  .tags {
    margin: 0.1rem 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
