<script lang="ts">
  import type { Snippet } from 'svelte'
  import { toDialogue, toSearch } from './router'
  import { LOCALES, i18n, t, type Locale } from './i18n.svelte'

  // 화면 이동은 전부 여기 모은다. 예전에는 검색 실행 버튼과 Search 탭이 나란히
  // 붙어 있어서, 생김새가 같은 두 개가 서로 다른 일을 했다. 위 줄은 이동만,
  // 아래 줄은 도구만 두어 둘을 갈라놓는다.
  let {
    active,
    actions,
  }: {
    active: 'search' | 'dialogue' | 'favorites' | 'downloads' | 'history'
    actions?: Snippet
  } = $props()

  const TABS = [
    { id: 'search', key: 'nav.search', href: toSearch() },
    { id: 'dialogue', key: 'nav.dialogue', href: toDialogue() },
    { id: 'favorites', key: 'nav.favorites', href: '#/favorites' },
    { id: 'downloads', key: 'nav.downloads', href: '#/downloads' },
    { id: 'history', key: 'nav.history', href: '#/history' },
  ] as const
</script>

<header>
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
    <select
      class="locale"
      aria-label={t('nav.locale')}
      value={i18n.locale}
      onchange={(e) => i18n.set(e.currentTarget.value as Locale)}
    >
      {#each Object.entries(LOCALES) as [code, name] (code)}
        <option value={code}>{name}</option>
      {/each}
    </select>
  </nav>
</header>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.7rem 1rem;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    font-weight: 600;
    text-decoration: none;
    letter-spacing: 0.02em;
    margin-right: auto;
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

  /* The language belongs with the navigation, but it is not a place to go. */
  .locale {
    margin-left: 0.5rem;
    font: inherit;
    font-size: 0.85rem;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.2rem 0.3rem;
  }

  nav a {
    text-decoration: none;
    color: var(--muted);
    padding: 0.3rem 0.7rem;
    border-radius: var(--radius);
    font-size: 0.9rem;
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
</style>
