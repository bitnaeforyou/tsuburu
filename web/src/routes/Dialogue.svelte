<script lang="ts">
  import * as api from '../lib/api'
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
      const result = await api.dialogueSearch(query, 25, controller.signal)
      hits = result.hits
    } catch (cause) {
      if ((cause as Error).name !== 'AbortError') searchError = cause
    } finally {
      searching = false
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
      <strong>Dialogue search is not available on this platform.</strong>
      <p class="muted">
        It relies on the operating system's text recognition. macOS is supported; other
        platforms are not yet.
      </p>
    </div>
  {:else}
    <section class="panel status">
      <div class="row">
        <div>
          <strong>Index</strong>
          {#if coverage}
            <span class="muted">
              top 1,000: {percent(coverage.top_1k, 1000)}% ·
              top 10,000: {percent(coverage.top_10k, coverage.top_10k_total)}% ·
              all: {coverage.done.toLocaleString()} / {coverage.total.toLocaleString()}
            </span>
          {:else if status?.counts}
            <span class="muted">{status.counts.done.toLocaleString()} galleries indexed</span>
          {:else}
            <span class="muted">loading…</span>
          {/if}
        </div>
        {#if settings}
          <button class:on={settings.enabled} onclick={toggleIndexing}>
            {settings.enabled ? 'Stop indexing' : 'Start indexing'}
          </button>
        {/if}
      </div>
      {#if running}
        <p class="muted small">
          {#if running.running && running.current}
            Indexing #{running.current}{running.current_title ? ` — ${running.current_title}` : ''}
            · {running.pages_per_second.toFixed(1)} pages/s
            · {running.galleries_this_session} galleries this session
          {:else if settings?.enabled}
            Waiting for the next gallery…
          {:else}
            Indexing is off. Nothing is downloaded until you start it.
          {/if}
          {#if status?.counts}
            · queue {status.counts.pending.toLocaleString()} · failed {status.counts.failed}
          {/if}
        </p>
        {#if running.last_error}
          <p class="muted small">Last problem: {running.last_error}</p>
        {/if}
      {/if}
      {#if settings}
        <div class="row small muted">
          <span>
            Types:
            {#each KINDS.filter((k) => k !== 'all') as kind (kind)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={settings.kinds.includes(kind)}
                  onchange={() => toggleKind(kind)}
                />
                {kind}
              </label>
            {/each}
          </span>
          <label class="check">
            <input
              type="checkbox"
              checked={settings.reindex_imported}
              onchange={(e) => updateSetting({ reindex_imported: e.currentTarget.checked })}
            />
            Re-read imported galleries
          </label>
          <label>
            Download cap
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
        placeholder="A line you remember, in Korean"
        aria-label="Dialogue search"
        autocomplete="off"
      />
      <button type="submit" disabled={!input.trim()}>Find</button>
    </form>

    {#if searchError}
      <ErrorNote error={searchError} onretry={search} />
    {/if}

    {#if query}
      {#if searching}
        <p class="muted">Searching {status?.counts?.done.toLocaleString() ?? ''} indexed galleries…</p>
      {:else if hits.length === 0}
        <p class="muted">
          Nothing indexed so far contains that. It may still be in a gallery that has not
          been indexed yet.
        </p>
      {:else}
        <p class="muted">{hits.length} galleries</p>
        <ul class="hits">
          {#each hits as hit (hit.gallery_id)}
            <li class="hit">
              <div class="thumb"><Card id={hit.gallery_id} /></div>
              <div class="body">
                <a href={toGallery(hit.gallery_id, hit.page)}>
                  Page {hit.page + 1}
                  {#if hit.exact}<span class="badge">exact</span>{:else}<span class="badge fuzzy">~{Math.round(hit.score * 100)}%</span>{/if}
                  {#if hit.also?.length}
                    <span class="badge fuzzy" title={hit.also.join(', ')}>
                      +{hit.also.length} copy{hit.also.length > 1 ? 'ies' : ''}
                    </span>
                  {/if}
                </a>
                <blockquote>
                  {#each hit.snippet as line, i (i)}<span>{line}</span>{/each}
                </blockquote>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}

    <section class="panel tools">
      <form onsubmit={submitImport}>
        <strong>Import reading history</strong>
        <p class="muted small">
          Paste hitomi URLs or gallery ids from your browser history or an old artifact
          database. They are indexed before anything else.
        </p>
        <textarea bind:value={importText} rows="3" placeholder="https://hitomi.la/doujinshi/...-1234567.html"></textarea>
        <div class="row">
          <label class="check">
            <input type="checkbox" bind:checked={importForce} />
            Read them again even if they already have text
          </label>
          <button type="submit" disabled={!importText.trim()}>Queue</button>
        </div>
        {#if importResult}<p class="muted small">{importResult}</p>{/if}
      </form>

      <form onsubmit={submitHunt}>
        <strong>Hunt</strong>
        <p class="muted small">
          Narrow with what you remember (tags, language, type) and queue those galleries
          ahead of the background sweep.
        </p>
        <div class="row">
          <input bind:value={hunt.q} placeholder="tags, e.g. 안경 거유" aria-label="Hunt terms" />
          <select bind:value={hunt.language}>
            {#each LANGUAGES as l (l)}<option value={l}>{l}</option>{/each}
          </select>
          <select bind:value={hunt.kind}>
            {#each KINDS as k (k)}<option value={k}>{k}</option>{/each}
          </select>
          <input type="number" bind:value={hunt.limit} min="1" max="5000" aria-label="Limit" />
          <label class="check">
            <input type="checkbox" bind:checked={hunt.force} />
            re-read
          </label>
          <button type="submit">Queue</button>
        </div>
        {#if huntResult}<p class="muted small">{huntResult}</p>{/if}
      </form>

      <form onsubmit={submitArtifact}>
        <strong>Import artifact's corpus</strong>
        <p class="muted small">
          If you have artifact's <code>llm-search-index</code> directory, its recognised
          Korean text can be loaded whole: about 108,000 galleries in under a minute, with
          no downloading or recognition.
        </p>
        <div class="row">
          <input bind:value={artifactDir} placeholder="/path/to/llm-search-index" aria-label="Directory" />
          <button type="submit" disabled={!artifactDir.trim() || status?.import?.running}>Import</button>
        </div>
        {#if status?.import}
          <p class="muted small">
            {#if status.import.running}
              Importing… {status.import.works_seen.toLocaleString()} galleries so far
            {:else if status.import.error}
              Import failed: {status.import.error}
            {:else}
              Imported {status.import.added.toLocaleString()} galleries
              ({status.import.skipped.toLocaleString()} were already here).
            {/if}
          </p>
        {:else if artifactResult}
          <p class="muted small">{artifactResult}</p>
        {/if}
      </form>

      <div class="exchange">
        <strong>Share the index</strong>
        <p class="muted small">
          Recognised text is small; the images are not. Export what this machine has read
          as shard files and hand them to someone else, or import theirs. Files are
          verified against the hash in their name.
        </p>
        <div class="row">
          <label class="check">
            <input type="checkbox" bind:checked={backgroundOnly} />
            Only background-indexed galleries (keeps what you chose to read out of the file)
          </label>
          <button onclick={exportShards} disabled={exchanging}>Export</button>
          <label class="upload">
            <input type="file" accept=".tsd" multiple onchange={importFiles} disabled={exchanging} />
            Import .tsd files
          </label>
        </div>
        {#if exchangeResult}<p class="muted small">{exchangeResult}</p>{/if}
        {#if shards && shards.files.length}
          <ul class="files">
            {#each shards.files as file (file.name)}
              <li>
                <a href={api.shardUrl(file.name)} download={file.name}>{file.name}</a>
                <span class="muted small">
                  {formatBytes(file.bytes)}{file.galleries ? ` · ${file.galleries} galleries` : ''}
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
