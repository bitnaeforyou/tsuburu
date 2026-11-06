/// Finding among what is downloaded, and copying what was found.

import { beforeEach, describe, expect, test, vi } from 'vitest'
import { fireEvent, render, waitFor } from '@testing-library/svelte'
import type { DownloadItem } from '../lib/api'
import { t } from '../lib/i18n.svelte'
import Downloads from './Downloads.svelte'

const item = (id: number, title: string): DownloadItem => ({
  id,
  title,
  language: 'korean',
  pages: 10,
  have: 10,
  bytes: 1024,
  added_at: id,
  complete: true,
  job: { running: false, wanted: 0, fetched: 0, failed: 0, error: null },
})

const copied = vi.hoisted(() => vi.fn())

vi.mock('../lib/clipboard', () => ({
  copy: async (text: string) => {
    copied(text)
    return true
  },
}))
vi.mock('../lib/AppHeader.svelte', () => ({ default: () => {} }))
vi.mock('../lib/Card.svelte', () => ({ default: () => {} }))
vi.mock('../lib/api', async (original) => ({
  ...(await original<typeof import('../lib/api')>()),
  downloads: async () => ({
    items: [item(4200001, '여름 방학'), item(4200002, '겨울 방학'), item(3100000, 'Summer')],
    bytes: 3072,
  }),
  folders: async () => [],
}))

async function opened() {
  const screen = render(Downloads)
  const box = await waitFor(() => screen.getByRole('searchbox'))
  const copyAll = screen.getByRole('button', { name: t('copy.all') })
  return { screen, box, copyAll }
}

beforeEach(() => copied.mockClear())

describe('the downloads list', () => {
  test('copies every number, separated by commas', async () => {
    const { copyAll } = await opened()
    await fireEvent.click(copyAll)
    await waitFor(() => expect(copied).toHaveBeenCalledWith('4200002, 4200001, 3100000'))
  })

  test('a search narrows what is listed and what is copied', async () => {
    const { screen, box, copyAll } = await opened()
    await fireEvent.input(box, { target: { value: '방학' } })
    await waitFor(() => expect(screen.container.querySelectorAll('.entry')).toHaveLength(2))
    await fireEvent.click(copyAll)
    await waitFor(() => expect(copied).toHaveBeenCalledWith('4200002, 4200001'))
  })

  test('says so when nothing answers', async () => {
    const { screen, box } = await opened()
    await fireEvent.input(box, { target: { value: '없는 제목' } })
    await waitFor(() => expect(screen.getByText(t('downloads.noMatch'))).toBeInTheDocument())
    expect(screen.container.querySelectorAll('.entry')).toHaveLength(0)
  })
})
