<script lang="ts">
  import { LOCALES, i18n, t, type Locale } from './i18n.svelte'
</script>

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

<style>
  /* Not a place to go, so it does not look like the tabs beside it. */
  .locale {
    font: inherit;
    font-size: 0.85rem;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.2rem 0.3rem;
  }
</style>
