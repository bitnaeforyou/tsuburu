<script lang="ts">
  import * as api from './api'
  import { t } from './i18n.svelte'
  import { library } from './library.svelte'
  import { nameFor, NotABackup, pack, restore, unpack } from './backup'

  /// Taking what this machine remembers to another one, and back.
  let busy = $state(false)
  let note = $state<string | null>(null)
  let failure = $state<string | null>(null)
  let progress = $state<{ done: number; total: number } | null>(null)
  let file = $state<HTMLInputElement | null>(null)

  async function save() {
    busy = true
    failure = null
    note = null
    try {
      const [favorites, artists, history] = await Promise.all([
        api.favorites(),
        api.followedArtists().catch(() => []),
        api.history(),
      ])
      const backup = pack(
        favorites.items,
        artists.map((a) => a.name),
        history.items,
      )
      offer(JSON.stringify(backup, null, 2), nameFor())
      note = t('backup.saved', {
        works: backup.favorites.length,
        artists: backup.artists.length,
        read: backup.history.length,
      })
    } catch (cause) {
      failure = cause instanceof Error ? cause.message : String(cause)
    } finally {
      busy = false
    }
  }

  /// A blob the browser saves under a name, which is as close as a page gets
  /// to writing a file.
  function offer(text: string, name: string) {
    const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url
    link.download = name
    link.click()
    URL.revokeObjectURL(url)
  }

  async function load(event: Event) {
    const chosen = (event.currentTarget as HTMLInputElement).files?.[0]
    if (!chosen) return
    busy = true
    failure = null
    note = null
    progress = null
    try {
      const backup = unpack(await chosen.text())
      const result = await restore(
        backup,
        {
          favorite: api.addFavorite,
          follow: api.followArtist,
          progress: api.recordProgress,
        },
        (done, total) => (progress = { done, total }),
      )
      library.loaded = false
      await library.load().catch(() => {})
      note =
        result.failed === 0
          ? t('backup.restored', { n: result.done })
          : t('backup.restoredSome', { n: result.done - result.failed, failed: result.failed })
    } catch (cause) {
      failure = cause instanceof NotABackup ? t('backup.notOurs') : String(cause)
    } finally {
      busy = false
      progress = null
      if (file) file.value = ''
    }
  }
</script>

<section class="backup">
  <p class="what">{t('backup.what')}</p>
  <div class="row">
    <button onclick={save} disabled={busy}>{t('backup.save')}</button>
    <button onclick={() => file?.click()} disabled={busy}>{t('backup.load')}</button>
    <input
      bind:this={file}
      type="file"
      accept="application/json,.json"
      onchange={load}
      hidden
    />
    {#if progress}
      <span class="muted">{t('backup.putting', { done: progress.done, total: progress.total })}</span>
    {:else if note}
      <span class="muted">{note}</span>
    {/if}
  </div>
  {#if failure}<p class="bad">{failure}</p>{/if}
</section>

<style>
  .backup {
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid var(--line);
  }
  .what {
    margin: 0 0 0.5rem;
    color: var(--muted);
    font-size: var(--text-sm);
    max-width: 68ch;
    text-wrap: pretty;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }
  .row button {
    font-size: var(--text-md);
    padding: 0.35rem 0.7rem;
  }
  .muted {
    color: var(--muted);
    font-size: var(--text-sm);
  }
  .bad {
    color: var(--danger);
    font-size: var(--text-sm);
    margin: 0.5rem 0 0;
  }
</style>
