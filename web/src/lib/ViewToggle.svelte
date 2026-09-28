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

  /* The same quiet icon button as the reload that sits beside it: no edge
     while resting, a filled shape under the pointer. Two conventions in one
     row of tools read as two interfaces, and the square is what makes it
     big enough to hit - it used to be 0.3rem of padding around a 14px icon,
     which is under the floor for a target. */
  .views button {
    display: grid;
    place-items: center;
    inline-size: 2.5rem;
    block-size: 2.5rem;
    padding: 0;
    color: var(--muted);
    background: none;
    /* Transparent rather than absent: the one that is on takes this edge,
       and a border appearing would move the icon by a pixel. */
    border: 1px solid transparent;
    border-radius: var(--radius);
  }

  /* Said here rather than left to the global rule, which turns the edge the
     accent colour: on a control with no edge at rest that is a line
     appearing out of nothing. */
  .views button:hover:not(:disabled) {
    color: var(--text);
    background: var(--raised);
    border-color: transparent;
  }

  /* Which one is on is not a hover - it keeps its shape when the pointer
     leaves, and says so with an edge as well as a fill. */
  .views button.on,
  .views button.on:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface);
    border-color: var(--edge);
  }
</style>
