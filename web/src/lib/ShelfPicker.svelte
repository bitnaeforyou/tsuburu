<script lang="ts">
  import { t } from './i18n.svelte'
  import { chosen, leafOf } from './folders'

  /// The control that moves one work onto a shelf.
  let {
    id,
    folder = null,
    title,
    shelves,
    onmove,
    disabled = false,
  }: {
    id: number
    folder?: string | null
    title: string
    /// The shelves already in use, so a reader mostly picks rather than types.
    shelves: string[]
    onmove: (id: number, folder: string | null) => void
    disabled?: boolean
  } = $props()

  function pick(value: string) {
    const to = chosen(value, () => prompt(t('folders.name')))
    if (to !== undefined) onmove(id, to)
  }

  /// The shelves gathered under the one they sit inside.
  ///
  /// A flat list reads as `A`, `B`, `D/a`, `D/b`, `D/c`, `E` - the shelves
  /// inside `D` outnumber the ones beside it and the shape is lost. A select
  /// cannot be walked into, so the next best thing is what the platform
  /// already draws: a heading per outermost shelf, and the rest under it.
  const grouped = $derived.by(() => {
    const under = new Map<string, string[]>()
    for (const name of shelves) {
      const outermost = name.split('/')[0]
      const held = under.get(outermost)
      if (held) held.push(name)
      else under.set(outermost, [name])
    }
    return [...under].map(([outermost, names]) => ({
      outermost,
      names: [...names].sort((a, b) => a.localeCompare(b)),
      /// One shelf with nothing inside it does not need a heading of its own.
      alone: names.length === 1 && names[0] === outermost,
    }))
  })

  /// How deep a shelf sits, as the indent that says so in a plain option.
  const inset = (name: string) => '\u00a0\u00a0'.repeat(name.split('/').length - 1)
</script>

<select
  class="shelf"
  value={folder ?? ''}
  {disabled}
  aria-label={t('folders.move', { title })}
  onchange={(e) => pick(e.currentTarget.value)}
>
  <option value="">{t('folders.none')}</option>
  {#each grouped as group (group.outermost)}
    {#if group.alone}
      <option value={group.outermost}>{group.outermost}</option>
    {:else}
      <optgroup label={group.outermost}>
        {#each group.names as name (name)}
          <option value={name}>{inset(name)}{leafOf(name)}</option>
        {/each}
      </optgroup>
    {/if}
  {/each}
  <option value="__new__">{t('folders.new')}</option>
</select>

<style>
  /* Padding comes from the shared dropdown rule: set here it came to
     nineteen pixels, and this is how a work gets filed. */
  .shelf {
    width: 100%;
    font-size: var(--text-xs);
    color: var(--muted);
  }
</style>
