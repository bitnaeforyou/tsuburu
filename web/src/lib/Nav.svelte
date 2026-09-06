<script lang="ts">
  import { toSearch } from './router'

  let { active }: { active: 'search' | 'favorites' | 'history' } = $props()
</script>

<nav>
  <a href={toSearch()} class:active={active === 'search'}>Search</a>
  <a href="#/favorites" class:active={active === 'favorites'}>Favorites</a>
  <a href="#/history" class:active={active === 'history'}>History</a>
</nav>

<style>
  nav {
    display: flex;
    gap: 0.25rem;
  }
  a {
    text-decoration: none;
    padding: 0.35rem 0.7rem;
    border-radius: var(--radius);
    color: var(--muted);
  }
  a:hover { color: var(--text); }
  .active {
    color: var(--text);
    background: var(--surface);
  }
</style>
