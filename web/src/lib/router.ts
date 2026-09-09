// 해시 라우터. 화면이 셋뿐이라 라이브러리를 쓰지 않는다.
//
// 정렬과 필터도 해시에 담는다. 그래야 뒤로가기와 새로고침, 링크 공유가
// 사용자가 기대하는 대로 동작한다.

export type Sort = 'date' | 'today' | 'week' | 'month' | 'year'

export type Scope = 'hitomi' | 'local'

export type SearchState = {
  query: string
  sort: Sort
  language: string
  kind: string
  /** `hitomi` searches the remote tag index; `local` the metadata snapshot. */
  scope: Scope
}

export type Route =
  | ({ name: 'search' } & SearchState)
  | { name: 'gallery'; id: number; page: number | null }
  | { name: 'favorites' }
  | { name: 'history' }
  | { name: 'dialogue'; query: string }
  | { name: 'downloads' }

const SORTS: Sort[] = ['date', 'today', 'week', 'month', 'year']

export const defaultSearch: SearchState = {
  query: '',
  sort: 'date',
  language: 'all',
  kind: 'all',
  scope: 'hitomi',
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

  const sort = params.get('sort') as Sort | null
  return {
    name: 'search',
    query: params.get('q') ?? '',
    sort: sort && SORTS.includes(sort) ? sort : 'date',
    language: params.get('language') || 'all',
    kind: params.get('kind') || 'all',
    scope: params.get('scope') === 'local' ? 'local' : 'hitomi',
  }
}

export function toSearch(state: Partial<SearchState> = {}): string {
  const merged = { ...defaultSearch, ...state }
  const params = new URLSearchParams()
  if (merged.query) params.set('q', merged.query)
  if (merged.sort !== 'date') params.set('sort', merged.sort)
  if (merged.language !== 'all') params.set('language', merged.language)
  if (merged.kind !== 'all') params.set('kind', merged.kind)
  if (merged.scope !== 'hitomi') params.set('scope', merged.scope)
  const query = params.toString()
  return query ? `#/?${query}` : '#/'
}

export function toGallery(id: number, page?: number): string {
  return page !== undefined ? `#/g/${id}?p=${page}` : `#/g/${id}`
}

export function toDialogue(query = ''): string {
  return query ? `#/dialogue?q=${encodeURIComponent(query)}` : '#/dialogue'
}
