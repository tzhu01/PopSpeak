import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { RecognitionModelGallery } from '../RecognitionModelGallery'

afterEach(cleanup)

describe('recognition model gallery', () => {
  it('shows only real local configurations and labels cloud choices as user-paid', () => {
    render(
      <RecognitionModelGallery
        selected="sensevoice"
        onChoose={vi.fn()}
        availability={{ sensevoice: true, 'whisper-small': false }}
      />,
    )
    expect(screen.getAllByRole('button')).toHaveLength(12)
    expect(screen.getByRole('button', { name: /SenseVoice Small/ })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(screen.getByRole('button', { name: /SenseVoice Small/ })).toHaveTextContent('已安装')
    expect(screen.getByRole('button', { name: /豆包 SeedASR/ })).toHaveTextContent('自费 API')
    expect(screen.getByText(/软件功能按当前激活状态开放/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Whisper Small/ })).toHaveTextContent('约 190 MB')
    expect(screen.getByRole('button', { name: /Whisper Small/ })).toHaveTextContent('待下载')
    expect(screen.getByRole('button', { name: /Whisper Large v3 Turbo/ })).toHaveTextContent(
      '约 574 MB',
    )
    expect(screen.getByRole('button', { name: /Qwen3-ASR 1.7B/ })).toHaveTextContent('按需下载')
    expect(screen.getByRole('button', { name: /Cohere Transcribe/ })).toHaveTextContent('14 种语言')
    expect(screen.getByRole('button', { name: /Parakeet Unified/ })).toHaveTextContent('英语专用')
    expect(screen.getByRole('button', { name: /Nemotron 3.5/ })).toHaveTextContent('整段离线识别')
  })

  it('lets users choose the on-demand multilingual variant', () => {
    const onChoose = vi.fn()
    render(<RecognitionModelGallery selected="sensevoice" onChoose={onChoose} />)
    fireEvent.click(screen.getByRole('button', { name: /Whisper Base/ }))
    expect(onChoose).toHaveBeenCalledWith('whisper-base')
  })

  it.each([
    ['Qwen3-ASR 1.7B', 'qwen3-asr-1.7b'],
    ['Cohere Transcribe 03-2026', 'cohere-transcribe-03-2026'],
    ['Nemotron 3.5 ASR Streaming 0.6B', 'nemotron-3.5-asr-streaming-0.6b'],
    ['Parakeet Unified EN 0.6B', 'parakeet-unified-en-0.6b'],
  ])('selects the native backend for %s', (name, id) => {
    const onChoose = vi.fn()
    render(<RecognitionModelGallery selected="sensevoice" onChoose={onChoose} />)
    fireEvent.click(screen.getByRole('button', { name: new RegExp(name) }))
    expect(onChoose).toHaveBeenCalledExactlyOnceWith(id)
  })
})
