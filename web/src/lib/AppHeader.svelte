<script lang="ts">
  import type { Snippet } from 'svelte'
  import { toSearch, toSettings } from './router'
  import { t } from './i18n.svelte'
  import LocalePicker from './LocalePicker.svelte'

  // 화면 이동은 전부 여기 모은다. 예전에는 검색 실행 버튼과 Search 탭이 나란히
  // 붙어 있어서, 생김새가 같은 두 개가 서로 다른 일을 했다. 위 줄은 이동만,
  // 아래 줄은 도구만 두어 둘을 갈라놓는다.
  let {
    active,
    actions,
  }: {
    active: 'search' | 'settings' | 'favorites' | 'downloads' | 'history'
    actions?: Snippet
  } = $props()

  // The bar is only at the bottom on a phone, but the padding that keeps the
  // last row of a page out from under it is on the body, which no component
  // owns. Saying so here means it is set exactly while the bar exists.
  $effect(() => {
    document.body.classList.add('has-tabs')
    return () => document.body.classList.remove('has-tabs')
  })

  // In the order they are reached for: looking for something, then the things
  // you kept, then what you read, then what is on disk. Settings last, where
  // settings go.
  function toContent() {
    const main = document.querySelector('main')
    if (!main) return
    main.setAttribute('tabindex', '-1')
    main.focus()
    main.scrollIntoView()
  }

  /// Drawn here rather than pulled from an icon set: five shapes is less
  /// than a dependency, and they have to match the stroke of the ones
  /// already in this program. One path each, 24x24, no fills - at the size a
  /// tab bar uses, a filled glyph closes up into a blob.
  const MARKS = {
    search: 'M10.5 3a7.5 7.5 0 1 0 4.55 13.46l4.24 4.25 1.42-1.42-4.25-4.24A7.5 7.5 0 0 0 10.5 3Zm0 2a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11Z',
    favorites: 'M12 20.5 4.8 13.3a4.6 4.6 0 0 1 6.5-6.5l.7.7.7-.7a4.6 4.6 0 0 1 6.5 6.5Z',
    history: 'M12 7v5l3.5 2M12 3a9 9 0 1 0 9 9M12 3a9 9 0 0 1 9 9M3.5 7.5 3 3m.5 4.5H8',
    downloads: 'M12 3v11m0 0 4-4m-4 4-4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2',
    settings: 'M12 9.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5ZM19.4 13a7.6 7.6 0 0 0 0-2l2-1.5-2-3.5-2.3 1a7.6 7.6 0 0 0-1.7-1l-.4-2.5h-4l-.4 2.5c-.6.2-1.2.6-1.7 1l-2.3-1-2 3.5L4.6 11a7.6 7.6 0 0 0 0 2l-2 1.5 2 3.5 2.3-1c.5.4 1.1.8 1.7 1l.4 2.5h4l.4-2.5c.6-.2 1.2-.6 1.7-1l2.3 1 2-3.5Z',
  } as const

  const TABS = [
    { id: 'search', key: 'nav.search', href: toSearch() },
    { id: 'favorites', key: 'nav.favorites', href: '#/favorites' },
    { id: 'history', key: 'nav.history', href: '#/history' },
    { id: 'downloads', key: 'nav.downloads', href: '#/downloads' },
    { id: 'settings', key: 'nav.settings', href: toSettings() },
  ] as const
</script>

<header>
  <!-- Not a link: the address bar holds the route, so jumping to the content
       has to move focus rather than change where we are. -->
  <button class="skip" onclick={toContent}>{t('nav.skip')}</button>
  <a class="brand" href={toSearch()}>tsuburu</a>

  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}

  <nav>
    {#each TABS as tab (tab.id)}
      <a href={tab.href} class:current={active === tab.id} aria-current={active === tab.id ? 'page' : undefined}>
        <!-- Only drawn where the bar is at the bottom of a phone. On a
             desktop this row is a set of links in a line of chrome, and a
             picture beside each one is noise. -->
        <svg class="mark" viewBox="0 0 24 24" aria-hidden="true">
          <path d={MARKS[tab.id]} />
        </svg>
        <span>{t(tab.key)}</span>
      </a>
    {/each}
  </nav>

  <LocalePicker />
</header>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 1rem;
    /* The phone draws its clock and battery over the top of the page, so the
       first row has to start below them. Zero everywhere that has no such
       strip. */
    padding: calc(0.7rem + var(--safe-top)) var(--gutter) 0.7rem;
    background: var(--bg);
    border-block-end: 1px solid var(--line);
  }

  .brand {
    font-weight: 600;
    text-decoration: none;
    letter-spacing: 0.02em;
    margin-inline-end: auto;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  nav {
    display: flex;
    align-items: center;
    gap: 0.15rem;
  }

  header :global(.locale) {
    margin-inline-start: 0.5rem;
  }

  nav a {
    text-decoration: none;
    color: var(--muted);
    padding: 0.3rem 0.7rem;
    border-radius: var(--radius);
    font-size: var(--text-md);
  }

  nav a:hover {
    color: var(--text);
  }

  /* A line of links in a strip of chrome. A picture beside each word here is
     noise; the bar at the bottom of a phone is where they earn their room. */
  .mark {
    display: none;
  }

  /* 탭은 버튼처럼 보이지 않아야 한다. 선택된 것만 밑줄로 표시한다. */
  nav a.current {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
    border-radius: 0;
  }

  /* On a phone the five destinations move to the bottom, where a thumb
     reaches them, and stop eating three rows of the top of every screen. */
  @media (max-width: 640px) {
    header {
      padding: calc(0.6rem + var(--safe-top)) var(--gutter) 0.6rem;
      gap: 0.5rem;
    }

    nav {
      position: fixed;
      inset-inline: 0;
      bottom: 0;
      z-index: 3;
      justify-content: space-around;
      gap: 0;
      padding: 0.25rem 0 calc(0.25rem + var(--safe-bottom));
      background: var(--bg);
      border-block-start: 1px solid var(--line);
    }

    /* A picture and a word, stacked, in a target a thumb can find without
       looking. The row used to be five words at twelve pixels with the
       system's own handle running through them. */
    nav a {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 0.15rem;
      min-block-size: 3rem;
      padding: 0.35rem 0.2rem;
      font-size: var(--text-xs);
      border-radius: 0;
    }

    .mark {
      display: block;
      inline-size: 1.45rem;
      block-size: 1.45rem;
      fill: none;
      stroke: currentColor;
      /* Matched to the weight of the label under it. */
      stroke-width: 1.7;
      stroke-linecap: round;
      stroke-linejoin: round;
    }

    /* Where you are is said by the whole tab going solid, not by a line
       above it: a line at the top of a bar pinned to the bottom of the
       window reads as the edge of the content, not as a mark on a tab. */
    nav a.current {
      color: var(--accent);
      box-shadow: none;
    }

    nav a.current .mark {
      stroke-width: 2.1;
    }
  }
</style>
