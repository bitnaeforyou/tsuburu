// 해시 라우터. 화면이 셋뿐이라 라이브러리를 쓰지 않는다.
//
// 정렬과 필터도 해시에 담는다. 그래야 뒤로가기와 새로고침, 링크 공유가
// 사용자가 기대하는 대로 동작한다.

export type Sort = 'date' | 'today' | 'week' | 'month' | 'year'

export type Scope = 'all' | 'hitomi' | 'local' | 'dialogue'

export type SearchState = {
  query: string
  sort: Sort
  language: string
  kind: string
  /**
   * `all` asks every source at once and shows them in sections; the rest
   * narrow to one - hitomi's tag index, the metadata snapshot, or the
   * recognised dialogue.
   */
  scope: Scope
}

export type Route =
  | ({ name: 'search' } & SearchState)
  | { name: 'gallery'; id: number; page: number | null }
  | { name: 'favorites' }
  | { name: 'history' }
  | { name: 'dialogue'; query: string }
  | { name: 'artist'; artist: string }
  | { name: 'downloads' }
  | { name: 'keyword'; word: string }

const SORTS: Sort[] = ['date', 'today', 'week', 'month', 'year']
const SCOPES: Scope[] = ['all', 'hitomi', 'local', 'dialogue']

function asScope(value: string | null): Scope {
  return value && (SCOPES as string[]).includes(value) ? (value as Scope) : 'all'
}

export const defaultSearch: SearchState = {
  query: '',
  sort: 'date',
  language: 'all',
  kind: 'all',
  scope: 'all',
}

export function parse(hash: string): Route {
  const path = hash.replace(/^#/, '') || '/'
  const [head, rawParams] = path.split('?', 2)

  const params = new URLSearchParams(rawParams ?? '')
  const gallery = /^\/g\/(\d+)$/.exec(head)
  if (gallery) {
    const page = params.get('p')
    return { name: 'gallery', id: Number(gallery[1]), page: page ? Number(page) : null }
  }
  if (head === '/favorites') return { name: 'favorites' }
  if (head === '/history') return { name: 'history' }
  if (head === '/dialogue') return { name: 'dialogue', query: params.get('q') ?? '' }
  if (head === '/downloads') return { name: 'downloads' }
  const artist = /^\/artist\/(.+)$/.exec(head)
  if (artist) return { name: 'artist', artist: decodeURIComponent(artist[1]) }
  const keyword = /^\/keyword\/(.+)$/.exec(head)
  if (keyword) return { name: 'keyword', word: decodeURIComponent(keyword[1]) }

  const sort = params.get('sort') as Sort | null
  return {
    name: 'search',
    query: params.get('q') ?? '',
    sort: sort && SORTS.includes(sort) ? sort : 'date',
    language: params.get('language') || 'all',
    kind: params.get('kind') || 'all',
    scope: asScope(params.get('scope')),
  }
}

export function toSearch(state: Partial<SearchState> = {}): string {
  const merged = { ...defaultSearch, ...state }
  const params = new URLSearchParams()
  if (merged.query) params.set('q', merged.query)
  if (merged.sort !== 'date') params.set('sort', merged.sort)
  if (merged.language !== 'all') params.set('language', merged.language)
  if (merged.kind !== 'all') params.set('kind', merged.kind)
  if (merged.scope !== defaultSearch.scope) params.set('scope', merged.scope)
  const query = params.toString()
  return query ? `#/?${query}` : '#/'
}

export function toGallery(id: number, page?: number): string {
  return page !== undefined ? `#/g/${id}?p=${page}` : `#/g/${id}`
}

export function toArtist(name: string): string {
  return `#/artist/${encodeURIComponent(name)}`
}

export function toKeyword(word: string): string {
  return `#/keyword/${encodeURIComponent(word)}`
}

export function toDialogue(query = ''): string {
  return query ? `#/dialogue?q=${encodeURIComponent(query)}` : '#/dialogue'
}
