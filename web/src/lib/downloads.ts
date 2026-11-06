/// Finding a work among those downloaded, and handing their numbers on.

/// Composed and lower-cased, so a title stored in decomposed Hangul or another
/// case still answers what was typed.
const comparable = (text: string) => text.normalize('NFC').toLowerCase()

/// Whether a work has every word asked for in its title or number.
export function matchesAsked(work: { id: number; title: string | null }, asked: string): boolean {
  const words = comparable(asked).split(/\s+/).filter(Boolean)
  const said = comparable(`${work.title ?? ''} #${work.id}`)
  return words.every((word) => said.includes(word))
}

/// Numbers in the form the box that takes them shows as its own example.
export const numberList = (ids: number[]) => ids.join(', ')
