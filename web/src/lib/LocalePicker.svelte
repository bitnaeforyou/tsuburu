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
  /* Shape comes from the one rule every dropdown here shares; this says
     only what makes it the quiet one in a bar of links. It used to set its
     own padding, which came to twenty pixels tall - under the floor for
     something to press. */
  .locale {
    font-size: var(--text-md);
    color: var(--muted);
  }

  @media (max-width: 640px) {
    .locale {
      padding: 0.4rem 0.35rem;
      font-size: var(--text-base);
    }
  }
</style>
