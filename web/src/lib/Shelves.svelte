<script lang="ts">
  import { t } from './i18n.svelte'
  import type { Filed, Picked } from './folders'
  import { shelvesOf, unfiled } from './folders'

  /// The row that narrows a list of works to one shelf.
  let {
    works,
    picked = $bindable('all'),
  }: {
    works: Filed[]
    picked?: Picked
  } = $props()

  const shelves = $derived(shelvesOf(works))
  const loose = $derived(unfiled(works))
</script>

{#if shelves.length}
  <ul class="shelves">
    <li>
      <button class:on={picked === 'all'} onclick={() => (picked = 'all')}>
        {t('folders.all')} <span class="count">{works.length}</span>
      </button>
    </li>
    {#each shelves as [name, filed] (name)}
      <li>
        <button class:on={picked === name} onclick={() => (picked = name)}>
          {name} <span class="count">{filed}</span>
        </button>
      </li>
    {/each}
    {#if loose > 0}
      <li>
        <button class:on={picked === null} onclick={() => (picked = null)}>
          {t('folders.loose')} <span class="count">{loose}</span>
        </button>
      </li>
    {/if}
  </ul>
{/if}

<style>
  .shelves {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0;
    margin: 0;
  }
  .shelves button {
    font-size: var(--text-md);
    padding: 0.25rem 0.7rem;
    border-radius: 999px;
    color: var(--muted);
  }
  .shelves button.on {
    color: var(--accent-on);
    background: var(--accent-solid);
    border-color: var(--accent-solid);
  }
  .count {
    margin-inline-start: 0.2rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.75;
  }
</style>
