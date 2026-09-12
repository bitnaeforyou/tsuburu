// 해시 라우터. 화면이 셋뿐이라 라이브러리를 쓰지 않는다.
//
// 정렬과 필터도 해시에 담는다. 그래야 뒤로가기와 새로고침, 링크 공유가
// 사용자가 기대하는 대로 동작한다.

export type Sort = 'date' | 'today' | 'week' | 'month' | 'year'

export type Scope = 'all' | 'hitomi' | 'local' | 'dialogue'

/// Two ways to ask the dialogue: the words as written, or what they mean.
export type Mode = 'words' | 'meaning'

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
  /** Only the dialogue can be asked either way; elsewhere this is ignored. */
  mode: Mode
}

export type Route =
  | ({ name: 'search' } & SearchState)
  | { name: 'gallery'; id: number; page: number | null }
  | { name: 'favorites' }
  | { name: 'history' }
  | { name: 'settings' }
  | { name: 'artist'; artist: string }
  | { name: 'downloads' }
  | { name: 'keyword'; word: string }

const SORTS: Sort[] = ['date', 'today', 'week', 'month', 'year']
const MODES: Mode[] = ['words', 'meaning']
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
  mode: 'words',
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
  if (head === '/settings') return { name: 'settings' }
  // The dialogue had a screen of its own before the searching moved to the one
  // box and the rest became settings. Its old addresses still lead somewhere.
  if (head === '/dialogue') {
    const asked = params.get('q')
    if (!asked) return { name: 'settings' }
    return { name: 'search', ...defaultSearch, query: asked, scope: 'dialogue' }
  }
  if (head === '/downloads') return { name: 'downloads' }
  const artist = /^\/artist\/(.+)$/.exec(head)
  if (artist) return { name: 'artist', artist: decodeURIComponent(artist[1]) }
  const keyword = /^\/keyword\/(.+)$/.exec(head)
  if (keyword) return { name: 'keyword', word: decodeURIComponent(keyword[1]) }

  const sort = params.get('sort') as Sort | null
  const mode = params.get('mode') as Mode | null
  return {
    name: 'search',
    query: params.get('q') ?? '',
    sort: sort && SORTS.includes(sort) ? sort : 'date',
    language: params.get('language') || 'all',
    kind: params.get('kind') || 'all',
    scope: asScope(params.get('scope')),
    mode: mode && MODES.includes(mode) ? mode : 'words',
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
  if (merged.mode !== defaultSearch.mode) params.set('mode', merged.mode)
  const query = params.toString()
  return query ? `#/?${query}` : '#/'
}

export function toGallery(id: number, page?: number): string {
  return page !== undefined ? `#/g/${id}?p=${page}` : `#/g/${id}`
}

/// The gallery a query names outright, rather than describes.
///
/// hitomi's index maps words to works, so a number is not a search term in it
/// at all: it is an address. Someone who pastes one - or the page it came
/// from - means open that, and searching for it can only ever find nothing.
/// Four digits or fewer stay a search, because a year is a thing people look
/// for and a gallery that old is not.
export function galleryNamed(query: string): number | null {
  const text = query.trim()
  if (text === '' || /\s/.test(text)) return null

  const bare = /^#?(\d{5,})$/.exec(text)
  if (bare) return Number(bare[1])

  if (!/(^|\/\/|\.)hitomi\.la\//i.test(text)) return null
  const address = text.replace(/[?#].*$/, '').replace(/\.html$/i, '')
  const found = /(?:^|[-/])(\d{5,})$/.exec(address)
  return found ? Number(found[1]) : null
}

/// A series, looked for where series are indexed: the local snapshot. hitomi's
/// own tag index has no term for one.
export function toSeries(name: string): string {
  return toSearch({ ...defaultSearch, query: `series:${name}`, scope: 'local' })
}

export function toArtist(name: string): string {
  return `#/artist/${encodeURIComponent(name)}`
}

export function toKeyword(word: string): string {
  return `#/keyword/${encodeURIComponent(word)}`
}

export function toSettings(): string {
  return '#/settings'
}

/// The dialogue is asked for in the one search box like everything else; this
/// is the address that narrows it to that source.
export function toDialogue(query = '', mode: Mode = 'words'): string {
  return toSearch({ ...defaultSearch, query, scope: 'dialogue', mode })
}
