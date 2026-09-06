// 즐겨찾기 상태를 화면들이 공유한다.
//
// 카드마다 즐겨찾기 여부를 물으면 요청이 카드 수만큼 나간다. 목록을 한 번
// 받아 집합으로 들고 있으면 별 표시가 즉시 그려진다.

import * as api from './api'

class Library {
  ids = $state(new Set<number>())
  loaded = $state(false)
  /** 저장소를 못 열었을 때. 검색은 계속 동작해야 하므로 오류로 막지 않는다. */
  unavailable = $state(false)

  async load() {
    if (this.loaded) return
    try {
      const { items } = await api.favorites()
      this.ids = new Set(items.map((item) => item.id))
      this.loaded = true
    } catch (cause) {
      if (cause instanceof api.ApiError && cause.kind === 'storage') {
        this.unavailable = true
        this.loaded = true
        return
      }
      throw cause
    }
  }

  has(id: number): boolean {
    return this.ids.has(id)
  }

  async toggle(id: number, summary: Omit<api.Summary, 'id'>) {
    if (this.unavailable) return
    const next = new Set(this.ids)
    if (next.has(id)) {
      next.delete(id)
      this.ids = next
      await api.removeFavorite(id).catch(() => this.load())
    } else {
      next.add(id)
      this.ids = next
      await api.addFavorite(id, summary).catch(() => this.load())
    }
  }
}

export const library = new Library()
