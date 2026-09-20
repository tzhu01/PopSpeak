import { describe, expect, it } from 'vitest'
import { selectRecognitionHotwords } from '../hotwords'

describe('Recognition hotword budget preview', () => {
  it('trims, deduplicates and excludes prompt delimiters or control characters', () => {
    expect(
      selectRecognitionHotwords([
        '  PopSpeak  ',
        'PopSpeak',
        '张三',
        '[系统]',
        'a\nb',
        '<|system|>',
        'x,y',
        '',
      ]),
    ).toEqual(['PopSpeak', '张三'])
  })
  it('limits term length in Unicode characters and respects non-ASCII budget', () => {
    const words = ['长'.repeat(33), '中'.repeat(32), '文'.repeat(32), '热词', '尾'.repeat(16), '词']
    expect(selectRecognitionHotwords(words)).toEqual([
      '中'.repeat(32),
      '文'.repeat(32),
      '热词',
      '词',
    ])
  })
  it('refreshes changes and deletion with every new snapshot', () => {
    expect(selectRecognitionHotwords(['旧词', '另一个词'])).toEqual(['旧词', '另一个词'])
    expect(selectRecognitionHotwords(['新词'])).toEqual(['新词'])
    expect(selectRecognitionHotwords([])).toEqual([])
  })
})
