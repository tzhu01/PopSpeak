import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { History } from '../index'
import { useAppStore, type HistoryEntry } from '../../../stores/appStore'
import { deleteHistoryEntry } from '../../../lib/tauri'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))
vi.mock('../../../lib/tauri', () => ({
  clearHistory: vi.fn(),
  deleteHistoryEntry: vi.fn(),
  updateHistoryEntry: vi.fn(),
}))
vi.mock('../../Toast', () => ({ toast: { success: vi.fn(), error: vi.fn() } }))

const entry = (id: number, text: string): HistoryEntry => ({
  id,
  created_at: '2026-09-05T12:00:00',
  app_name: 'Test',
  app_type: '',
  raw_text: text,
  polished_text: text,
  language: 'zh',
  duration_ms: 1000,
})

describe('History single entry deletion', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ history: [entry(1, '第一条保留'), entry(2, '第二条删除')] })
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    vi.mocked(deleteHistoryEntry).mockResolvedValue(undefined)
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('deletes only the confirmed row after persistence succeeds', async () => {
    render(<History />)
    fireEvent.click(screen.getByRole('button', { name: 'history.delete: 第二条删除' }))
    await waitFor(() => expect(screen.queryByText('第二条删除')).not.toBeInTheDocument())
    expect(deleteHistoryEntry).toHaveBeenCalledExactlyOnceWith(2)
    expect(screen.getByText('第一条保留')).toBeInTheDocument()
    expect(useAppStore.getState().history.map((h) => h.id)).toEqual([1])
  })

  it('does not mutate records when confirmation is cancelled', () => {
    vi.mocked(window.confirm).mockReturnValue(false)
    render(<History />)
    fireEvent.click(screen.getByRole('button', { name: 'history.delete: 第二条删除' }))
    expect(deleteHistoryEntry).not.toHaveBeenCalled()
    expect(screen.getByText('第二条删除')).toBeInTheDocument()
  })

  it('retains the row on database failure', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    vi.mocked(deleteHistoryEntry).mockRejectedValue(new Error('database busy'))
    render(<History />)
    fireEvent.click(screen.getByRole('button', { name: 'history.delete: 第二条删除' }))
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'history.delete: 第二条删除' })).toBeEnabled(),
    )
    expect(useAppStore.getState().history).toHaveLength(2)
  })

  it('does not lose records appended while deletion is in flight', async () => {
    let resolveDeletion: () => void = () => {}
    vi.mocked(deleteHistoryEntry).mockReturnValue(
      new Promise<void>((resolve) => {
        resolveDeletion = resolve
      }),
    )
    render(<History />)
    fireEvent.click(screen.getByRole('button', { name: 'history.delete: 第二条删除' }))
    useAppStore.setState((state) => ({ history: [entry(3, '新录音'), ...state.history] }))
    resolveDeletion()
    await waitFor(() => expect(useAppStore.getState().history.map((h) => h.id)).toEqual([3, 1]))
  })

  it('keeps the full-height editor instead of a fixed-height inner scroll box', () => {
    const text = '这是很长的识别记录，需要完整显示。'.repeat(60)
    useAppStore.setState({ history: [entry(3, text)] })
    render(<History />)
    fireEvent.click(screen.getByRole('button', { name: `Edit: ${text.slice(0, 30)}` }))
    const textarea = screen.getAllByRole('textbox').find((field) => field.tagName === 'TEXTAREA')!
    expect(textarea).toHaveClass('h-full', 'overflow-hidden')
    expect(textarea.parentElement?.querySelector('[aria-hidden="true"]')).toHaveTextContent(text)
  })
})
