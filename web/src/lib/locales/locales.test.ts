/// The catalogues only work if they agree: the same keys, the same
/// placeholders, and nothing in the interface asking for a key nobody wrote.

import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, test } from 'vitest'
import { en } from './en'
import { ja } from './ja'
import { ko } from './ko'

const source = join(process.cwd(), 'src')
const catalogues = { ko, ja } as const

function placeholders(message: string): string[] {
  return [...message.matchAll(/\{(\w+)\}/g)].map((found) => found[1]).sort()
}

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry): string[] => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return files(path)
    return /\.(svelte|ts)$/.test(entry.name) ? [path] : []
  })
}

describe.each(Object.entries(catalogues))('%s', (_name, catalogue) => {
  test('says everything English says, and nothing else', () => {
    expect(Object.keys(catalogue).sort()).toEqual(Object.keys(en).sort())
  })

  test('leaves nothing blank', () => {
    for (const [key, message] of Object.entries(catalogue)) {
      expect(message.trim(), key).not.toBe('')
    }
  })

  test('keeps the values a message is given', () => {
    for (const [key, message] of Object.entries(catalogue)) {
      expect(placeholders(message), key).toEqual(placeholders(en[key as keyof typeof en]))
    }
  })
})

test('every key the interface asks for exists', () => {
  const asked = new Set<string>()
  for (const path of files(source)) {
    if (path.includes('/locales/')) continue
    for (const found of readFileSync(path, 'utf8').matchAll(/\bt\(\s*'([\w.]+)'/g)) {
      asked.add(found[1])
    }
  }

  expect(asked.size).toBeGreaterThan(20)
  expect([...asked].filter((key) => !(key in en))).toEqual([])
})
