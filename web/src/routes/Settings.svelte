<script lang="ts">
  import * as api from '../lib/api'
  import { t, type Key, number } from '../lib/i18n.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import Backup from '../lib/Backup.svelte'
  import { toDialogue } from '../lib/router'

  const LANGUAGES = ['all', 'korean', 'japanese', 'english']
  const KINDS = ['all', 'doujinshi', 'manga']
  const CAPS = [
    { label: '1 MB/s', value: 1 * 1024 * 1024 },
    { label: '3 MB/s', value: 3 * 1024 * 1024 },
    { label: '6 MB/s', value: 6 * 1024 * 1024 },
  ]

  let status = $state<api.DialogueStatus | null>(null)
  let statusError = $state<unknown>(null)

  let importText = $state('')
  let importForce = $state(false)
  let importResult = $state<string | null>(null)
  let hunt = $state({ q: '', language: 'korean', kind: 'doujinshi', limit: 500, force: false })
  let huntResult = $state<string | null>(null)

  let update = $state<api.UpdateState | null>(null)
  let model = $state<api.ModelState | null>(null)
  let corpus = $state<api.CorpusState | null>(null)
  let modelError = $state<string | null>(null)
  let cache = $state<{
    items: api.Stored[]
    total: number
    bytes: number
    vectors: number
  } | null>(null)
  let cacheBusy = $state(false)
  let forgetting = $state(false)
  let pack = $state<api.EmbedderSettings | null>(null)
  let packCheck = $state<api.PackCheck | null>(null)
  let packBusy = $state(false)
  let packError = $state<string | null>(null)


  let artifactDir = $state('')
  let artifactResult = $state<string | null>(null)
  let backgroundOnly = $state(true)
  let shards = $state<api.ShardListing | null>(null)
  let exchangeResult = $state<string | null>(null)
  let exchanging = $state(false)

  // Fetching several gigabytes takes long enough that the switch has to keep
  // saying so. Asked unconditionally: turning it on answers before the first
  // byte moves, so what it says at that moment is no guide to what to do next.
  $effect(() => {
    void refreshModel()
    const timer = setInterval(() => void refreshModel(), 1000)
    return () => clearInterval(timer)
  })

  async function refreshModel() {
    try {
      model = await api.modelState()
      modelError = null
    } catch (cause) {
      modelError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  // Same reason as the model: four hundred megabytes takes long enough that
  // the panel has to keep saying where it has got to.
  $effect(() => {
    void refreshCorpus()
    const timer = setInterval(() => void refreshCorpus(), 1000)
    return () => clearInterval(timer)
  })

  async function refreshCorpus() {
    try {
      corpus = await api.corpusState()
    } catch {
      // The panel simply does not appear; there is nothing to act on.
    }
  }

  async function getCorpus() {
    try {
      corpus = await api.fetchCorpus()
    } catch (cause) {
      corpus = {
        available: true,
        state: 'failed',
        error: cause instanceof Error ? cause.message : String(cause),
      }
    }
  }

  async function toggleModel() {
    const on = model?.state === 'ready' || model?.state === 'fetching' || model?.state === 'starting'
    try {
      model = on ? await api.disableModel() : await api.enableModel()
      modelError = null
    } catch (cause) {
      modelError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  /// What the weights weigh, so the switch can say so before it has them.
  const WEIGHTS = 2_496_703_776
  const gigabytes = (n: number) => `${(n / 1073741824).toFixed(1)} GB`
  const megabytes = (n: number) => `${(n / 1048576).toFixed(0)} MB`

  // The program asks once when it starts; this is only so the answer, and the
  // fetching after it, reach the screen.
  $effect(() => {
    void refreshUpdate()
    const timer = setInterval(() => void refreshUpdate(), 1500)
    return () => clearInterval(timer)
  })

  async function refreshUpdate() {
    try {
      update = await api.updateState()
    } catch {
      // Nothing to say if the question itself could not be asked.
    }
  }

  // The status line is what makes a miss readable as "not yet" instead of
  // "does not exist", so keep it fresh while the page is open.
  $effect(() => {
    void refreshStatus()
    const timer = setInterval(() => void refreshStatus(), 5000)
    return () => clearInterval(timer)
  })

  async function refreshStatus() {
    try {
      status = await api.dialogueStatus()
      statusError = null
    } catch (cause) {
      statusError = cause
    }
  }

  $effect(() => {
    void api.getPack().then((p) => (pack = p)).catch(() => {})
    void loadCache()
  })

  async function loadCache() {
    try {
      cache = await api.stored(true, 24)
    } catch {
      cache = null
    }
  }

  async function forgetOne(id: number) {
    cacheBusy = true
    try {
      await api.forget(id)
      await loadCache()
    } finally {
      cacheBusy = false
    }
  }

  async function forgetEverything() {
    cacheBusy = true
    try {
      await api.forgetRead()
      await loadCache()
      forgetting = false
    } finally {
      cacheBusy = false
    }
  }

  async function savePack() {
    if (!pack) return
    packBusy = true
    packError = null
    packCheck = null
    try {
      pack = await api.setPack(pack)
      packCheck = await api.checkPack()
    } catch (cause) {
      packError = cause instanceof Error ? cause.message : String(cause)
    } finally {
      packBusy = false
    }
  }

  async function toggleIndexing() {
    if (!status?.settings) return
    try {
      await api.updateGrinder({ ...status.settings, enabled: !status.settings.enabled })
      await refreshStatus()
    } catch (cause) {
      statusError = cause
    }
  }

  async function updateSetting(changes: Partial<api.GrinderSettings>) {
    if (!status?.settings) return
    try {
      await api.updateGrinder({ ...status.settings, ...changes })
      await refreshStatus()
    } catch (cause) {
      statusError = cause
    }
  }

  async function toggleKind(kind: string) {
    if (!status?.settings) return
    const kinds = status.settings.kinds.includes(kind)
      ? status.settings.kinds.filter((k) => k !== kind)
      : [...status.settings.kinds, kind]
    if (kinds.length === 0) return
    await updateSetting({ kinds })
  }

  async function submitImport(event: SubmitEvent) {
    event.preventDefault()
    importResult = null
    try {
      const r = await api.enqueueDialogue(importText, importForce)
      importResult = t('dialogue.importQueued', { found: r.found, added: r.added })
      importText = ''
      await refreshStatus()
    } catch (cause) {
      importResult = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function submitHunt(event: SubmitEvent) {
    event.preventDefault()
    huntResult = null
    try {
      const r = await api.huntDialogue({
        q: hunt.q,
        language: hunt.language,
        kind: hunt.kind,
        limit: hunt.limit,
        force: hunt.force,
      })
      huntResult = t('dialogue.huntQueued', { found: r.found, added: r.added })
      await refreshStatus()
    } catch (cause) {
      huntResult = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function submitArtifact(event: SubmitEvent) {
    event.preventDefault()
    artifactResult = null
    try {
      const p = await api.importArtifact(artifactDir)
      artifactResult = t('dialogue.artifactStarted', {
        chunks: number(p.chunks_total),
        directory: p.directory,
      })
      await refreshStatus()
    } catch (cause) {
      artifactResult = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function exportShards() {
    exchanging = true
    exchangeResult = null
    try {
      shards = await api.exportShards(backgroundOnly)
      exchangeResult = t('dialogue.exported', {
        files: shards.files.length,
        directory: shards.directory,
      })
    } catch (cause) {
      exchangeResult = cause instanceof Error ? cause.message : String(cause)
    } finally {
      exchanging = false
    }
  }

  async function importFiles(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const files = Array.from(input.files ?? [])
    if (files.length === 0) return
    exchanging = true
    exchangeResult = null
    const lines: string[] = []
    for (const file of files) {
      try {
        const r = await api.importShard(file)
        lines.push(
          t('dialogue.shardImported', {
            file: file.name,
            added: r.added,
            skipped: r.skipped,
          }),
        )
      } catch (cause) {
        lines.push(`${file.name}: ${cause instanceof Error ? cause.message : String(cause)}`)
      }
    }
    exchangeResult = lines.join(' · ')
    exchanging = false
    input.value = ''
    await refreshStatus()
  }

  const formatBytes = (n: number) =>
    n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`

  const percent = (n: number, d: number) => (d > 0 ? Math.round((n / d) * 100) : 0)
  const coverage = $derived(status?.coverage ?? null)
  const settings = $derived(status?.settings ?? null)
  const running = $derived(status?.status ?? null)
</script>

<AppHeader active="settings" />

<main>
  <h1>{t('nav.settings')}</h1>
  <section class="panel version" class:offer={update?.state === 'found'}>
      <div class="row">
        <div>
          <h2>{t('update.title')}</h2>
          <p class="muted small">
            {#if update?.state === 'found'}
              {t('update.found', { version: update.version ?? '' })}
              {#if update.notes}&middot; {update.notes}{/if}
            {:else if update?.state === 'fetching'}
              {t('update.fetching', {
                done: megabytes(update.done ?? 0),
                total: megabytes(update.total ?? 0),
              })}
            {:else if update?.state === 'ready'}
              {t('update.ready', { version: update.version ?? '' })}
            {:else if update?.state === 'failed'}
              {t('update.failed', { error: update.error ?? '' })}
            {:else if update && !update.published}
              {t('update.unpublished')}
            {:else if update?.state === 'checking'}
              {t('update.checking')}
            {:else}
              {t('update.none')}
            {/if}
          </p>
        </div>

        {#if update?.state === 'found'}
          <button class="primary" onclick={() => void api.applyUpdate().then((u) => (update = u))}>
            {t('update.get', { version: update.version ?? '' })}
          </button>
        {:else if update?.published && update.state !== 'fetching' && update.state !== 'ready'}
          <button onclick={() => void api.checkUpdate().then((u) => (update = u))}>
            {t('update.check')}
          </button>
        {/if}
      </div>
      <p class="muted small">{t('update.here', { version: update?.here ?? '' })}</p>
    </section>

  <!-- Everything below reads dialogue out of pictures, which not every
       platform can do. The notice replaces those panels alone: an update and
       a backup are no business of the dialogue's, and hiding them left a
       phone with a settings screen that was one sentence long. -->
  {#if status && !status.supported}
    <div class="panel">
      <strong>{t('dialogue.unsupported')}</strong>
      <p class="muted">
        {status.note
          ? t('dialogue.unsupportedNote', { note: status.note })
          : t('dialogue.unsupportedPlain')}
      </p>
    </div>
  {:else}
    <section class="panel switch">
        <div class="row">
          <div>
            <h2>{t('model.title')}</h2>
            <p class="muted small">{t('model.note', { size: gigabytes(WEIGHTS) })}</p>
          </div>
          <button
            class:on={model?.state === 'ready'}
            disabled={!model}
            onclick={toggleModel}
            aria-pressed={model?.state === 'ready'}
          >
            {model?.state === 'off' || model?.state === 'failed'
              ? model?.kept
                ? t('model.turnOn')
                : t('model.getIt', { size: gigabytes(WEIGHTS) })
              : t('model.turnOff')}
          </button>
        </div>

        {#if model?.state === 'fetching'}
          <div class="meter" style:--done={`${model.total ? ((model.done ?? 0) / model.total) * 100 : 0}%`}>
            <span>
              {model.what === 'model' ? t('model.gettingWeights') : t('model.gettingServer')}
              &middot; {gigabytes(model.done ?? 0)} / {gigabytes(model.total ?? 0)}
            </span>
          </div>
        {:else if model?.state === 'starting'}
          <p class="muted small">{t('model.starting')}</p>
        {:else if model?.state === 'ready'}
          <p class="muted small">{t('model.ready')}</p>
        {:else if model?.state === 'failed'}
          <p class="muted small">{t('model.failed', { error: model.error ?? '' })}</p>
        {:else if model?.kept}
          <p class="muted small">{t('model.kept', { size: gigabytes(model.bytes) })}</p>
        {/if}
        {#if modelError}<p class="muted small">{modelError}</p>{/if}
    </section>

    <!-- The text somebody else's machine already spent weeks recognising.
         One button, because a reader should not have to go and find a
         directory to be able to search. -->
    {#if corpus?.available && corpus.state !== 'ready'}
      <section class="panel switch">
        <div class="row">
          <div>
            <h2>{t('corpus.title')}</h2>
            <p class="muted small">{t('corpus.note')}</p>
            {#if corpus.state === 'failed'}
              <p class="muted small">{t('corpus.failed', { error: corpus.error ?? '' })}</p>
            {/if}
          </div>
          {#if corpus.state === 'fetching'}
            <button disabled>{t('common.loading')}</button>
          {:else}
            <button class="primary" onclick={getCorpus}>
              {corpus.state === 'failed' ? t('corpus.again') : t('corpus.get')}
            </button>
          {/if}
        </div>
        {#if corpus.state === 'fetching'}
          <div class="meter" style:--done={`${((corpus.done ?? 0) / Math.max(corpus.total ?? 1, 1)) * 100}%`}>
            <span>
              {t('corpus.fetching', {
                done: corpus.done ?? 0,
                total: corpus.total ?? 0,
                works: number(corpus.works ?? 0),
              })}
            </span>
          </div>
        {/if}
      </section>
    {/if}

    <section class="panel status">
      <div class="row">
        <div>
          <h2>{t('dialogue.index')}</h2>
          {#if coverage}
            <span class="muted">
              {t('dialogue.coverageTop1k', { percent: percent(coverage.top_1k, 1000) })} ·
              {t('dialogue.coverageTop10k', {
                percent: percent(coverage.top_10k, coverage.top_10k_total),
              })} ·
              {t('dialogue.coverageAll', {
                done: number(coverage.done),
                total: number(coverage.total),
              })}
            </span>
          {:else if status?.counts}
            <span class="muted">
              {t('dialogue.indexed', { n: number(status.counts.done) })}
            </span>
          {:else}
            <span class="muted">{t('dialogue.loadingIndex')}</span>
          {/if}
        </div>
        {#if settings}
          <button class:on={settings.enabled} onclick={toggleIndexing}>
            {settings.enabled ? t('dialogue.stop') : t('dialogue.start')}
          </button>
        {/if}
      </div>
      {#if running}
        <p class="muted small">
          {#if running.running && running.current}
            {t('dialogue.indexingNow', { id: running.current })}{running.current_title
              ? ` — ${running.current_title}`
              : ''}
            · {t('dialogue.rate', { rate: running.pages_per_second.toFixed(1) })}
            · {t('dialogue.thisSession', { n: running.galleries_this_session })}
          {:else if settings?.enabled}
            {t('dialogue.waiting')}
          {:else}
            {t('dialogue.off')}
          {/if}
          {#if status?.counts}
            · {t('dialogue.queued', {
              queue: number(status.counts.pending),
              failed: status.counts.failed,
            })}
          {/if}
        </p>
        {#if running.last_error}
          <p class="muted small">{t('dialogue.lastProblem', { error: running.last_error })}</p>
        {/if}
      {/if}
      {#if settings}
        <div class="row small muted">
          <span>
            {t('dialogue.types')}
            {#each KINDS.filter((k) => k !== 'all') as kind (kind)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={settings.kinds.includes(kind)}
                  onchange={() => toggleKind(kind)}
                />
                {t(`kind.${kind}` as Key)}
              </label>
            {/each}
          </span>
          <label class="check">
            <input
              type="checkbox"
              checked={settings.read_indexing}
              onchange={(e) => updateSetting({ read_indexing: e.currentTarget.checked })}
            />
            {t('dialogue.readIndexing')}
          </label>
          <label class="check">
            <input
              type="checkbox"
              checked={settings.reindex_imported}
              onchange={(e) => updateSetting({ reindex_imported: e.currentTarget.checked })}
            />
            {t('dialogue.reindexImported')}
          </label>
          <label>
            {t('dialogue.downloadCap')}
            <select
              value={settings.bytes_per_second}
              onchange={(e) => updateSetting({ bytes_per_second: Number(e.currentTarget.value) })}
            >
              {#each CAPS as cap (cap.value)}
                <option value={cap.value}>{cap.label}</option>
              {/each}
            </select>
          </label>
        </div>
      {/if}
      {#if statusError}
        <ErrorNote error={statusError} onretry={refreshStatus} />
      {/if}
    </section>

    <section class="panel">
      <form onsubmit={submitImport}>
        <h2>{t('dialogue.importHistory')}</h2>
        <p class="muted small">{t('dialogue.importHistoryNote')}</p>
        <textarea
          bind:value={importText}
          rows="3"
          aria-label={t('dialogue.importHistory')}
          placeholder="https://hitomi.la/doujinshi/…-1234567.html"
        ></textarea>
        <div class="row">
          <label class="check">
            <input type="checkbox" bind:checked={importForce} />
            {t('dialogue.readAgain')}
          </label>
          <button type="submit" disabled={!importText.trim()}>{t('dialogue.queue')}</button>
        </div>
        {#if importResult}<p class="muted small">{importResult}</p>{/if}
      </form>
    </section>

    <section class="panel">
      <form onsubmit={submitHunt}>
        <h2>{t('dialogue.hunt')}</h2>
        <p class="muted small">{t('dialogue.huntNote')}</p>
        <div class="row">
          <input
            bind:value={hunt.q}
            placeholder={t('dialogue.huntPlaceholder')}
            aria-label={t('dialogue.huntTerms')}
          />
          <select bind:value={hunt.language} aria-label={t('search.language')}>
            {#each LANGUAGES as l (l)}<option value={l}>{t(`lang.${l}` as Key)}</option>{/each}
          </select>
          <select bind:value={hunt.kind} aria-label={t('search.type')}>
            {#each KINDS as k (k)}<option value={k}>{t(`kind.${k}` as Key)}</option>{/each}
          </select>
          <input
            type="number"
            bind:value={hunt.limit}
            min="1"
            max="5000"
            aria-label={t('dialogue.limit')}
          />
          <label class="check">
            <input type="checkbox" bind:checked={hunt.force} />
            {t('dialogue.reread')}
          </label>
          <button type="submit">{t('dialogue.queue')}</button>
        </div>
        {#if huntResult}<p class="muted small">{huntResult}</p>{/if}
      </form>
    </section>

    <section class="panel">
      <div class="exchange">
        <h2>{t('dialogue.cache')}</h2>
        <p class="muted small">{t('dialogue.cacheNote')}</p>
        {#if cache && cache.total > 0}
          <div class="row">
            <span class="muted small">
              {t('dialogue.cacheSummary', {
                works: number(cache.total),
                size: formatBytes(cache.bytes),
              })}{cache.vectors > 0
                ? ` · ${t('dialogue.cacheVectors', { n: number(cache.vectors) })}`
                : ''}
            </span>
            {#if forgetting}
              <span class="muted small">{t('dialogue.forgetConfirm')}</span>
              <button onclick={forgetEverything} disabled={cacheBusy}>
                {t('history.confirmYes')}
              </button>
              <button onclick={() => (forgetting = false)}>{t('common.cancel')}</button>
            {:else}
              <button onclick={() => (forgetting = true)} disabled={cacheBusy}>
                {t('dialogue.forgetAll')}
              </button>
            {/if}
          </div>
          <ul class="kept">
            {#each cache.items as item (item.id)}
              <li>
                <div class="thumb"><Card id={item.id} level={3} /></div>
                <span class="muted small">
                  {t('common.pages', { n: item.pages })} &middot; {formatBytes(item.bytes)}
                </span>
                <button onclick={() => forgetOne(item.id)} disabled={cacheBusy}>
                  {t('dialogue.forget')}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted small">{t('dialogue.cacheEmpty')}</p>
        {/if}
      </div>

    </section>

    <section class="panel">
      <div class="exchange">
        <h2>{t('dialogue.share')}</h2>
        <p class="muted small">{t('dialogue.shareNote')}</p>
        <div class="row">
          <label class="check">
            <input type="checkbox" bind:checked={backgroundOnly} />
            {t('dialogue.exportSwept')}
          </label>
          <button onclick={exportShards} disabled={exchanging}>{t('dialogue.export')}</button>
          <label class="upload">
            <input type="file" accept=".tsd" multiple onchange={importFiles} disabled={exchanging} />
            {t('dialogue.importShards')}
          </label>
        </div>
        {#if exchangeResult}<p class="muted small">{exchangeResult}</p>{/if}
        {#if shards && shards.files.length}
          <ul class="files">
            {#each shards.files as file (file.name)}
              <li>
                <a href={api.shardUrl(file.name)} download={file.name}>{file.name}</a>
                <span class="muted small">
                  {formatBytes(file.bytes)}{file.galleries
                    ? ` · ${t('common.galleries', { n: file.galleries })}`
                    : ''}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </section>

    <details class="advanced">
      <summary>{t('settings.advanced')}</summary>
      <p class="muted small">{t('settings.advancedNote')}</p>
      <form onsubmit={submitArtifact}>
        <h2>{t('dialogue.artifact')}</h2>
        <p class="muted small">{t('dialogue.artifactNote')}</p>
        <div class="row">
          <input
            bind:value={artifactDir}
            placeholder="/path/to/llm-search-index"
            aria-label={t('dialogue.directory')}
          />
          <button type="submit" disabled={!artifactDir.trim() || status?.import?.running}>
            {t('dialogue.import')}
          </button>
        </div>
        {#if status?.import}
          <p class="muted small">
            {#if status.import.running}
              {t('dialogue.importing', { n: number(status.import.works_seen) })}
            {:else if status.import.error}
              {t('dialogue.importFailed', { error: status.import.error })}
            {:else}
              {t('dialogue.imported', {
                added: number(status.import.added),
                skipped: number(status.import.skipped),
              })}
            {/if}
          </p>
        {:else if artifactResult}
          <p class="muted small">{artifactResult}</p>
        {/if}
      </form>
      <form onsubmit={(e) => { e.preventDefault(); void savePack() }}>
        <h2>{t('dialogue.pack')}</h2>
        <p class="muted small">{t('dialogue.packNote')}</p>
        {#if pack}
          <div class="row">
            <input bind:value={pack.url} aria-label={t('dialogue.packUrl')} />
            <input bind:value={pack.model} aria-label={t('dialogue.packModel')} />
            <button type="submit" disabled={packBusy}>{t('dialogue.packSave')}</button>
          </div>
        {/if}
        {#if packBusy}
          <p class="muted small">{t('dialogue.packAsking')}</p>
        {:else if packCheck}
          <p class="muted small">
            {packCheck.ok ? '✓' : '✗'}
            {t('dialogue.packResult', {
              cosine: packCheck.cosine.toFixed(3),
              gallery: packCheck.sample_gallery,
              page: packCheck.sample_page + 1,
              note: packCheck.note,
            })}
          </p>
        {:else if packError}
          <p class="muted small">{packError}</p>
        {/if}
      </form>
    </details>
  {/if}

  <Backup />
</main>

<style>
  .kept {
    list-style: none;
    padding: 0;
    margin: 0.6rem 0 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.8rem;
  }
  .kept li {
    display: grid;
    gap: 0.25rem;
    align-content: start;
  }

  main {
    max-width: var(--page-narrow);
    margin-inline: auto;
    padding: 1rem;
  }

  h1 {
    margin: 0.25rem 0 1rem;
  }

  /* Every box says what it is in the same voice, so the page reads as a list
     of things rather than a wall. */
  h2 {
    /* The body's size, told apart by weight: ten of these down a page at a
       step up would be ten headlines shouting over the settings. */
    font-size: var(--text-base);
    margin: 0;
  }
  h2 + .small {
    margin-top: 0.25rem;
  }
  .panel form,
  .panel .exchange {
    display: grid;
    gap: 0.4rem;
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.9rem 1rem;
    margin-bottom: 1rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: center;
    justify-content: space-between;
  }
  .muted {
    color: var(--muted);
  }
  /* What a setting is for, in a sentence. A sentence that runs the width of
     this page is one the eye loses its place in. */
  .small {
    font-size: var(--text-sm);
    margin: 0.4rem 0 0;
    max-width: 68ch;
    text-wrap: pretty;
  }
  .check {
    margin-inline-end: 0.6rem;
  }
  button.on {
    border-color: var(--accent);
    color: var(--accent);
  }

  textarea {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--edge);
    border-radius: var(--radius);
    padding: 0.5rem 0.7rem;
    resize: vertical;
  }
  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--edge);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
  }
  input[type='number'] {
    width: 6rem;
  }
  .exchange {
    display: grid;
    gap: 0.4rem;
  }
  .upload {
    cursor: pointer;
    border: 1px solid var(--edge);
    border-radius: var(--radius);
    padding: 0.45rem 0.8rem;
    background: var(--surface);
  }
  .upload input {
    display: none;
  }
  .files {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
  }
  .files a {
    margin-inline-end: 0.5rem;
  }

  /* On a phone the line you are trying to remember needs the whole width;
     the two controls that qualify it go under it. */
  @media (max-width: 640px) {
    main {
      padding: 0.9rem;
    }
    .panel {
      padding: 0.8rem 0.9rem;
    }
  }

  /* A version is a quiet line until there is a newer one to offer, and then
     it is the first thing on the page. */
  .version .row {
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: nowrap;
  }
  .version .row > div {
    flex: 1;
    min-width: 0;
  }
  .version.offer {
    border-color: var(--accent);
  }

  /* The one switch a reader is ever likely to touch, so it reads as one:
     what it does on the left, what it costs on the button. */
  .switch .row {
    align-items: flex-start;
    gap: 1rem;
    flex-wrap: nowrap;
  }
  /* The words take what is left; the switch stays where a switch goes. */
  .switch .row > div {
    flex: 1;
    min-width: 0;
  }
  .switch button {
    flex: none;
    white-space: nowrap;
  }
  .switch button.on {
    color: var(--accent);
    border-color: var(--accent);
  }

  .meter {
    position: relative;
    margin-top: 0.6rem;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    font-size: var(--text-xs);
    /* Counts here climb while a download runs. */
    font-variant-numeric: tabular-nums;
    padding: 0.2rem 0.5rem;
    overflow: hidden;
  }
  .meter::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--done);
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  @media (prefers-reduced-motion: no-preference) {
    .meter::before {
      transition: width 200ms linear;
    }
  }
  .meter span {
    position: relative;
  }

  .advanced {
    margin-bottom: 1rem;
    color: var(--muted);
    font-size: var(--text-md);
    max-width: 68ch;
  }
  .advanced summary {
    cursor: pointer;
    padding: 0.4rem 0;
  }
  .advanced :global(strong) {
    color: var(--text);
  }
</style>
