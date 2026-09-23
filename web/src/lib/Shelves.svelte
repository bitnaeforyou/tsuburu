<script lang="ts">
  import * as api from './api'
  import { t } from './i18n.svelte'
  import type { Filed, Picked } from './folders'
  import { unfiled } from './folders'

  /// The row that narrows a list of works to one shelf, and makes the
  /// shelves themselves.
  ///
  /// A shelf exists because the reader made one, not because something is on
  /// it - so one can be set up before there is anything to put there, and
  /// emptying it does not take it away.
  let {
    works,
    shelves,
    picked = $bindable('all'),
    onchange,
  }: {
    works: Filed[]
    /// Every shelf there is, and how much is on each.
    shelves: api.Folder[]
    picked?: Picked
    /// Said when the shelves themselves change.
    onchange: (shelves: api.Folder[]) => void
  } = $props()

  const loose = $derived(unfiled(works))
  let busy = $state(false)

  async function make() {
    const name = prompt(t('folders.name'))?.trim()
    if (!name) return
    await run(() => api.addFolder(name))
  }

  async function rename(name: string) {
    const to = prompt(t('folders.rename', { name }), name)?.trim()
    if (!to || to === name) return
    await run(() => api.renameFolder(name, to))
    if (picked === name) picked = to
  }

  async function drop(name: string) {
    if (!confirm(t('folders.dropSure', { name }))) return
    await run(() => api.removeFolder(name))
    if (picked === name) picked = 'all'
  }

  async function run(what: () => Promise<api.Folder[]>) {
    busy = true
    try {
      onchange(await what())
    } catch {
      // Reported by whichever screen owns the list.
    } finally {
      busy = false
    }
  }
</script>

<ul class="shelves">
  <li>
    <button class:on={picked === 'all'} onclick={() => (picked = 'all')}>
      {t('folders.all')} <span class="count">{works.length}</span>
    </button>
  </li>
  {#each shelves as shelf (shelf.name)}
    <li class:on={picked === shelf.name}>
      <button class:on={picked === shelf.name} onclick={() => (picked = shelf.name)}>
        {shelf.name} <span class="count">{shelf.works}</span>
      </button>
      {#if picked === shelf.name}
        <!-- Only for the one being looked at: a row of shelves is not a row
             of buttons to press by accident. -->
        <button
          class="edit"
          disabled={busy}
          onclick={() => rename(shelf.name)}
          aria-label={t('folders.rename', { name: shelf.name })}
          title={t('folders.rename', { name: shelf.name })}
        >✎</button>
        <button
          class="edit"
          disabled={busy}
          onclick={() => drop(shelf.name)}
          aria-label={t('folders.drop', { name: shelf.name })}
          title={t('folders.drop', { name: shelf.name })}
        >&times;</button>
      {/if}
    </li>
  {/each}
  {#if loose > 0 && shelves.length > 0}
    <li>
      <button class:on={picked === null} onclick={() => (picked = null)}>
        {t('folders.loose')} <span class="count">{loose}</span>
      </button>
    </li>
  {/if}
  <li>
    <button class="make" disabled={busy} onclick={make}>+ {t('folders.make')}</button>
  </li>
</ul>

<style>
  .shelves {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0;
    margin: 0;
  }
  .shelves li {
    display: flex;
    align-items: center;
    gap: 0.1rem;
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
  .edit {
    padding: 0.25rem 0.5rem;
    line-height: 1;
  }
  .make {
    border-style: dashed;
  }
</style>
