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
        {t(tab.key)}
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
    border-bottom: 1px solid var(--line);
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
      padding: 0.25rem 0 calc(0.25rem + env(safe-area-inset-bottom));
      background: var(--bg);
      border-top: 1px solid var(--line);
    }

    nav a {
      flex: 1;
      text-align: center;
      padding: 0.5rem 0.2rem;
      font-size: var(--text-xs);
    }

    nav a.current {
      box-shadow: inset 0 2px 0 var(--accent);
    }
  }
</style>
