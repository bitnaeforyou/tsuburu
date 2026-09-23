<script lang="ts">
  import { t } from './i18n.svelte'
  import { chosen } from './folders'

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
</script>

<select
  class="shelf"
  value={folder ?? ''}
  {disabled}
  aria-label={t('folders.move', { title })}
  onchange={(e) => pick(e.currentTarget.value)}
>
  <option value="">{t('folders.none')}</option>
  {#each shelves as name (name)}
    <option value={name}>{name}</option>
  {/each}
  <option value="__new__">{t('folders.new')}</option>
</select>

<style>
  .shelf {
    width: 100%;
    font-size: var(--text-xs);
    padding: 0.2rem 0.3rem;
    color: var(--muted);
  }
</style>
