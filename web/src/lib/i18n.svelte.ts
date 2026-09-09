/// Interface language. Content stays in whatever language hitomi holds it in;
/// this is only the wording around it.

import { en } from './locales/en'
import { ko } from './locales/ko'
import { ja } from './locales/ja'

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

  set(locale: Locale) {
    this.locale = locale
    document.documentElement.lang = locale
    try {
      localStorage.setItem(STORED, locale)
    } catch {
      // The choice lasts for this session only, which is better than failing.
    }
  }
}

export const i18n = new I18n()

export function t(key: Key, values?: Record<string, string | number>): string {
  const message: string = CATALOGUE[i18n.locale][key]
  if (!values) return message
  return message.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in values ? String(values[name]) : whole,
  )
}
