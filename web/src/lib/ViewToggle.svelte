<script lang="ts">
  import { t } from './i18n.svelte'
  import { view } from './view.svelte'
</script>

<div class="views" role="group" aria-label={t('view.label')}>
  <button
    class:on={!view.rows}
    onclick={() => view.set('covers')}
    aria-pressed={!view.rows}
    title={t('view.covers')}
  >
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      <rect x="1" y="1" width="6" height="6" rx="1" fill="currentColor" />
      <rect x="9" y="1" width="6" height="6" rx="1" fill="currentColor" />
      <rect x="1" y="9" width="6" height="6" rx="1" fill="currentColor" />
      <rect x="9" y="9" width="6" height="6" rx="1" fill="currentColor" />
    </svg>
    <span class="sr-only">{t('view.covers')}</span>
  </button>
  <button
    class:on={view.rows}
    onclick={() => view.set('rows')}
    aria-pressed={view.rows}
    title={t('view.rows')}
  >
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      <rect x="1" y="2" width="4" height="4" rx="1" fill="currentColor" />
      <rect x="7" y="2.5" width="8" height="1.4" rx="0.7" fill="currentColor" />
      <rect x="7" y="4.8" width="5" height="1.2" rx="0.6" fill="currentColor" opacity="0.6" />
      <rect x="1" y="10" width="4" height="4" rx="1" fill="currentColor" />
      <rect x="7" y="10.5" width="8" height="1.4" rx="0.7" fill="currentColor" />
      <rect x="7" y="12.8" width="5" height="1.2" rx="0.6" fill="currentColor" opacity="0.6" />
    </svg>
    <span class="sr-only">{t('view.rows')}</span>
  </button>
</div>

<style>
  .views {
    display: flex;
    gap: 0.15rem;
  }
  .views button {
    display: grid;
    place-items: center;
    padding: 0.3rem 0.45rem;
    color: var(--muted);
    border-color: transparent;
    background: transparent;
  }
  .views button.on {
    color: var(--text);
    background: var(--surface);
    border-color: var(--edge);
  }
</style>
