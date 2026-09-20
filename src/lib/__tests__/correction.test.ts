import { describe, expect, it } from 'vitest'
import { changedSpan } from '../correction'

describe('changedSpan', () => {
  it('learns a Chinese term instead of a single character', () => {
    expect(changedSpan('这是专业词汇', '这是展业词汇')).toEqual({
      from: '专业',
      to: '展业',
    })
  })

  it('expands a corrected last character to the full Chinese term', () => {
    expect(changedSpan('去除口屁、重复字等。', '去除口癖、重复字等。')).toEqual({
      from: '口屁',
      to: '口癖',
    })
  })

  it('uses the corrected word boundary when the ASR typo is not a known word', () => {
    expect(changedSpan('这个口辟问题', '这个口癖问题')).toEqual({
      from: '口辟',
      to: '口癖',
    })
  })

  it('extracts the edited Latin token', () => {
    expect(changedSpan('use open ai now', 'use OpenAI now')).toEqual({
      from: 'open ai',
      to: 'OpenAI',
    })
  })
})
