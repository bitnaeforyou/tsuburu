// What the reader has said they want to see, kept between launches.
//
// The language and the kind live in the address so a link carries them, which
// also meant they were gone the next time the program opened - and a reader
// who only reads Korean had to say so again every time. A link that names
// them still wins; one that does not now means "what I usually read".

import { keep, kept } from './kept'

const STORED = 'tsuburu.search'

type Stored = { language: string; kind: string }

const EVERYTHING: Stored = { language: 'all', kind: 'all' }

function read(): Stored {
  const raw = kept(STORED)
  if (!raw) return { ...EVERYTHING }
  try {
    const parsed = JSON.parse(raw) as Partial<Stored>
    return {
      language: typeof parsed.language === 'string' ? parsed.language : EVERYTHING.language,
      kind: typeof parsed.kind === 'string' ? parsed.kind : EVERYTHING.kind,
    }
  } catch {
    return { ...EVERYTHING }
  }
}

export const preferred: Stored = read()

export function remember(next: Partial<Stored>) {
  if (next.language !== undefined) preferred.language = next.language
  if (next.kind !== undefined) preferred.kind = next.kind
  keep(STORED, JSON.stringify(preferred))
}
