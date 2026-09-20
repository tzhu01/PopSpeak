const HAN_CHARACTER = /^[\u3400-\u9fff]$/

interface WordSegment {
  segment: string
  index: number
  isWordLike?: boolean
}

interface WordSegmenter {
  segment(input: string): Iterable<WordSegment>
}

type SegmenterConstructor = new (
  locales?: string | string[],
  options?: { granularity: 'word' },
) => WordSegmenter

function isHanText(value: string) {
  const characters = [...value]
  return characters.length > 0 && characters.every((character) => HAN_CHARACTER.test(character))
}

function chineseWordAt(text: string, start: number, end: number) {
  const Segmenter = (Intl as typeof Intl & { Segmenter?: SegmenterConstructor }).Segmenter
  if (!Segmenter) return null

  const segmenter = new Segmenter('zh-CN', { granularity: 'word' })
  for (const part of segmenter.segment(text)) {
    const partStart = [...text.slice(0, part.index)].length
    const partLength = [...part.segment].length
    const partEnd = partStart + partLength
    const overlapsChange = partStart < end && partEnd > start
    if (overlapsChange && part.isWordLike && partLength >= 2 && isHanText(part.segment)) {
      return { start: partStart, end: partEnd }
    }
  }
  return null
}

export function changedSpan(before: string, after: string) {
  const left = [...before]
  const right = [...after]
  let prefix = 0
  while (prefix < left.length && prefix < right.length && left[prefix] === right[prefix])
    prefix += 1
  let suffix = 0
  while (
    suffix < left.length - prefix &&
    suffix < right.length - prefix &&
    left[left.length - suffix - 1] === right[right.length - suffix - 1]
  ) {
    suffix += 1
  }

  let leftStart = prefix
  let rightStart = prefix
  let leftEnd = left.length - suffix
  let rightEnd = right.length - suffix

  const isSingleHanSubstitution =
    left.length === right.length &&
    leftEnd - leftStart === 1 &&
    rightEnd - rightStart === 1 &&
    HAN_CHARACTER.test(left[leftStart] ?? '') &&
    HAN_CHARACTER.test(right[rightStart] ?? '')

  if (isSingleHanSubstitution) {
    // Use both the original and corrected sentence: a misspelt ASR token is often not
    // segmented as a word, while the corrected token is (口屁 → 口癖), or vice versa
    // (专业 → 展业). Expanding to the union prevents learning a dangerous one-character rule.
    const originalWord = chineseWordAt(before, leftStart, leftEnd)
    const correctedWord = chineseWordAt(after, rightStart, rightEnd)
    const detectedWords = [originalWord, correctedWord].filter(
      (word): word is { start: number; end: number } => word !== null,
    )

    if (detectedWords.length > 0) {
      const expandedStart = Math.min(leftStart, ...detectedWords.map((word) => word.start))
      const expandedEnd = Math.max(leftEnd, ...detectedWords.map((word) => word.end))
      if (expandedEnd - expandedStart <= 8) {
        leftStart = expandedStart
        rightStart = expandedStart
        leftEnd = expandedEnd
        rightEnd = expandedEnd
      }
    } else {
      const sharedLeft =
        leftStart > 0 &&
        left[leftStart - 1] === right[rightStart - 1] &&
        HAN_CHARACTER.test(left[leftStart - 1])
      const sharedRight =
        leftEnd < left.length &&
        left[leftEnd] === right[rightEnd] &&
        HAN_CHARACTER.test(left[leftEnd])

      if (sharedLeft && !sharedRight) {
        leftStart -= 1
        rightStart -= 1
      } else if (sharedRight) {
        leftEnd += 1
        rightEnd += 1
      } else if (sharedLeft) {
        leftStart -= 1
        rightStart -= 1
      }
    }
  }

  return {
    from: left.slice(leftStart, leftEnd).join(''),
    to: right.slice(rightStart, rightEnd).join(''),
  }
}
