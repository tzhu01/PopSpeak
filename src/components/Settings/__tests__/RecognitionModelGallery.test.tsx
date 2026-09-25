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
    expect(screen.getAllByRole('button')).toHaveLength(8)
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
    expect(
      screen.queryByRole('button', { name: /Qwen3-ASR|Cohere Transcribe|Parakeet|Nemotron/ }),
    ).not.toBeInTheDocument()
  })

  it('lets users choose the on-demand multilingual variant', () => {
    const onChoose = vi.fn()
    render(<RecognitionModelGallery selected="sensevoice" onChoose={onChoose} />)
    fireEvent.click(screen.getByRole('button', { name: /Whisper Base/ }))
    expect(onChoose).toHaveBeenCalledWith('whisper-base')
  })

  it('disables models whose recognition runtime is absent without hiding them', () => {
    const onChoose = vi.fn()
    render(
      <RecognitionModelGallery
        selected="sensevoice"
        onChoose={onChoose}
        availability={{ 'whisper-tiny': true }}
        runtimeIssues={{
          'whisper-tiny': '本安装包未提供 Whisper 本地识别组件；仅下载模型无法使用。',
        }}
      />,
    )
    const whisper = screen.getByRole('button', { name: /Whisper Tiny/ })
    expect(whisper).toBeDisabled()
    expect(whisper).toHaveTextContent('识别组件缺失')
    expect(whisper).not.toHaveTextContent('已安装')
    fireEvent.click(whisper)
    expect(onChoose).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: /SenseVoice Small/ })).not.toBeDisabled()
  })
})
