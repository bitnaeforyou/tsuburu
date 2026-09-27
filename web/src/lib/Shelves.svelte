<script lang="ts">
  import * as api from './api'
  import { t } from './i18n.svelte'
  import type { Filed, Picked } from './folders'
  import { leafOf, parentOf, unfiled, within } from './folders'

  /// The row that narrows a list of works to one shelf, and makes the
  /// shelves themselves.
  ///
  /// A shelf exists because the reader made one, not because something is on
  /// it - so one can be set up before there is anything to put there, and
  /// emptying it does not take it away.
  ///
  /// A shelf can be inside another one, which is `/` in its name and nothing
  /// else. Only one level is on screen at a time: the shelves at the top,
  /// then the ones inside whichever of those is being looked at.
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
  /// Renaming and deleting are off until asked for: a row of shelves is a row
  /// of things to press, and one of them should not throw a shelf away.
  let tidying = $state(false)

  const outermost = $derived(shelves.filter((shelf) => !shelf.name.includes('/')))
  /// Which of the outermost shelves the list is inside, if any.
  const opened = $derived(
    typeof picked === 'string' && picked !== 'all'
      ? outermost.find((shelf) => within(picked as string, shelf.name))?.name ?? null
      : null,
  )
  const inside = $derived(
    opened === null ? [] : shelves.filter((shelf) => parentOf(shelf.name) === opened),
  )

  async function make(parent: string | null) {
    const asked = parent
      ? prompt(t('folders.insideName', { name: parent }))
      : prompt(t('folders.name'))
    const name = asked?.trim()
    if (!name) return
    await run(() => api.addFolder(parent ? `${parent}/${name}` : name))
  }

  async function rename(name: string) {
    // Only the part that is its own name: the shelf it is inside is moved by
    // renaming that one, not by retyping it here.
    const to = prompt(t('folders.rename', { name: leafOf(name) }), leafOf(name))?.trim()
    if (!to || to === leafOf(name)) return
    const parent = parentOf(name)
    await run(() => api.renameFolder(name, parent ? `${parent}/${to}` : to))
    if (typeof picked === 'string' && within(picked, name)) picked = parent ?? 'all'
  }

  async function drop(name: string) {
    if (!confirm(t('folders.dropSure', { name }))) return
    await run(() => api.removeFolder(name))
    if (typeof picked === 'string' && within(picked, name)) picked = parentOf(name) ?? 'all'
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

{#snippet tools(name: string)}
  <button
    class="edit"
    disabled={busy}
    onclick={() => make(name)}
    aria-label={t('folders.inside', { name })}
    title={t('folders.inside', { name })}
  >+</button>
  <button
    class="edit"
    disabled={busy}
    onclick={() => rename(name)}
    aria-label={t('folders.rename', { name })}
    title={t('folders.rename', { name })}
  >✎</button>
  <button
    class="edit"
    disabled={busy}
    onclick={() => drop(name)}
    aria-label={t('folders.drop', { name })}
    title={t('folders.drop', { name })}
  >&times;</button>
{/snippet}

<!-- One block, because the screens around this put it in a flex row beside
     the view and sort controls: the shelves inside one must go under it, not
     beside it. -->
<div class="shelving">
<div class="row">
  <ul class="shelves">
    <li>
      <button
        class:on={picked === 'all'}
        aria-pressed={picked === 'all'}
        onclick={() => (picked = 'all')}
      >
        {t('folders.all')} <span class="count">{works.length}</span>
      </button>
    </li>
    {#each outermost as shelf (shelf.name)}
      <li class:on={opened === shelf.name}>
        <button
          class:on={opened === shelf.name}
          aria-pressed={opened === shelf.name}
          onclick={() => (picked = shelf.name)}
        >
          {shelf.name} <span class="count">{shelf.works}</span>
        </button>
        {#if tidying}{@render tools(shelf.name)}{/if}
      </li>
    {/each}
    {#if loose > 0 && shelves.length > 0}
      <li>
        <button
          class:on={picked === null}
          aria-pressed={picked === null}
          onclick={() => (picked = null)}
        >
          {t('folders.loose')} <span class="count">{loose}</span>
        </button>
      </li>
    {/if}
    <li>
      <button class="make" disabled={busy} onclick={() => make(null)}>+ {t('folders.make')}</button>
    </li>
  </ul>
  {#if shelves.length > 0}
    <button class="tidy" aria-pressed={tidying} onclick={() => (tidying = !tidying)}>
      {tidying ? t('folders.tidyDone') : t('folders.tidy')}
    </button>
  {/if}
</div>

{#if opened !== null && (inside.length > 0 || tidying)}
  <!-- The shelves inside the one being looked at. The outer one stays chosen
       above, so the way back out is where it was. -->
  <ul class="shelves within">
    <li>
      <button
        class:on={picked === opened}
        aria-pressed={picked === opened}
        onclick={() => (picked = opened)}
      >
        {t('folders.all')}
      </button>
    </li>
    {#each inside as shelf (shelf.name)}
      <li>
        <button
          class:on={picked === shelf.name}
          aria-pressed={picked === shelf.name}
          onclick={() => (picked = shelf.name)}
        >
          {leafOf(shelf.name)} <span class="count">{shelf.works}</span>
        </button>
        {#if tidying}{@render tools(shelf.name)}{/if}
      </li>
    {/each}
  </ul>
{/if}
</div>

<style>
  .shelving {
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
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
  /* Inset, and in a quieter row, so it reads as being inside the shelf
     chosen above rather than as another shelf beside it. */
  .within {
    margin-block-start: 0.4rem;
    margin-inline-start: 1rem;
    padding-inline-start: 0.6rem;
    border-inline-start: 2px solid var(--line);
  }
  .within button {
    font-size: var(--text-sm);
  }
  .count {
    margin-inline-start: 0.2rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.75;
  }
  /* Attached to the shelf beside them rather than standing on their own:
     with the same outline as a shelf, three tools per shelf read as more
     shelves than there are. */
  .edit {
    padding: 0.25rem 0.4rem;
    line-height: 1;
    border-color: transparent;
    background: transparent;
  }
  .edit:hover:not(:disabled) {
    border-color: var(--line);
  }
  .make {
    border-style: dashed;
  }
  .tidy {
    flex: none;
    font-size: var(--text-sm);
    padding: 0.25rem 0.7rem;
    border-radius: 999px;
    color: var(--muted);
  }
  .tidy[aria-pressed='true'] {
    color: var(--text);
    border-color: var(--text);
  }
</style>
