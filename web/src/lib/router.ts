// 해시 라우터. 라우팅 요구가 두 화면뿐이라 라이브러리를 쓰지 않는다.

export type Route = { name: 'search'; query: string } | { name: 'gallery'; id: number }

export function parse(hash: string): Route {
  const path = hash.replace(/^#/, '')
  const gallery = /^\/g\/(\d+)$/.exec(path)
  if (gallery) return { name: 'gallery', id: Number(gallery[1]) }

  const search = /^\/(?:\?q=(.*))?$/.exec(path)
  return { name: 'search', query: search?.[1] ? decodeURIComponent(search[1]) : '' }
}

export function toSearch(query: string): string {
  return query ? `#/?q=${encodeURIComponent(query)}` : '#/'
}

export function toGallery(id: number): string {
  return `#/g/${id}`
}
