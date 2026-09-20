import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { DictionaryPane } from '../DictionaryPane'
import { useAppStore } from '../../../stores/appStore'
import type { AppConfig } from '../../../stores/appStore'
import { useActivationStore } from '../../../lib/activation'
import {
  addDictionaryEntry,
  getDictionary,
  removeDictionaryEntry,
  updateDictionaryEntry,
  updateConfig,
} from '../../../lib/tauri'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(vi.fn()) }))
vi.mock('../../../lib/tauri', () => ({
  getDictionary: vi.fn(),
  addDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  updateDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  removeDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  updateConfig: vi.fn(async (config: AppConfig) => ({ ...config })),
  setAutoStart: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../../Toast', () => ({ toast: { success: vi.fn(), error: vi.fn() } }))

describe('Recognition hotword vocabulary', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ ...useAppStore.getInitialState() })
    useActivationStore.setState({
      error: null,
      status: {
        installation_id: 'test-device',
        configured: true,
        activated: true,
        license_id: 'test-license',
        trial_recordings_used: 0,
        trial_recordings_limit: 200,
        trial_recordings_remaining: 200,
        trial_milliseconds_used: 0,
        trial_milliseconds_limit: 1200000,
        trial_milliseconds_remaining: 1200000,
        trial_exhausted: false,
        recording_in_progress: false,
      },
    })
    vi.mocked(getDictionary).mockResolvedValue([])
  })
  afterEach(cleanup)

  it('labels the default engine as post-processing instead of decoder hotwords', async () => {
    const navigate = vi.fn()
    render(<DictionaryPane onOpenRecognition={navigate} />)
    expect(screen.getByText('dictionary.postprocessHotwordsSupported')).toBeInTheDocument()
    expect(screen.getByText('dictionary.postprocessHotwordsHint')).toBeInTheDocument()
    expect(navigate).not.toHaveBeenCalled()
    expect(useAppStore.getState().config.stt_provider).toBe('sensevoice')
    await waitFor(() => expect(getDictionary).toHaveBeenCalled())
  })

  it.each(['funasr-nano', 'local-whisper'] as const)(
    'shows native support for saved %s configuration',
    async (provider) => {
      useAppStore.setState((state) => ({
        config: { ...state.config, stt_provider: provider },
        savedConfig: { ...state.config, stt_provider: provider },
      }))
      render(<DictionaryPane />)
      expect(screen.getByText('dictionary.decoderHotwordsSupported')).toBeInTheDocument()
      expect(screen.getByText('dictionary.decoderHotwordsHint')).toBeInTheDocument()
      await waitFor(() => expect(getDictionary).toHaveBeenCalled())
    },
  )

  it('explains a selected but unapplied engine and only confirms support after native save succeeds', async () => {
    const initial = useAppStore.getState().config
    useAppStore.setState({
      savedConfig: initial,
      config: { ...initial, stt_provider: 'funasr-nano' },
    })
    let finish: () => void = () => {}
    vi.mocked(updateConfig).mockReturnValueOnce(
      new Promise<AppConfig>((resolve) => {
        finish = () => resolve({ ...initial, stt_provider: 'funasr-nano' })
      }),
    )
    render(<DictionaryPane />)

    expect(screen.getByText('识别模式待应用')).toBeInTheDocument()
    expect(screen.getByText(/当前生效：SenseVoice Small INT8/)).toBeInTheDocument()
    expect(screen.getByText(/你已选择“Fun-ASR-Nano GGUF/)).toBeInTheDocument()
    expect(screen.queryByText('dictionary.decoderHotwordsSupported')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '保存当前设置并应用' }))
    expect(updateConfig).toHaveBeenCalledWith(
      expect.objectContaining({ stt_provider: 'funasr-nano' }),
    )
    expect(useAppStore.getState().savedConfig?.stt_provider).toBe('sensevoice')

    await act(async () => {
      finish()
    })
    expect(screen.getByText('dictionary.decoderHotwordsSupported')).toBeInTheDocument()
    expect(screen.getByText(/当前生效：Fun-ASR-Nano GGUF/)).toBeInTheDocument()
    expect(screen.queryByText('识别模式待应用')).not.toBeInTheDocument()
    expect(screen.getByText(/识别组件仍需安装并成功加载/)).toBeInTheDocument()
  })

  it('keeps the actual mode unchanged when applying settings fails, and can discard the draft', async () => {
    const initial = useAppStore.getState().config
    useAppStore.setState({
      savedConfig: initial,
      config: { ...initial, stt_provider: 'funasr-nano' },
    })
    vi.mocked(updateConfig).mockRejectedValueOnce(new Error('write failed'))
    render(<DictionaryPane />)
    fireEvent.click(screen.getByRole('button', { name: '保存当前设置并应用' }))

    expect(await screen.findByText(/设置未保存，仍使用原来的识别模式/)).toBeInTheDocument()
    expect(screen.queryByText('dictionary.decoderHotwordsSupported')).not.toBeInTheDocument()
    expect(useAppStore.getState().savedConfig?.stt_provider).toBe('sensevoice')
    fireEvent.click(screen.getByRole('button', { name: '撤销待应用设置' }))
    expect(screen.getByText('dictionary.postprocessHotwordsSupported')).toBeInTheDocument()
    expect(useAppStore.getState().config.stt_provider).toBe('sensevoice')
    expect(screen.queryByText('识别模式待应用')).not.toBeInTheDocument()
  })

  it('allows adding a hotword without a wrong-form replacement rule', async () => {
    render(<DictionaryPane />)
    fireEvent.change(screen.getByPlaceholderText('dictionary.correctWordExample'), {
      target: { value: 'PopSpeak' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'dictionary.add' }))
    await waitFor(() => expect(addDictionaryEntry).toHaveBeenCalledWith('PopSpeak', null, null))
  })

  it('persists edits and removal through dictionary commands', async () => {
    vi.mocked(getDictionary).mockResolvedValue([
      { id: 8, word: '旧词', pronunciation: null, correction_from: null },
    ])
    render(<DictionaryPane />)
    await screen.findByText('旧词')
    fireEvent.click(screen.getByTitle('dictionary.edit'))
    fireEvent.change(
      within(screen.getByRole('table')).getByRole('textbox', {
        name: 'dictionary.correctWord',
      }),
      { target: { value: '新专名' } },
    )
    fireEvent.click(screen.getByTitle('dictionary.save'))
    await waitFor(() => expect(updateDictionaryEntry).toHaveBeenCalledWith(8, '新专名', null, null))
    fireEvent.click(screen.getByTitle('dictionary.delete'))
    await waitFor(() => expect(removeDictionaryEntry).toHaveBeenCalledWith(8))
  })

  it('prevents duplicate submissions while a vocabulary change is pending', async () => {
    let finish: () => void = () => {}
    vi.mocked(addDictionaryEntry).mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finish = resolve
      }),
    )
    render(<DictionaryPane />)
    fireEvent.change(screen.getByPlaceholderText('dictionary.correctWordExample'), {
      target: { value: '专名' },
    })
    const add = screen.getByRole('button', { name: 'dictionary.add' })
    fireEvent.click(add)
    fireEvent.click(add)
    expect(addDictionaryEntry).toHaveBeenCalledOnce()
    expect(screen.getByPlaceholderText('dictionary.correctWordExample')).toBeDisabled()
    await act(async () => {
      finish()
    })
    expect(screen.getByPlaceholderText('dictionary.correctWordExample')).toBeEnabled()
  })

  it('shows engine capability while keeping the feature locked before activation', async () => {
    useActivationStore.setState((state) => ({
      status: { ...state.status!, activated: false, license_id: null },
    }))
    useAppStore.setState((state) => ({
      config: { ...state.config, stt_provider: 'funasr-nano' },
      savedConfig: { ...state.config, stt_provider: 'funasr-nano' },
    }))
    render(<DictionaryPane />)
    expect(screen.getByText('激活后生效')).toBeInTheDocument()
    expect(screen.getByText('dictionary.decoderHotwordsSupported')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('dictionary.correctWordExample')).toBeDisabled()
    expect(screen.getByRole('button', { name: 'dictionary.add' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '激活热词与纠错' })).toBeInTheDocument()
    await waitFor(() => expect(getDictionary).toHaveBeenCalled())
  })

  it('keeps existing words readable and deletable while add and edit are locked', async () => {
    useActivationStore.setState((state) => ({
      status: { ...state.status!, activated: false, license_id: null },
    }))
    vi.mocked(getDictionary).mockResolvedValue([
      { id: 7, word: '保留词条', pronunciation: null, correction_from: null },
    ])
    render(<DictionaryPane />)
    expect(await screen.findByText('保留词条')).toBeInTheDocument()
    expect(screen.getByTitle('dictionary.edit')).toBeDisabled()
    fireEvent.click(screen.getByTitle('dictionary.edit'))
    expect(updateDictionaryEntry).not.toHaveBeenCalled()
    expect(screen.getByTitle('dictionary.delete')).toBeEnabled()
    fireEvent.click(screen.getByTitle('dictionary.delete'))
    await waitFor(() => expect(removeDictionaryEntry).toHaveBeenCalledWith(7))
    expect(addDictionaryEntry).not.toHaveBeenCalled()
  })
})
