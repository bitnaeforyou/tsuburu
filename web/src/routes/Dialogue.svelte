<script lang="ts">
  import * as api from '../lib/api'
  import { t, type Key } from '../lib/i18n.svelte'
  import AppHeader from '../lib/AppHeader.svelte'
  import Card from '../lib/Card.svelte'
  import ErrorNote from '../lib/ErrorNote.svelte'
  import { toDialogue, toGallery } from '../lib/router'

  let { query }: { query: string } = $props()

  const LANGUAGES = ['all', 'korean', 'japanese', 'english']
  const KINDS = ['all', 'doujinshi', 'manga']
  const CAPS = [
    { label: '1 MB/s', value: 1 * 1024 * 1024 },
    { label: '3 MB/s', value: 3 * 1024 * 1024 },
    { label: '6 MB/s', value: 6 * 1024 * 1024 },
  ]

  let input = $state(query)
  let hits = $state<api.DialogueHit[]>([])
  let searching = $state(false)
  let searchError = $state<unknown>(null)
  let status = $state<api.DialogueStatus | null>(null)
  let statusError = $state<unknown>(null)

  let importText = $state('')
  let importForce = $state(false)
  let importResult = $state<string | null>(null)
  let hunt = $state({ q: '', language: 'korean', kind: 'doujinshi', limit: 500, force: false })
  let huntResult = $state<string | null>(null)

  // Two ways to ask: the words as written, or what they mean.
  let mode = $state<'words' | 'meaning'>('words')
  let cache = $state<{ items: api.Stored[]; total: number; bytes: number } | null>(null)
  let cacheBusy = $state(false)
  let forgetting = $state(false)
  let pack = $state<api.EmbedderSettings | null>(null)
  let packCheck = $state<api.PackCheck | null>(null)
  let packBusy = $state(false)
  let packError = $state<string | null>(null)

  let similarFor = $state<string | null>(null)
  let similar = $state<api.SimilarHit[]>([])
  let similarError = $state<string | null>(null)

  // The embeddings answer "what else reads like this", using the vector
  // already stored for the passage. No model is loaded to ask.
  async function showSimilar(hit: api.DialogueHit) {
    const key = `${hit.gallery_id}:${hit.page}`
    if (similarFor === key) {
      similarFor = null
      return
    }
    similarFor = key
    similar = []
    similarError = null
    try {
      similar = await api.similarScenes(hit.gallery_id, hit.page)
    } catch (cause) {
      similarError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  let artifactDir = $state('')
  let artifactResult = $state<string | null>(null)
  let backgroundOnly = $state(true)
  let shards = $state<api.ShardListing | null>(null)
  let exchangeResult = $state<string | null>(null)
  let exchanging = $state(false)

  let controller: AbortController | null = null

  $effect(() => {
    input = query
    if (query) void search()
    else hits = []
  })

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

  async function search() {
    controller?.abort()
    controller = new AbortController()
    searching = true
    searchError = null
    try {
      if (mode === 'meaning') {
        const found = await api.phraseScenes(query)
        hits = found.map((h) => ({ ...h, exact: false, also: [] }))
      } else {
        const result = await api.dialogueSearch(query, 25, controller.signal)
        hits = result.hits
      }
    } catch (cause) {
      if ((cause as Error).name !== 'AbortError') searchError = cause
    } finally {
      searching = false
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

  function submit(event: SubmitEvent) {
    event.preventDefault()
    location.hash = toDialogue(input.trim())
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
      importResult = `${r.found} ids found, ${r.added} queued ahead of everything else.`
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
      huntResult = `${r.found} galleries matched, ${r.added} queued.`
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
      artifactResult = `Importing ${p.chunks_total.toLocaleString()} chunks from ${p.directory}`
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
      exchangeResult = `${shards.files.length} file(s) written to ${shards.directory}`
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
        lines.push(`${file.name}: ${r.added} added, ${r.skipped} already here`)
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

<AppHeader active="dialogue" />

<main>
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
    <section class="panel status">
      <div class="row">
        <div>
          <strong>{t('dialogue.index')}</strong>
          {#if coverage}
            <span class="muted">
              {t('dialogue.coverageTop1k', { percent: percent(coverage.top_1k, 1000) })} ·
              {t('dialogue.coverageTop10k', {
                percent: percent(coverage.top_10k, coverage.top_10k_total),
              })} ·
              {t('dialogue.coverageAll', {
                done: coverage.done.toLocaleString(),
                total: coverage.total.toLocaleString(),
              })}
            </span>
          {:else if status?.counts}
            <span class="muted">
              {t('dialogue.indexed', { n: status.counts.done.toLocaleString() })}
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
              queue: status.counts.pending.toLocaleString(),
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

    <form class="searchbar" onsubmit={submit}>
      <input
        bind:value={input}
        placeholder={mode === 'meaning'
          ? t('dialogue.searchMeaning')
          : t('dialogue.searchWords')}
        aria-label={t('dialogue.searchLabel')}
        autocomplete="off"
      />
      <select bind:value={mode} aria-label={t('dialogue.mode')}>
        <option value="words">{t('dialogue.modeWords')}</option>
        <option value="meaning">{t('dialogue.modeMeaning')}</option>
      </select>
      <button type="submit" disabled={!input.trim()}>{t('dialogue.find')}</button>
    </form>
    {#if mode === 'meaning'}
      <p class="muted small">
        {t('dialogue.meaningNote')}
      </p>
    {/if}

    {#if searchError}
      <ErrorNote error={searchError} onretry={search} />
    {/if}

    {#if query && !searchError}
      {#if searching}
        <p class="muted">
          {status?.counts?.done
            ? t('dialogue.searchingCount', { n: status.counts.done.toLocaleString() })
            : t('dialogue.searching')}
        </p>
      {:else if hits.length === 0}
        <p class="muted">
          {t('dialogue.noHits')}
        </p>
      {:else}
        <p class="muted">{t('common.galleries', { n: hits.length })}</p>
        <ul class="hits">
          {#each hits as hit (hit.gallery_id)}
            <li class="hit">
              <div class="thumb"><Card id={hit.gallery_id} /></div>
              <div class="body">
                <a href={toGallery(hit.gallery_id, hit.page)}>
                  {t('common.page', { n: hit.page + 1 })}
                  {#if hit.exact}<span class="badge">{t('dialogue.exact')}</span>{:else}<span
                      class="badge fuzzy">~{Math.round(hit.score * 100)}%</span
                    >{/if}
                  {#if hit.also?.length}
                    <span class="badge fuzzy" title={hit.also.join(', ')}>
                      {hit.also.length > 1
                        ? t('dialogue.copies', { n: hit.also.length })
                        : t('dialogue.copy', { n: hit.also.length })}
                    </span>
                  {/if}
                </a>
                <blockquote>
                  {#each hit.snippet as line, i (i)}<span>{line}</span>{/each}
                </blockquote>
                <button class="similar-toggle" onclick={() => showSimilar(hit)}>
                  {similarFor === `${hit.gallery_id}:${hit.page}`
                    ? t('dialogue.similarHide')
                    : t('dialogue.similarShow')}
                </button>
                {#if similarFor === `${hit.gallery_id}:${hit.page}`}
                  {#if similarError}
                    <p class="muted small">{similarError}</p>
                  {:else if similar.length === 0}
                    <p class="muted small">{t('dialogue.similarSearching')}</p>
                  {:else}
                    <ul class="similar">
                      {#each similar as near (near.gallery_id)}
                        <li>
                          <a href={toGallery(near.gallery_id, near.page)}>
                            {t('common.page', { n: near.page + 1 })}
                            <span class="badge fuzzy">{Math.round(near.score * 100)}%</span>
                          </a>
                          <span class="muted small">{near.snippet.join(' / ')}</span>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}

    <section class="panel tools">
      <form onsubmit={submitImport}>
        <strong>{t('dialogue.importHistory')}</strong>
        <p class="muted small">{t('dialogue.importHistoryNote')}</p>
        <textarea bind:value={importText} rows="3" placeholder="https://hitomi.la/doujinshi/...-1234567.html"></textarea>
        <div class="row">
          <label class="check">
            <input type="checkbox" bind:checked={importForce} />
            {t('dialogue.readAgain')}
          </label>
          <button type="submit" disabled={!importText.trim()}>{t('dialogue.queue')}</button>
        </div>
        {#if importResult}<p class="muted small">{importResult}</p>{/if}
      </form>

      <form onsubmit={submitHunt}>
        <strong>{t('dialogue.hunt')}</strong>
        <p class="muted small">{t('dialogue.huntNote')}</p>
        <div class="row">
          <input
            bind:value={hunt.q}
            placeholder={t('dialogue.huntPlaceholder')}
            aria-label={t('dialogue.huntTerms')}
          />
          <select bind:value={hunt.language}>
            {#each LANGUAGES as l (l)}<option value={l}>{t(`lang.${l}` as Key)}</option>{/each}
          </select>
          <select bind:value={hunt.kind}>
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

      <form onsubmit={submitArtifact}>
        <strong>{t('dialogue.artifact')}</strong>
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
              {t('dialogue.importing', { n: status.import.works_seen.toLocaleString() })}
            {:else if status.import.error}
              {t('dialogue.importFailed', { error: status.import.error })}
            {:else}
              {t('dialogue.imported', {
                added: status.import.added.toLocaleString(),
                skipped: status.import.skipped.toLocaleString(),
              })}
            {/if}
          </p>
        {:else if artifactResult}
          <p class="muted small">{artifactResult}</p>
        {/if}
      </form>

      <form onsubmit={(e) => { e.preventDefault(); void savePack() }}>
        <strong>{t('dialogue.pack')}</strong>
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

      <div class="exchange">
        <strong>{t('dialogue.cache')}</strong>
        <p class="muted small">{t('dialogue.cacheNote')}</p>
        {#if cache && cache.total > 0}
          <div class="row">
            <span class="muted small">
              {t('dialogue.cacheSummary', {
                works: cache.total.toLocaleString(),
                size: formatBytes(cache.bytes),
              })}
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
                <div class="thumb"><Card id={item.id} /></div>
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

      <div class="exchange">
        <strong>{t('dialogue.share')}</strong>
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
  {/if}
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
    padding: 1rem;
    max-width: 64rem;
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
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
  .small {
    font-size: 0.85rem;
    margin: 0.4rem 0 0;
  }
  .check {
    margin-right: 0.6rem;
  }
  button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .searchbar {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  .searchbar input {
    flex: 1;
  }
  .hits {
    list-style: none;
    padding: 0;
    margin: 0 0 1.5rem;
    display: grid;
    gap: 0.75rem;
  }
  .hit {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 0.9rem;
    align-items: start;
  }
  .hit a {
    font-weight: 500;
    text-decoration: none;
  }
  .badge {
    font-size: 0.7rem;
    margin-left: 0.4rem;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    background: var(--accent);
    color: var(--bg);
  }
  .badge.fuzzy {
    background: var(--border);
    color: var(--text);
  }
  blockquote {
    margin: 0.4rem 0 0;
    padding: 0.5rem 0.75rem;
    border-left: 3px solid var(--accent);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .similar-toggle {
    margin-top: 0.4rem;
    font-size: 0.8rem;
    padding: 0.2rem 0.55rem;
  }
  .similar {
    list-style: none;
    margin: 0.5rem 0 0;
    padding: 0 0 0 0.75rem;
    border-left: 2px solid var(--border);
    display: grid;
    gap: 0.3rem;
    font-size: 0.85rem;
  }
  .similar a {
    text-decoration: none;
    margin-right: 0.5rem;
  }

  .tools {
    display: grid;
    gap: 1.25rem;
  }
  .tools form {
    display: grid;
    gap: 0.4rem;
  }
  textarea {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.5rem 0.7rem;
    resize: vertical;
  }
  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
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
    border: 1px solid var(--border);
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
    margin-right: 0.5rem;
  }
</style>
