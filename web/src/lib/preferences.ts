// What the reader has said they want to see, kept between launches.
//
// The language and the kind live in the address so a link carries them, which
// also meant they were gone the next time the program opened - and a reader
// who only reads Korean had to say so again every time. A link that names
// them still wins; one that does not now means "what I usually read".

const STORED = 'tsuburu.search'

type Stored = { language: string; kind: string }

function read(): Stored {
  try {
    const raw = localStorage.getItem(STORED)
    if (!raw) return { language: 'all', kind: 'all' }
    const parsed = JSON.parse(raw) as Partial<Stored>
    return {
      language: typeof parsed.language === 'string' ? parsed.language : 'all',
      kind: typeof parsed.kind === 'string' ? parsed.kind : 'all',
    }
  } catch {
    return { language: 'all', kind: 'all' }
  }
}

export const preferred: Stored = read()

export function remember(next: Partial<Stored>) {
  if (next.language !== undefined) preferred.language = next.language
  if (next.kind !== undefined) preferred.kind = next.kind
  try {
    localStorage.setItem(STORED, JSON.stringify(preferred))
  } catch {
    // A browser with storage switched off still gets to search.
  }
}
