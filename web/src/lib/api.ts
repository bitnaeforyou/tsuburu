export type ApiErrorKind = 'format_changed' | 'network' | 'bad_request' | 'storage' | 'unsupported'

export class ApiError extends Error {
  constructor(
    readonly kind: ApiErrorKind,
    message: string,
    /**
     * Names the few failures the reader can act on, so the interface can
     * say what to do in their language. The message stays English: it is
     * the diagnostic underneath.
     */
    readonly code?: string,
  ) {
    super(message)
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response
  try {
    response = await fetch(path, init)
  } catch (cause) {
    if ((cause as Error).name === 'AbortError') throw cause
    throw new ApiError('network', 'could not reach the tsuburu server')
  }

  if (!response.ok) {
    const body = await response.json().catch(() => null)
    throw new ApiError(
      body?.error ?? 'network',
      body?.message ?? `request failed with ${response.status}`,
      body?.code,
    )
  }
  return response.json() as Promise<T>
}

function send<T>(method: string, path: string, body?: unknown): Promise<T> {
  return request<T>(path, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
}

export type Term = {
  input: string
  used: string
  alternatives: string[]
  translated: boolean
  excluded: boolean
}

export type SearchResponse = { total: number; ids: number[]; terms: Term[] }

export type Card = {
  id: number
  title: string | null
  kind: string | null
  language: string | null
  pages: number
  artists: string[]
  tags: string[]
  /// Whether hitomi still lists it. Absent until the list has been read.
  listed?: boolean
  thumbnail: string | null
}

export type Page = { src: string; width: number; height: number }

export type Gallery = {
  id: number
  title: string | null
  japanese_title: string | null
  kind: string | null
  language: string | null
  date: string | null
  tags: string[]
  artists: string[]
  series: string[]
  pages: Page[]
}

export type Summary = {
  id: number
  title: string | null
  language: string | null
  kind: string | null
  pages: number
  thumbnail_hash: string | null
}

export type Favorite = Summary & { added_at: number; folder?: string | null }
export type Folder = { name: string; works: number }
export type HistoryEntry = Summary & { last_seen_at: number; last_page: number }

export type SearchParams = {
  q: string
  offset: number
  limit: number
  sort?: string
  language?: string
  kind?: string
}

export function search(params: SearchParams, signal?: AbortSignal): Promise<SearchResponse> {
  const search = new URLSearchParams({
    q: params.q,
    offset: String(params.offset),
    limit: String(params.limit),
  })
  if (params.sort && params.sort !== 'date') search.set('sort', params.sort)
  if (params.language && params.language !== 'all') search.set('language', params.language)
  if (params.kind && params.kind !== 'all') search.set('kind', params.kind)
  return request(`/api/search?${search}`, { signal })
}

export function cards(ids: number[], signal?: AbortSignal): Promise<Card[]> {
  return request(`/api/cards?ids=${ids.join(',')}`, { signal })
}

export function gallery(id: number, signal?: AbortSignal): Promise<Gallery> {
  return request(`/api/gallery/${id}`, { signal })
}

// --- 라이브러리 ---

export function favorites(): Promise<{ items: Favorite[] }> {
  return request('/api/favorites')
}

export function addFavorite(id: number, summary: Omit<Summary, 'id'>): Promise<Favorite> {
  return send('PUT', `/api/favorites/${id}`, summary)
}

export function removeFavorite(id: number): Promise<{ removed: boolean }> {
  return send('DELETE', `/api/favorites/${id}`)
}

/// Shelves are only their names and the works that name them, so there is
/// nothing to create and none is left behind empty.
export function folders(): Promise<Folder[]> {
  return request('/api/folders')
}

export function setFolder(id: number, folder: string | null): Promise<Favorite> {
  return send('PUT', `/api/favorites/${id}/folder`, { folder })
}

export type HiddenTags = { tags: string[] }

/// Tags no screen ever shows, whatever was asked for.
export function hiddenTags(): Promise<HiddenTags> {
  return request('/api/hidden')
}

export function setHiddenTags(tags: string[]): Promise<HiddenTags> {
  return send('PUT', '/api/hidden', { tags })
}

export type KeptCards = { cards: number }

/// Covers and titles held from earlier runs, and throwing them away.
export function keptCards(): Promise<KeptCards> {
  return request('/api/kept')
}

export function forgetKeptCards(): Promise<KeptCards> {
  return send('DELETE', '/api/kept')
}

export function history(): Promise<{ items: HistoryEntry[] }> {
  return request('/api/history')
}

export function recordProgress(
  id: number,
  page: number,
  summary: Omit<Summary, 'id'>,
): Promise<HistoryEntry> {
  return send('PUT', `/api/history/${id}`, { page, ...summary })
}

export function clearHistory(): Promise<{ removed: boolean }> {
  return send('DELETE', '/api/history')
}

// --- dialogue ---

export type GrinderSettings = {
  enabled: boolean
  language: string
  kinds: string[]
  bytes_per_second: number
  reindex_imported: boolean
  read_indexing: boolean
}

export type GrinderStatus = {
  running: boolean
  current: number | null
  current_title: string | null
  pages_per_second: number
  last_error: string | null
  galleries_this_session: number
  pages_this_session: number
}

export type Counts = { pending: number; done: number; failed: number }

export type Coverage = {
  top_1k: number
  top_10k: number
  top_10k_total: number
  done: number
  total: number
}

export type DialogueStatus = {
  supported: boolean
  // What the platform is missing, when installing it would help.
  note?: string
  settings?: GrinderSettings
  status?: GrinderStatus
  counts?: Counts
  coverage?: Coverage
  import?: ImportProgress
}

export type DialogueHit = {
  gallery_id: number
  page: number
  score: number
  exact: boolean
  snippet: string[]
  /** Other galleries carrying the same passage: re-uploads of one work. */
  also?: number[]
}

export function dialogueStatus(): Promise<DialogueStatus> {
  return request('/api/dialogue/status')
}

export type CorpusState = {
  available: boolean
  state: 'idle' | 'fetching' | 'ready' | 'failed'
  done?: number
  total?: number
  works?: number
  error?: string
}

export function corpusState(): Promise<CorpusState> {
  return request('/api/dialogue/corpus')
}

export function fetchCorpus(): Promise<CorpusState> {
  return send('POST', '/api/dialogue/corpus')
}

export function updateGrinder(settings: GrinderSettings): Promise<GrinderSettings> {
  return send('PUT', '/api/dialogue/settings', settings)
}

export function dialogueSearch(
  q: string,
  limit = 25,
  signal?: AbortSignal,
): Promise<{ hits: DialogueHit[]; counts: Counts }> {
  const params = new URLSearchParams({ q, limit: String(limit) })
  return request(`/api/dialogue/search?${params}`, { signal })
}

export function enqueueDialogue(
  text: string,
  force = false,
): Promise<{ found: number; added: number }> {
  return send('POST', '/api/dialogue/enqueue', { text, priority: 'imported', force })
}

export function huntDialogue(body: {
  q: string
  language?: string
  kind?: string
  limit?: number
  force?: boolean
}): Promise<{ found: number; added: number }> {
  return send('POST', '/api/dialogue/hunt', body)
}

// --- shard exchange ---

export type ShardFile = { name: string; bytes: number; galleries: number }
export type ShardListing = { directory: string; files: ShardFile[] }
export type ImportSummary = { galleries: number; added: number; skipped: number }

export function exportShards(backgroundOnly: boolean): Promise<ShardListing> {
  return send('POST', '/api/dialogue/export', { background_only: backgroundOnly })
}

export function listShards(): Promise<ShardListing> {
  return request('/api/dialogue/shards')
}

export function shardUrl(name: string): string {
  return `/api/dialogue/shards/${encodeURIComponent(name)}`
}

export async function importShard(file: File): Promise<ImportSummary> {
  return request(`/api/dialogue/import?name=${encodeURIComponent(file.name)}`, {
    method: 'POST',
    headers: { 'content-type': 'application/octet-stream' },
    body: file,
  })
}

// --- metadata snapshot ---

export type MetaStatus = { available: boolean; works?: number; latest_id?: number }

export function metaStatus(): Promise<MetaStatus> {
  return request('/api/meta/status')
}

export function metaSearch(
  params: { q: string; language?: string; kind?: string; offset: number; limit: number },
  signal?: AbortSignal,
): Promise<{ total: number; ids: number[] }> {
  const search = new URLSearchParams({
    q: params.q,
    offset: String(params.offset),
    limit: String(params.limit),
  })
  if (params.language && params.language !== 'all') search.set('language', params.language)
  if (params.kind && params.kind !== 'all') search.set('kind', params.kind)
  return request(`/api/meta/search?${search}`, { signal })
}

export type ImportProgress = {
  running: boolean
  directory: string
  chunks_total: number
  works_seen: number
  added: number
  skipped: number
  error: string | null
}

export function importArtifact(dir: string): Promise<ImportProgress> {
  return send('POST', '/api/dialogue/import-artifact', { dir })
}

export type SimilarHit = { gallery_id: number; page: number; score: number; snippet: string[] }

// --- downloads ---

export type DownloadItem = {
  id: number
  title: string | null
  language: string | null
  pages: number
  have: number
  bytes: number
  added_at: number
  complete: boolean
  job: { running: boolean; wanted: number; fetched: number; failed: number; error: string | null }
}

export function downloads(): Promise<{ items: DownloadItem[]; bytes: number }> {
  return request('/api/downloads')
}

export function downloadStatus(id: number): Promise<DownloadItem> {
  return request(`/api/downloads/${id}`)
}

export function startDownload(
  id: number,
  pages: number[] = [],
): Promise<{ id: number; wanted: number; already_here: number }> {
  return send('POST', `/api/downloads/${id}`, { pages })
}

export function removeDownload(id: number): Promise<{ removed: boolean }> {
  return send('DELETE', `/api/downloads/${id}`)
}

// --- artists ---

export type ArtistResponse = {
  name: string
  total: number
  ids: number[]
  following: boolean
  languages: [string, number][]
}

export function artist(
  name: string,
  offset = 0,
  limit = 25,
  language?: string,
): Promise<ArtistResponse> {
  return byName('artists', name, offset, limit, language)
}

/// A series reads the same way an artist does, and comes from the same kind
/// of list; only hitomi indexes them by name at all.
export function series(
  name: string,
  offset = 0,
  limit = 25,
  language?: string,
): Promise<ArtistResponse> {
  return byName('series', name, offset, limit, language)
}

function byName(
  where: 'artists' | 'series',
  name: string,
  offset: number,
  limit: number,
  language?: string,
): Promise<ArtistResponse> {
  const params = new URLSearchParams({ offset: String(offset), limit: String(limit) })
  if (language && language !== 'all') params.set('language', language)
  return request(`/api/${where}/${encodeURIComponent(name)}?${params}`)
}

export type FollowedArtist = { name: string; works: number; recent: number[] }

export function followedArtists(): Promise<FollowedArtist[]> {
  return request('/api/artists/following')
}

export function followArtist(name: string): Promise<{ following: boolean }> {
  return send('PUT', `/api/artists/${encodeURIComponent(name)}/follow`)
}

export function unfollowArtist(name: string): Promise<{ following: boolean }> {
  return send('DELETE', `/api/artists/${encodeURIComponent(name)}/follow`)
}

// --- phrase search over the embeddings ---

/// Whether a newer tsuburu has been published, and how far along becoming it
/// has got.
export type UpdateState = {
  state: 'idle' | 'checking' | 'found' | 'fetching' | 'ready' | 'failed'
  version?: string
  notes?: string
  done?: number
  total?: number
  error?: string
  here: string
  published: boolean
}

export function updateState(): Promise<UpdateState> {
  return request('/api/update')
}

export function checkUpdate(): Promise<UpdateState> {
  return send('POST', '/api/update/check')
}

export function applyUpdate(): Promise<UpdateState> {
  return send('POST', '/api/update/apply')
}

/// The model that turns a phrase into a vector: what it is doing, and whether
/// its weights are already on this machine.
export type ModelState = {
  state: 'off' | 'fetching' | 'starting' | 'ready' | 'failed'
  what?: string
  done?: number
  total?: number
  error?: string
  kept: boolean
  bytes: number
}

export function modelState(): Promise<ModelState> {
  return request('/api/model')
}

export function enableModel(): Promise<ModelState> {
  return send('POST', '/api/model/enable')
}

export function disableModel(): Promise<ModelState> {
  return send('POST', '/api/model/disable')
}

export type EmbedderSettings = { url: string; model: string }

export function getPack(): Promise<EmbedderSettings> {
  return request('/api/dialogue/pack')
}

export function setPack(settings: EmbedderSettings): Promise<EmbedderSettings> {
  return send('PUT', '/api/dialogue/pack', settings)
}

export type PackCheck = {
  ok: boolean
  cosine: number
  sample_gallery: number
  sample_page: number
  note: string
}

export function checkPack(): Promise<PackCheck> {
  return request('/api/dialogue/pack/check')
}

export function phraseScenes(q: string, limit = 25): Promise<SimilarHit[]> {
  return request(`/api/dialogue/phrase?${new URLSearchParams({ q, limit: String(limit) })}`)
}

// --- keywords, out of an imported graph.csv ---

export type Keyword = { word: string; score: number }

export function keywords(id: number): Promise<{ id: number; words: Keyword[] }> {
  return request(`/api/keywords/${id}`)
}

export type NearWork = { id: number; score: number; shared: string[] }

export function nearWorks(id: number, limit = 12): Promise<NearWork[]> {
  return request(`/api/keywords/${id}/near?limit=${limit}`)
}

export type KeywordSearch = {
  word: string
  works: { id: number; score: number }[]
  /** Set when the word was left out for being in this many works. */
  too_common?: number
}

export function keywordSearch(word: string, limit = 25): Promise<KeywordSearch> {
  const params = new URLSearchParams({ q: word, limit: String(limit) })
  return request(`/api/keywords/search?${params}`)
}

// --- what recognition has kept ---

export type Stored = {
  id: number
  pages: number
  lines: number
  bytes: number
  finished_at: number | null
  from_reading: boolean
  imported: boolean
}

export function stored(
  readingOnly = true,
  limit = 50,
): Promise<{ items: Stored[]; total: number; bytes: number; vectors: number }> {
  const params = new URLSearchParams({ reading_only: String(readingOnly), limit: String(limit) })
  return request(`/api/dialogue/stored?${params}`)
}

export function forget(id: number): Promise<{ removed: boolean }> {
  return send('DELETE', `/api/dialogue/stored/${id}`)
}

export function forgetRead(): Promise<{ removed: number }> {
  return send('DELETE', '/api/dialogue/stored')
}
