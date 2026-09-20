import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { HomePage } from '../index'
import { useAppStore } from '../../../stores/appStore'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: { hotkey?: string }) =>
      values?.hotkey ? `${key} ${values.hotkey}` : key,
  }),
}))
vi.mock('../../../stores/authStore', () => ({ useAuthStore: () => ({ user: null, plan: 'free' }) }))
vi.mock('../../../lib/router', () => ({ useRoute: () => ({ navigate: vi.fn() }) }))
const originalConfig = { ...useAppStore.getState().config }

beforeEach(() =>
  useAppStore.setState({ config: { ...originalConfig }, savedConfig: null, history: [] }),
)
afterEach(cleanup)

describe('Home applied configuration', () => {
  it('does not claim an unsubmitted precision mode is active', () => {
    useAppStore.setState({
      savedConfig: { ...originalConfig, stt_provider: 'sensevoice' },
      config: { ...originalConfig, stt_provider: 'funasr-nano' },
    })
    render(<HomePage />)
    expect(screen.getByText('SenseVoice Small INT8')).toBeInTheDocument()
    expect(screen.queryByText('Fun-ASR-Nano GGUF')).not.toBeInTheDocument()
  })
  it('shows the saved precision mode after it has been applied', () => {
    useAppStore.setState({ savedConfig: { ...originalConfig, stt_provider: 'funasr-nano' } })
    render(<HomePage />)
    expect(screen.getByText('Fun-ASR-Nano GGUF')).toBeInTheDocument()
  })
  it('describes the actual saved toggle shortcut, not hold-to-talk', () => {
    useAppStore.setState({
      savedConfig: { ...originalConfig, hotkey_mode: 'toggle', hotkey: 'Ctrl+,' },
    })
    render(<HomePage />)
    expect(screen.getByText('home.descriptionToggle Ctrl+,')).toBeInTheDocument()
  })
})
