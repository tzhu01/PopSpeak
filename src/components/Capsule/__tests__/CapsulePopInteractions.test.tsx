import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { HTMLAttributes, ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { useAppStore } from '../../../stores/appStore'
import { Capsule } from '..'
import { CapsuleContextMenu } from '../CapsuleContextMenu'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../../../hooks/useCapsuleResize', () => ({ useCapsuleResize: vi.fn() }))
vi.mock('../Waveform', () => ({ Waveform: () => <div data-testid="waveform" /> }))
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))
vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => children,
  useReducedMotion: () => true,
  motion: {
    span: ({ children, className }: HTMLAttributes<HTMLSpanElement>) => (
      <span className={className}>{children}</span>
    ),
    div: ({
      children,
      className,
      style,
      onPointerDown,
      onPointerMove,
      onPointerUp,
    }: HTMLAttributes<HTMLDivElement>) => (
      <div
        className={className}
        style={style}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        {children}
      </div>
    ),
  },
}))

function pointerClick(button: HTMLElement) {
  fireEvent(button, new MouseEvent('pointerdown', { bubbles: true, button: 0 }))
  fireEvent(button, new MouseEvent('pointerup', { bubbles: true, button: 0 }))
  fireEvent.click(button)
}

beforeEach(() => {
  useAppStore.setState(useAppStore.getInitialState())
  useAppStore.getState().setSavedConfig({ ...useAppStore.getState().config })
  vi.mocked(invoke).mockReset().mockResolvedValue(undefined)
  vi.mocked(writeText).mockReset().mockResolvedValue(undefined)
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe('POP recording controls', () => {
  it('bounds POP content and scrolls the menu on smaller work areas', () => {
    useAppStore.setState({ pipelineState: 'recording' })
    const { container } = render(<Capsule />)
    expect(container.querySelector('section')).toHaveStyle({
      maxWidth: 'calc(100vw - 24px)',
      maxHeight: 'calc(100vh - 24px)',
    })
    act(() => useAppStore.setState({ contextMenuOpen: true, contextMenuReady: true }))
    expect(container.querySelector('.signal-menu')?.parentElement).toHaveStyle({
      maxHeight: 'calc(100vh - 72px)',
      overflowY: 'auto',
    })
  })

  it('copies only the visible transcript without stopping recording', async () => {
    useAppStore.setState({
      pipelineState: 'recording',
      previewTranscript: '保留我的空格  和文字。',
    })
    render(<Capsule />)
    pointerClick(screen.getByRole('button', { name: '复制当前文字' }))
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('已复制当前文字'))
    expect(writeText).toHaveBeenCalledExactlyOnceWith('保留我的空格  和文字。')
    expect(invoke).not.toHaveBeenCalled()
    expect(useAppStore.getState().pipelineState).toBe('recording')
  })

  it('disables copy for a blank preview and reports clipboard failures', async () => {
    useAppStore.setState({ pipelineState: 'recording', previewTranscript: '  ' })
    render(<Capsule />)
    expect(screen.getByRole('button', { name: '复制当前文字' })).toBeDisabled()
    act(() => useAppStore.getState().setPreviewTranscript('这段应允许重试'))
    vi.mocked(writeText).mockRejectedValueOnce(new Error('clipboard locked'))
    pointerClick(screen.getByRole('button', { name: '复制当前文字' }))
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('复制失败，请重试'))
    expect(invoke).not.toHaveBeenCalled()
  })

  it.each([
    { pipelineState: 'idle' as const, error: null },
    { pipelineState: 'recording' as const, error: null },
    { pipelineState: 'idle' as const, error: '模型加载失败' },
  ])(
    'pins the compact $pipelineState/$error surface with inline absolute positioning',
    ({ pipelineState, error }) => {
      useAppStore.setState((state) => ({
        pipelineState,
        pipelineError: error,
        config: { ...state.config, capsule_preview_enabled: false },
      }))
      const { container } = render(<Capsule />)
      // Legacy, unlayered jelly styles declare position:relative and override
      // Tailwind's layered absolute utility. Inline positioning is intentional.
      const surface = container.querySelector('.signal-capsule')
      expect(surface).toHaveStyle({ position: 'absolute', right: '12px', bottom: '12px' })
    },
  )

  it('collapses and expands without stopping, aborting or resetting the clock', () => {
    vi.useFakeTimers()
    useAppStore.setState({ pipelineState: 'recording' })
    render(<Capsule />)
    act(() => vi.advanceTimersByTime(7250))
    expect(screen.getByText('00:07')).toBeInTheDocument()

    pointerClick(screen.getByRole('button', { name: '收起预览' }))
    expect(screen.queryByRole('region', { name: 'POP 录音预览' })).not.toBeInTheDocument()
    expect(screen.getByText('00:07')).toBeInTheDocument()
    pointerClick(screen.getByRole('button', { name: '展开预览' }))
    expect(screen.getByRole('region', { name: 'POP 录音预览' })).toBeInTheDocument()
    act(() => vi.advanceTimersByTime(1000))
    expect(screen.getByText('00:08')).toBeInTheDocument()
    expect(useAppStore.getState().pipelineState).toBe('recording')
    expect(invoke).not.toHaveBeenCalled()
  })

  it.each([true, false])('cancel only aborts with preview enabled=%s', (capsulePreview) => {
    useAppStore.setState((state) => ({
      pipelineState: 'recording',
      config: { ...state.config, capsule_preview_enabled: capsulePreview },
    }))
    render(<Capsule />)
    pointerClick(screen.getByRole('button', { name: '取消录音' }))
    expect(invoke).toHaveBeenCalledExactlyOnceWith('abort_recording')
    expect(invoke).not.toHaveBeenCalledWith('stop_recording')
  })

  it.each(['transcribing', 'polishing', 'outputting'] as const)(
    'can reopen the POP while %s without submitting or cancelling',
    (pipelineState) => {
      useAppStore.setState({ pipelineState, finalTranscript: '已经识别的文字' })
      render(<Capsule />)
      pointerClick(screen.getByRole('button', { name: '收起预览' }))
      pointerClick(screen.getByRole('button', { name: '展开预览' }))
      expect(screen.getByRole('region', { name: 'POP 录音预览' })).toBeInTheDocument()
      expect(useAppStore.getState().pipelineState).toBe(pipelineState)
      expect(invoke).not.toHaveBeenCalled()
    },
  )

  it('the explicit end button submits exactly once', () => {
    useAppStore.setState({ pipelineState: 'recording' })
    render(<Capsule />)
    pointerClick(screen.getByRole('button', { name: '结束录音' }))
    expect(invoke).toHaveBeenCalledExactlyOnceWith('stop_recording')
  })

  it('displays local preview text and distinguishes it from final output', () => {
    useAppStore.setState({ pipelineState: 'recording', previewTranscript: '你好，这是预览。' })
    render(<Capsule />)
    expect(screen.getByText('你好，这是预览。')).toBeInTheDocument()
    expect(screen.getByText('本地近似预览可能变化，以结束录音后的完整结果为准')).toBeInTheDocument()
  })

  it('announces the same local approximate preview for every selected model', () => {
    useAppStore.setState((state) => ({
      pipelineState: 'recording',
      config: { ...state.config, stt_provider: 'volcengine-seedasr' },
    }))
    render(<Capsule />)
    expect(screen.getByText('请说话，本地近似预览会实时刷新…')).toBeInTheDocument()
  })

  it('shows and copies the authoritative visible transcript by strict priority', async () => {
    useAppStore.setState({
      pipelineState: 'recording',
      previewTranscript: '本地预览',
    })
    render(<Capsule />)
    expect(screen.getByText('本地预览')).toBeInTheDocument()

    act(() => useAppStore.setState({ partialTranscript: '主路片段' }))
    expect(screen.getByText('主路片段')).toBeInTheDocument()
    expect(screen.queryByText('本地预览')).not.toBeInTheDocument()

    act(() => useAppStore.setState({ finalTranscript: '主路最终' }))
    expect(screen.getByText('主路最终主路片段')).toBeInTheDocument()

    act(() => useAppStore.setState({ polishedText: '润色流' }))
    expect(screen.getByText('润色流')).toBeInTheDocument()

    act(() => useAppStore.setState({ resolvedTranscript: '权威最终' }))
    expect(screen.getByText('权威最终')).toBeInTheDocument()
    expect(screen.queryByText('润色流')).not.toBeInTheDocument()
    pointerClick(screen.getByRole('button', { name: '复制当前文字' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith('权威最终'))
  })
})

describe('floating recorder context menu', () => {
  it('Close Floating Ball saves capsule_enabled=false without quitting or cancelling', async () => {
    const original = { ...useAppStore.getState().config }
    useAppStore.setState({ pipelineState: 'recording' })
    vi.mocked(invoke).mockResolvedValue({ ...original, capsule_enabled: false })
    const close = vi.fn()
    render(<CapsuleContextMenu onClose={close} />)
    fireEvent.click(screen.getByRole('menuitem', { name: '关闭悬浮球' }))
    await waitFor(() => expect(close).toHaveBeenCalledOnce())
    expect(invoke).toHaveBeenCalledExactlyOnceWith('set_capsule_enabled', { enabled: false })
    expect(useAppStore.getState().savedConfig?.capsule_enabled).toBe(false)
    expect(useAppStore.getState().config.capsule_enabled).toBe(false)
    expect(useAppStore.getState().pipelineState).toBe('recording')
    expect(invoke).not.toHaveBeenCalledWith('quit_app')
    expect(invoke).not.toHaveBeenCalledWith('abort_recording')
  })

  it('a failed close remains retryable and does not falsely disable the setting', async () => {
    const original = { ...useAppStore.getState().config }
    vi.mocked(invoke).mockRejectedValueOnce(new Error('write failed'))
    const close = vi.fn()
    render(<CapsuleContextMenu onClose={close} />)
    fireEvent.click(screen.getByRole('menuitem', { name: '关闭悬浮球' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('未能保存，请重试')
    expect(close).not.toHaveBeenCalled()
    expect(useAppStore.getState().config.capsule_enabled).toBe(true)
    vi.mocked(invoke).mockResolvedValueOnce({ ...original, capsule_enabled: false })
    fireEvent.click(screen.getByRole('menuitem', { name: '关闭悬浮球' }))
    await waitFor(() => expect(close).toHaveBeenCalledOnce())
    expect(invoke).toHaveBeenCalledTimes(2)
  })
})
