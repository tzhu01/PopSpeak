// Keep these limits aligned with stt::hotwords in the native recognizers.
// Terms are data, not instructions; reserve enough decoder context for audio.
export const HOTWORD_LIMITS = { count: 32, termCharacters: 32, budget: 160 } as const

export function selectRecognitionHotwords(words: string[]): string[] {
  const selected: string[] = []
  const seen = new Set<string>()
  let budget = 0
  for (const raw of words) {
    const word = raw.trim()
    const characters = Array.from(word)
    if (
      !word ||
      characters.length > HOTWORD_LIMITS.termCharacters ||
      /[\p{Cc}<>[\]{}|,]/u.test(word) ||
      seen.has(word)
    )
      continue
    const cost = characters.reduce((sum, char) => sum + (char.codePointAt(0)! <= 127 ? 1 : 2), 2)
    if (selected.length >= HOTWORD_LIMITS.count || budget + cost > HOTWORD_LIMITS.budget) continue
    selected.push(word)
    seen.add(word)
    budget += cost
  }
  return selected
}
