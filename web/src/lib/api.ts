export type ApiErrorKind = 'format_changed' | 'network' | 'bad_request'

export class ApiError extends Error {
  constructor(
    readonly kind: ApiErrorKind,
    message: string,
  ) {
    super(message)
  }
}

async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  let response: Response
  try {
    response = await fetch(path, { signal })
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

export type SearchResponse = { total: number; ids: number[] }

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

export function search(
  q: string,
  offset: number,
  limit: number,
  signal?: AbortSignal,
): Promise<SearchResponse> {
  const params = new URLSearchParams({ q, offset: String(offset), limit: String(limit) })
  return get(`/api/search?${params}`, signal)
}

export function cards(ids: number[], signal?: AbortSignal): Promise<Card[]> {
  return get(`/api/cards?ids=${ids.join(',')}`, signal)
}

export function gallery(id: number, signal?: AbortSignal): Promise<Gallery> {
  return get(`/api/gallery/${id}`, signal)
}
