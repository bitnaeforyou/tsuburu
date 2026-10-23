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

/// How far into each work the reader got.
///
/// The history screen drew a progress bar on every cover and no other screen
/// did, so a search result gave no sign you had already read it. One list,
/// read once, and every card can say.
class Read {
  entries = $state(new Map<number, { page: number; pages: number }>())
  private asked = false

  async load() {
    if (this.asked) return
    this.asked = true
    try {
      const { items } = await api.history()
      this.entries = new Map(
        items.map((item) => [item.id, { page: item.last_page, pages: item.pages }]),
      )
    } catch {
      // Without a library there is no history, and a search still works.
    }
  }

  of(id: number) {
    return this.entries.get(id) ?? null
  }

  /// Said as a work is read, so a cover shows where you got to the moment
  /// you come back to it rather than the next time the program opens.
  note(id: number, page: number, pages: number) {
    if (pages <= 0) return
    const next = new Map(this.entries)
    next.set(id, { page, pages })
    this.entries = next
  }

  /// Said when the history is thrown away.
  forget() {
    this.entries = new Map()
  }
}

export const read = new Read()

/// How much of a work is downloaded.
export type Kept = { have: number; pages: number; complete: boolean }

/// Which works are downloaded, and how much of each.
///
/// Only the downloads screen knew, so a reader scanning a list for a work
/// they already had had to go and look there. Read once, like the history,
/// and kept current by the two screens that change it.
class Downloaded {
  entries = $state(new Map<number, Kept>())
  private asked = false

  async load() {
    if (this.asked) return
    this.asked = true
    try {
      this.replace((await api.downloads()).items)
    } catch {
      // A phone or a computer with downloads switched off has none to mark.
    }
  }

  of(id: number): Kept | null {
    return this.entries.get(id) ?? null
  }

  /// Everything on disk, as the downloads screen has just been told.
  replace(items: api.DownloadItem[]) {
    this.entries = new Map(items.map((item) => [item.id, keptOf(item)]))
  }

  /// One work, as the work's own screen has just been told.
  note(item: api.DownloadItem) {
    this.entries = new Map(this.entries).set(item.id, keptOf(item))
  }

  forget(id: number) {
    if (!this.entries.has(id)) return
    const next = new Map(this.entries)
    next.delete(id)
    this.entries = next
  }
}

function keptOf(item: api.DownloadItem): Kept {
  return { have: item.have, pages: item.pages, complete: item.complete }
}

export const downloaded = new Downloaded()
