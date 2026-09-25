/// Interface language. Content stays in whatever language hitomi holds it in;
/// this is only the wording around it.

import { forget as forgetCards } from './cards.svelte'
import { forget as forgetResults } from './results.svelte'
import { en } from './locales/en'
import { ko } from './locales/ko'
import { ja } from './locales/ja'

/// The English catalogue doubles as the list of keys that exist, so a code
/// the server invents without a translation falls back rather than showing
/// the key itself.
export const MESSAGES = en
export type Key = keyof typeof en
/// Values are plain strings: the English file is `as const` so that its keys
/// are exact, not so that its wording becomes a type.
export type Messages = Record<Key, string>

export const LOCALES = { en: 'English', ko: '한국어', ja: '日本語' } as const
export type Locale = keyof typeof LOCALES

const CATALOGUE: Record<Locale, Messages> = { en, ko, ja }
const STORED = 'tsuburu.locale'

function isLocale(value: string): value is Locale {
  return value in LOCALES
}

/// The saved choice, else the first of the browser's languages we speak.
function preferred(): Locale {
  try {
    const saved = localStorage.getItem(STORED)
    if (saved && isLocale(saved)) return saved
  } catch {
    // Private windows refuse storage; the browser's own list still works.
  }
  const tags = navigator.languages?.length ? navigator.languages : [navigator.language]
  for (const tag of tags) {
    const base = tag.toLowerCase().split('-')[0]
    if (isLocale(base)) return base
  }
  return 'en'
}

class I18n {
  locale = $state<Locale>(preferred())

  constructor() {
    // `set` is only called when someone picks; the language we start in is
    // just as much the document's language, and quoting, hyphenation and
    // every screen reader's pronunciation read it from there.
    document.documentElement.lang = this.locale
  }

  set(locale: Locale) {
    if (locale === this.locale) return
    this.locale = locale
    document.documentElement.lang = locale
    // A card carries its tags, and those come back in the language they were
    // asked for; what is held was asked for in the old one.
    forgetCards()
    forgetResults()
    try {
      localStorage.setItem(STORED, locale)
    } catch {
      // The choice lasts for this session only, which is better than failing.
    }
  }
}

export const i18n = new I18n()

/// Groups digits the way the interface language does, not the browser.
export function number(value: number): string {
  return value.toLocaleString(i18n.locale)
}

export function t(key: Key, values?: Record<string, string | number>): string {
  const message: string = CATALOGUE[i18n.locale][key]
  if (!values) return message
  return message.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in values ? String(values[name]) : whole,
  )
}
