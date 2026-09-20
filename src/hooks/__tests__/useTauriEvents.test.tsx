import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '../../stores/appStore'
import { useTauriEvents } from '../useTauriEvents'

const events = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  unlisteners: [] as ReturnType<typeof vi.fn>[],
  getHistory: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, callback: (event: { payload: unknown }) => void) => {
    events.handlers.set(event, callback)
    const unlisten = vi.fn()
    events.unlisteners.push(unlisten)
    return Promise.resolve(unlisten)
  }),
}))
vi.mock('../../lib/tauri', () => ({
  getHistory: events.getHistory,
  updateConfig: vi.fn(),
  setAutoStart: vi.fn(),
}))
vi.mock('../../components/Toast', () => ({ toast: vi.fn() }))

function emit(event: string, payload: unknown) {
  act(() => events.handlers.get(event)?.({ payload }))
}

beforeEach(() => {
  useAppStore.setState(useAppStore.getInitialState())
  events.handlers.clear()
  events.unlisteners.length = 0
  events.getHistory.mockReset().mockResolvedValue([])
})
afterEach(cleanup)

describe('native event synchronization for POP', () => {
  it('starts a versioned session, then final text replaces both preview lanes', () => {
    useAppStore.setState({
      previewTranscript: '上一段本地预览',
      partialTranscript: '上一段预览',
      finalTranscript: '上一段结果',
      resolvedTranscript: '上一段权威结果',
      recordingDuration: 18,
    })
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 41, revision: 1, state: 'recording' })
    expect(useAppStore.getState()).toMatchObject({
      pipelineState: 'recording',
      activeSessionId: 41,
      previewTranscript: '',
      partialTranscript: '',
      finalTranscript: '',
      resolvedTranscript: '',
      recordingDuration: 0,
    })
    emit('stt:preview', { session_id: 41, revision: 0, text: '这是本地近似预览' })
    expect(useAppStore.getState().previewTranscript).toBe('这是本地近似预览')
    emit('stt:partial', { session_id: 41, revision: 0, text: '这是服务商正在录入的文本' })
    expect(useAppStore.getState().partialTranscript).toBe('这是服务商正在录入的文本')
    emit('stt:final', { session_id: 41, revision: 1, text: '这是最终文本。' })
    expect(useAppStore.getState()).toMatchObject({
      previewTranscript: '',
      partialTranscript: '',
      finalTranscript: '这是最终文本。',
      authoritativeFinalSeen: true,
    })
    emit('stt:preview', { session_id: 41, revision: 99, text: '不能覆盖最终结果' })
    expect(useAppStore.getState().previewTranscript).toBe('')

    // A provider "final" confirms the accumulated prefix, not necessarily the
    // whole recording. A newer partial for the next segment must still render.
    emit('stt:partial', { session_id: 41, revision: 2, text: '下一分段正在说' })
    expect(useAppStore.getState().partialTranscript).toBe('下一分段正在说')
  })

  it('keeps output errors visible after returning idle and refreshes local history', async () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 8, revision: 1, state: 'recording' })
    emit('pipeline:error', { session_id: 8, revision: 1, message: '模型无法加载' })
    emit('pipeline:state', { session_id: 8, revision: 2, state: 'idle' })
    expect(useAppStore.getState().pipelineError).toBe('模型无法加载')
    await waitFor(() => expect(events.getHistory).toHaveBeenCalledWith(200, 0))
  })

  it('rejects stale session messages while keeping global notices compatible', () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 51, revision: 1, state: 'recording' })
    emit('pipeline:error', { session_id: 50, revision: 99, message: '旧会话错误' })
    expect(useAppStore.getState().pipelineError).toBeNull()

    emit('pipeline:error', { session_id: 51, revision: 2, message: '当前会话错误' })
    emit('pipeline:error', { session_id: 51, revision: 1, message: '乱序错误' })
    expect(useAppStore.getState().pipelineError).toBe('当前会话错误')

    emit('pipeline:error', '全局快捷键错误')
    expect(useAppStore.getState().pipelineError).toBe('全局快捷键错误')
  })

  it('ignores other sessions and non-increasing revisions on both preview lanes', () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 12, revision: 1, state: 'recording' })
    emit('stt:preview', { session_id: 12, revision: 2, text: '本地预览二' })
    emit('stt:preview', { session_id: 11, revision: 50, text: '旧会话' })
    emit('stt:preview', { session_id: 12, revision: 1, text: '旧版本' })
    emit('stt:partial', { session_id: 12, revision: 4, text: '主路版本四' })
    emit('stt:final', { session_id: 12, revision: 6, text: '主路已确认' })
    emit('stt:partial', { session_id: 12, revision: 5, text: '跨类型乱序包' })
    emit('stt:partial', { session_id: 12, revision: 4, text: '重复版本' })
    emit('stt:partial', { session_id: 99, revision: 5, text: '其他会话' })
    emit('llm:chunk', { session_id: 99, revision: 1, text: '其他会话润色' })
    emit('pipeline:resolved', { session_id: 99, revision: 1, text: '其他会话结果' })

    expect(useAppStore.getState()).toMatchObject({
      previewTranscript: '',
      lastPreviewRevision: 2,
      partialTranscript: '',
      lastPartialRevision: 4,
      finalTranscript: '主路已确认',
      lastFinalRevision: 6,
      polishedText: '',
      resolvedTranscript: '',
    })

    emit('pipeline:state', { session_id: 11, revision: 2, state: 'idle' })
    expect(useAppStore.getState().pipelineState).toBe('recording')

    emit('pipeline:state', { session_id: 13, revision: 3, state: 'recording' })
    emit('pipeline:state', { session_id: 12, revision: 4, state: 'recording' })
    expect(useAppStore.getState()).toMatchObject({
      activeSessionId: 13,
      pipelineState: 'recording',
      previewTranscript: '',
      partialTranscript: '',
    })
  })

  it('keeps the session high-water mark when recording text is reset', () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 15, revision: 1, state: 'recording' })
    emit('pipeline:state', { session_id: 15, revision: 2, state: 'idle' })

    act(() => useAppStore.getState().resetRecording())
    emit('pipeline:state', { session_id: 15, revision: 3, state: 'recording' })

    expect(useAppStore.getState()).toMatchObject({
      activeSessionId: 15,
      lastPipelineStateRevision: 2,
      pipelineState: 'idle',
    })
  })

  it('accepts local preview only while recording', () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 21, revision: 1, state: 'recording' })
    emit('pipeline:state', { session_id: 21, revision: 3, state: 'transcribing' })
    emit('pipeline:state', { session_id: 21, revision: 2, state: 'idle' })
    emit('stt:preview', { session_id: 21, revision: 0, text: '太晚到达的预览' })
    expect(useAppStore.getState().previewTranscript).toBe('')
    expect(useAppStore.getState().pipelineState).toBe('transcribing')
  })

  it('orders LLM chunks by revision and makes resolved text authoritative', () => {
    renderHook(() => useTauriEvents())
    emit('pipeline:state', { session_id: 31, revision: 1, state: 'recording' })
    emit('stt:final', { session_id: 31, revision: 0, text: '原始结果' })
    emit('pipeline:state', { session_id: 31, revision: 2, state: 'polishing' })
    emit('llm:chunk', { session_id: 31, revision: 0, text: '润色' })
    emit('llm:chunk', { session_id: 31, revision: 0, text: '重复' })
    emit('llm:chunk', { session_id: 31, revision: 1, text: '完成' })
    expect(useAppStore.getState().polishedText).toBe('润色完成')

    emit('pipeline:resolved', { session_id: 31, revision: 5, text: '权威输出' })
    emit('llm:chunk', { session_id: 31, revision: 9, text: '迟到分片' })
    emit('stt:final', { session_id: 31, revision: 9, text: '迟到原文' })
    expect(useAppStore.getState()).toMatchObject({
      resolvedTranscript: '权威输出',
      polishedText: '润色完成',
      finalTranscript: '原始结果',
      lastResolvedRevision: 5,
    })
  })

  it('applies the native close event without overwriting an unrelated unsaved draft', () => {
    const saved = { ...useAppStore.getState().config }
    useAppStore.setState({ savedConfig: saved, config: { ...saved, hotkey: 'Alt+/' } })
    renderHook(() => useTauriEvents())
    emit('config:updated', { ...saved, capsule_enabled: false })
    expect(useAppStore.getState().config).toMatchObject({ capsule_enabled: false, hotkey: 'Alt+/' })
    expect(useAppStore.getState().savedConfig).toMatchObject({
      capsule_enabled: false,
      hotkey: saved.hotkey,
    })
  })

  it('unsubscribes even when native listener promises resolve after unmount', async () => {
    const hook = renderHook(() => useTauriEvents())
    hook.unmount()
    await waitFor(() =>
      expect(events.unlisteners.every((unlisten) => unlisten.mock.calls.length === 1)).toBe(true),
    )
    expect(events.unlisteners.length).toBeGreaterThan(10)
  })
})
