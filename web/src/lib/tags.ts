/// Splitting a hitomi tag into who it is about and what it says.
///
/// They arrive namespaced - `female:big breasts`, `male:sole male` - and the
/// prefix is the same handful of characters on every one, so a reader scanning
/// a wall of them reads it thirty times to learn nothing. Colour carries it
/// instead, and the word is shown on its own.
export type Tag = { word: string; who: 'female' | 'male' | null }

export function split(tag: string): Tag {
  if (tag.startsWith('female:')) return { word: tag.slice(7), who: 'female' }
  if (tag.startsWith('male:')) return { word: tag.slice(5), who: 'male' }
  return { word: tag, who: null }
}
