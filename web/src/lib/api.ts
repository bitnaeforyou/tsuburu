export type ApiErrorKind = 'format_changed' | 'network' | 'bad_request' | 'storage' | 'unsupported'

export class ApiError extends Error {
  constructor(
    readonly kind: ApiErrorKind,
    message: string,
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
  tags: string[]
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

export type Favorite = Summary & { added_at: number }
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
}

export function dialogueStatus(): Promise<DialogueStatus> {
  return request('/api/dialogue/status')
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

export function enqueueDialogue(text: string): Promise<{ found: number; added: number }> {
  return send('POST', '/api/dialogue/enqueue', { text, priority: 'imported' })
}

export function huntDialogue(body: {
  q: string
  language?: string
  kind?: string
  limit?: number
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
