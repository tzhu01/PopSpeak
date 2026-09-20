import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { EditorApp } from '../index'
import { formatEditorText, loadEditorPreferences } from '../preferences'
import * as tauri from '../../../lib/tauri'

type EditorEvent = {
  payload: {
    id: number
    text: string
    app_name: string
    language?: string
    auto_hide_enabled?: boolean
    auto_hide_seconds?: number
  }
}

let showEditor: ((event: EditorEvent) => void) | undefined
const setAlwaysOnTop = vi.fn().mockResolvedValue(undefined)
const minimize = vi.fn().mockResolvedValue(undefined)
const writeText = vi.fn().mockResolvedValue(undefined)

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((_event: string, handler: (event: EditorEvent) => void) => {
    showEditor = handler
    return Promise.resolve(vi.fn())
  }),
}))

vi.mock('../../../lib/tauri', () => ({
  addDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  hideEditorWindow: vi.fn().mockResolvedValue(undefined),
  updateHistoryEntry: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setAlwaysOnTop, minimize }),
}))

describe('EditorApp auto close', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    localStorage.clear()
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    })
    showEditor = undefined
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('uses the configured idle duration', async () => {
    render(<EditorApp />)
    expect(showEditor).toBeDefined()

    act(() => {
      showEditor?.({
        payload: {
          id: 1,
          text: '测试文本',
          app_name: 'PopSpeak',
          auto_hide_enabled: true,
          auto_hide_seconds: 8,
        },
      })
    })
    expect(screen.getByText(/8s 后关闭/)).toBeDefined()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(8100)
    })
    expect(tauri.hideEditorWindow).toHaveBeenCalledTimes(1)
  })

  it('stays visible when automatic closing is disabled', async () => {
    render(<EditorApp />)
    act(() => {
      showEditor?.({
        payload: {
          id: 2,
          text: '保持显示',
          app_name: 'PopSpeak',
          auto_hide_enabled: false,
          auto_hide_seconds: 3,
        },
      })
    })
    expect(screen.queryByText(/后关闭/)).toBeNull()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000)
    })
    expect(tauri.hideEditorWindow).not.toHaveBeenCalled()
  })

  it('copies the final text automatically when the persisted option is enabled', async () => {
    localStorage.setItem(
      'popspeak.editor.preferences.v1',
      JSON.stringify({ autoCopy: true, alwaysOnTop: false }),
    )
    render(<EditorApp />)

    await act(async () => {
      showEditor?.({
        payload: {
          id: 3,
          text: '自动复制内容',
          app_name: 'PopSpeak',
          language: 'zh',
          auto_hide_enabled: false,
        },
      })
      await Promise.resolve()
    })

    expect(writeText).toHaveBeenCalledWith('自动复制内容')
    expect(setAlwaysOnTop).toHaveBeenLastCalledWith(false)
    expect(screen.getByText(/识别语言：/).textContent).toContain('简体中文')
  })

  it('persists layout and topmost choices while keeping editing available', async () => {
    render(<EditorApp />)
    act(() => {
      showEditor?.({
        payload: {
          id: 4,
          text: '第一句。第二句！',
          app_name: 'PopSpeak',
          auto_hide_enabled: false,
        },
      })
    })

    fireEvent.click(screen.getByRole('button', { name: /排版/ }))
    fireEvent.click(screen.getByRole('button', { name: '按句分行' }))
    expect((screen.getByLabelText('识别结果') as HTMLTextAreaElement).value).toBe(
      '第一句。\n第二句！',
    )

    fireEvent.click(screen.getByRole('button', { name: '取消置顶' }))
    await act(async () => Promise.resolve())
    expect(setAlwaysOnTop).toHaveBeenLastCalledWith(false)
    expect(loadEditorPreferences().layoutMode).toBe('multi-line')
    expect(loadEditorPreferences().alwaysOnTop).toBe(false)
  })
})

describe('editor text formatting', () => {
  const base = {
    autoCopy: false,
    copyOnClose: false,
    highlightOnCopy: false,
    alwaysOnTop: true,
    layoutMode: 'auto' as const,
    removeSpaces: false,
    removePunctuation: false,
  }

  it('supports single-line, spacing and punctuation preferences', () => {
    expect(
      formatEditorText('  你好，\n 世界！ ', {
        ...base,
        layoutMode: 'single-line',
        removeSpaces: true,
        removePunctuation: true,
      }),
    ).toBe('你好世界')
  })
})
