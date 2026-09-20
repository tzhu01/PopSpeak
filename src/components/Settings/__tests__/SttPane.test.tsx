import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor, cleanup, act } from '@testing-library/react'
import { SttPane } from '../SttPane'
import * as tauri from '../../../lib/tauri'
import { open as selectDirectory } from '@tauri-apps/plugin-dialog'

// Mock Tauri
vi.mock('../../../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/tauri')>()
  return Object.fromEntries(
    Object.entries(actual).map(([key, value]) => [
      key,
      typeof value === 'function' ? vi.fn() : value,
    ]),
  )
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}))

// Mock i18n
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const translations: Record<string, string> = {
        'settings.provider': 'Provider',
        'settings.apiKey': 'API Key',
        'settings.test': 'Test',
        'settings.enterApiKey': 'Enter API Key',
        'settings.connectionSuccess': 'Connection successful',
        'settings.connectionFailed': 'Connection failed',
        'settings.storedLocally': 'Stored locally',
        'settings.sttLanguage': 'STT Language',
        'settings.cloudSttPro': 'Cloud STT (Pro)',
        'settings.sttSignInHint': 'Sign in to use cloud STT',
        'settings.sttUpgradeHint': 'Upgrade to Pro to use cloud STT',
        'settings.sttProActive': 'Cloud STT active',
        'settings.sttEndpoint': 'STT Endpoint URL',
        'settings.xiaomiPreset': 'Use Xiaomi Preset',
        'settings.xiaomiPresetHint': 'Xiaomi hint',
        'settings.xiaomiPresetHintCompatible': 'Xiaomi compatible hint',
      }
      return translations[key] || key
    },
  }),
}))

// Mock stores
const mockAppStore = {
  config: {
    stt_provider: 'deepgram' as string,
    stt_api_key: '',
    stt_base_url: '',
    stt_model: '',
    stt_language: 'en',
    volcengine_auth_mode: 'app-token' as 'app-token' | 'api-key',
    volcengine_app_id: '',
    volcengine_credential: '',
    sensevoice_use_custom_dir: false,
    sensevoice_model_dir: '',
    sensevoice_language: 'auto',
    sensevoice_num_threads: 4,
    funasr_use_custom_dir: false,
    funasr_model_dir: '',
    funasr_num_threads: 4,
    native_asr: { model_id: 'qwen3-asr-1.7b', model_dir: '', num_threads: 4 },
  },
  updateConfig: vi.fn(),
  sttTestStatus: 'idle' as 'idle' | 'testing' | 'success' | 'error',
  setSttTestStatus: vi.fn(),
  sttLatencyMs: null as number | null,
  setSttLatencyMs: vi.fn(),
}

const mockAuthStore = {
  user: null as any,
  plan: null as any,
}

vi.mock('../../../stores/appStore', () => ({
  useAppStore: (selector: any) => {
    if (typeof selector === 'function') {
      return selector(mockAppStore)
    }
    return mockAppStore
  },
}))

vi.mock('../../../stores/authStore', () => ({
  useAuthStore: (selector: any) => {
    if (typeof selector === 'function') {
      return selector(mockAuthStore)
    }
    return mockAuthStore
  },
}))

describe('SttPane', () => {
  beforeEach(() => {
    // Reset mock store state
    mockAppStore.config = {
      stt_provider: 'deepgram',
      stt_api_key: '',
      stt_base_url: '',
      stt_model: '',
      stt_language: 'en',
      volcengine_auth_mode: 'app-token',
      volcengine_app_id: '',
      volcengine_credential: '',
      sensevoice_use_custom_dir: false,
      sensevoice_model_dir: '',
      sensevoice_language: 'auto',
      sensevoice_num_threads: 4,
      funasr_use_custom_dir: false,
      funasr_model_dir: '',
      funasr_num_threads: 4,
      native_asr: { model_id: 'qwen3-asr-1.7b', model_dir: '', num_threads: 4 },
    }
    mockAppStore.sttTestStatus = 'idle'
    mockAppStore.sttLatencyMs = null
    mockAuthStore.user = null
    mockAuthStore.plan = null

    // Clear all mock function calls
    vi.clearAllMocks()
    vi.mocked(tauri.getSenseVoicePaths).mockResolvedValue({
      model_dir: 'D:\\models\\sensevoice',
      display_dir: '.\\models\\sensevoice',
      source: 'package-relative',
      model_path: 'D:\\models\\sensevoice\\model.int8.onnx',
      tokens_path: 'D:\\models\\sensevoice\\tokens.txt',
      ready: true,
      is_custom: false,
    })
    vi.mocked(tauri.getLocalModelPaths).mockResolvedValue({
      default_model_ready: true,
      upgrade_model_ready: true,
      small_model_ready: false,
      turbo_model_ready: false,
      small_model_path: 'D:\\PopSpeak\\models\\whisper\\ggml-small-q5_1.bin',
    } as tauri.LocalModelPaths)
    vi.mocked(tauri.getNativeAsrCatalog).mockResolvedValue([])
    vi.mocked(tauri.getNativeAsrPaths).mockImplementation(
      async (modelId) =>
        ({
          model_id: modelId,
          ready: false,
          verified: false,
        }) as tauri.NativeAsrPaths,
    )
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  describe('Provider selection', () => {
    it('selects a supported explicit language for Cohere and English for Parakeet', () => {
      mockAppStore.config.stt_language = 'yue'
      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: /Cohere Transcribe/ }))
      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({ stt_language: 'zh' })
      fireEvent.click(screen.getByRole('button', { name: /Parakeet Unified/ }))
      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({ stt_language: 'en' })
    })

    it('does not label the Cohere language default as automatic detection', async () => {
      mockAppStore.config.stt_provider = 'native-asr'
      mockAppStore.config.native_asr.model_id = 'cohere-transcribe-03-2026'
      render(<SttPane />)
      expect(
        screen.getByRole('option', { name: '中文（默认；此模型不自动检测语言）' }),
      ).toHaveValue('multi')
      expect(screen.queryByRole('option', { name: /粤语/ })).not.toBeInTheDocument()
      expect(screen.getByRole('option', { name: /Greek/ })).toHaveValue('el')
      await waitFor(() => expect(tauri.getNativeAsrPaths).toHaveBeenCalled())
    })
    it('keeps an uninstalled requested model selected instead of retaining the previous model', async () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      vi.mocked(tauri.getLocalModelPaths).mockResolvedValue({
        small_model_ready: false,
        small_model_path: 'D:\\PopSpeak\\models\\whisper\\ggml-small-q5_1.bin',
      } as tauri.LocalModelPaths)
      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: /Whisper Small/ }))
      await screen.findByText('Whisper Small Q5_1尚未安装，请在下方下载后保存设置。')
      expect(mockAppStore.updateConfig).toHaveBeenLastCalledWith({
        whisper_model_path: 'D:\\PopSpeak\\models\\whisper\\ggml-small-q5_1.bin',
      })
    })

    it('does not overwrite a newer selection when an older model lookup finishes', async () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      let finishLookup!: (paths: tauri.LocalModelPaths) => void
      render(<SttPane />)
      vi.mocked(tauri.getLocalModelPaths).mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishLookup = resolve
          }),
      )
      fireEvent.click(screen.getByRole('button', { name: /Whisper Small/ }))
      fireEvent.click(screen.getByRole('button', { name: /SenseVoice Small/ }))
      await act(async () => {
        finishLookup({
          small_model_ready: true,
          small_model_path: 'D:\\old-selection.bin',
        } as tauri.LocalModelPaths)
      })
      expect(mockAppStore.updateConfig).not.toHaveBeenCalledWith({
        whisper_model_path: 'D:\\old-selection.bin',
      })
    })

    it('shows real model names while keeping technical configuration collapsed', async () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      const { container } = render(<SttPane />)
      expect(screen.getByRole('button', { name: /SenseVoice Small/ })).toHaveAttribute(
        'aria-pressed',
        'true',
      )
      expect(await screen.findByText('模型已就绪')).toBeInTheDocument()
      const ordinaryView = container.cloneNode(true) as HTMLElement
      ordinaryView.querySelectorAll('details').forEach((detail) => detail.remove())
      expect(ordinaryView.textContent).toContain('SenseVoice Small')
      expect(ordinaryView.textContent).not.toMatch(/model\.int8|D:\\/i)
      expect(screen.getByText(/SenseVoice-Small INT8/).closest('details')).not.toHaveAttribute(
        'open',
      )
    })

    it('updates config and resets state when provider changes', () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: /Fun-ASR-Nano/ }))

      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({
        stt_provider: 'funasr-nano',
        stt_api_key: '',
      })
      expect(mockAppStore.setSttTestStatus).toHaveBeenCalledWith('idle')
      expect(mockAppStore.setSttLatencyMs).toHaveBeenCalledWith(null)
    })

    it('only shows the streamlined recognition choices', () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      render(<SttPane />)
      const gallery = screen.getByRole('region', { name: '语音转文字模型' })
      expect(gallery.querySelectorAll('button')).toHaveLength(12)
      expect(gallery.textContent).toMatch(/SenseVoice Small/)
      expect(gallery.textContent).toMatch(/Fun-ASR-Nano/)
      expect(gallery.textContent).toMatch(/Whisper Tiny/)
      expect(gallery.textContent).not.toMatch(/Deepgram|AssemblyAI|GLM-ASR|Groq|SiliconFlow/)
    })

    it('checks all four native models and wires a native selection to its real backend', async () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      render(<SttPane />)
      await waitFor(() => expect(tauri.getNativeAsrPaths).toHaveBeenCalledTimes(4))
      for (const id of tauri.NATIVE_ASR_IDS)
        expect(tauri.getNativeAsrPaths).toHaveBeenCalledWith(id, '')
      fireEvent.click(screen.getByRole('button', { name: /Qwen3-ASR 1.7B/ }))
      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({
        stt_provider: 'native-asr',
        stt_api_key: '',
      })
      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({
        native_asr: { model_id: 'qwen3-asr-1.7b', model_dir: '', num_threads: 4 },
      })
    })

    it('selects the official SeedASR 2.0 endpoint and duration resource', () => {
      mockAppStore.config.stt_provider = 'sensevoice'
      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: /豆包 SeedASR/ }))

      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({
        stt_provider: 'volcengine-seedasr',
        stt_api_key: '',
        stt_model: 'volc.seedasr.sauc.duration',
        stt_base_url: 'wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async',
        volcengine_auth_mode: 'app-token',
      })
    })
  })

  describe('Volcengine SeedASR UI', () => {
    it('preserves credentials when choosing an already-selected cloud card', () => {
      mockAppStore.config.stt_provider = 'volcengine-seedasr'
      mockAppStore.config.volcengine_credential = 'saved-token'
      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: /豆包 SeedASR/ }))
      expect(mockAppStore.updateConfig).not.toHaveBeenCalled()
      expect(screen.getByDisplayValue('saved-token')).toBeInTheDocument()
    })

    it('defaults to trial APP ID and Access Token without asking for a browser cookie', () => {
      mockAppStore.config.stt_provider = 'volcengine-seedasr'
      mockAppStore.config.stt_model = 'volc.seedasr.sauc.duration'
      mockAppStore.config.volcengine_app_id = '1234567890'
      mockAppStore.config.volcengine_credential = 'saved-access-token'
      const { container } = render(<SttPane />)

      expect(
        screen.getByPlaceholderText('输入 Access Token（保存后下次自动读入）'),
      ).toBeInTheDocument()
      expect(screen.getByDisplayValue('1234567890')).toBeInTheDocument()
      expect(screen.getByDisplayValue('volc.seedasr.sauc.duration')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: /获取 API Key/ })).toBeInTheDocument()
      expect(container.textContent).toContain('不读取浏览器 Cookie')
      expect(container.textContent).toContain('明文写入本机 settings.json')
      expect(screen.getAllByRole('combobox')).toHaveLength(1)
    })

    it('passes trial credentials to the official SeedASR benchmark', async () => {
      mockAppStore.config.stt_provider = 'volcengine-seedasr'
      mockAppStore.config.volcengine_credential = 'access-token'
      mockAppStore.config.stt_model = 'volc.seedasr.sauc.duration'
      mockAppStore.config.volcengine_app_id = '1234567890'
      vi.mocked(tauri.benchSttConnection).mockResolvedValue(210)

      render(<SttPane />)
      fireEvent.click(screen.getByRole('button', { name: 'Test' }))

      await waitFor(() =>
        expect(tauri.benchSttConnection).toHaveBeenCalledWith(
          'access-token',
          'volcengine-seedasr',
          '',
          'volc.seedasr.sauc.duration',
          '1234567890',
          'app-token',
        ),
      )
    })
  })

  describe('Cloud provider UI', () => {
    it('shows cloud info when provider is cloud and user not signed in', () => {
      mockAppStore.config.stt_provider = 'cloud'
      render(<SttPane />)
      expect(screen.getByText('Sign in to use cloud STT')).toBeInTheDocument()
    })

    it('shows upgrade hint when user is signed in but not pro', () => {
      mockAppStore.config.stt_provider = 'cloud'
      mockAuthStore.user = { id: '1', email: 'test@example.com' }
      mockAuthStore.plan = 'free'

      render(<SttPane />)
      expect(screen.getByText('Upgrade to Pro to use cloud STT')).toBeInTheDocument()
    })

    it('shows active status when user is pro', () => {
      mockAppStore.config.stt_provider = 'cloud'
      mockAuthStore.user = { id: '1', email: 'test@example.com' }
      mockAuthStore.plan = 'pro'

      render(<SttPane />)
      expect(screen.getByText('Cloud STT active')).toBeInTheDocument()
    })

    it('hides API key input when provider is cloud', () => {
      mockAppStore.config.stt_provider = 'cloud'

      const { container } = render(<SttPane />)
      const inputs = container.querySelectorAll('input[placeholder="Enter API Key"]')
      expect(inputs.length).toBe(0)
    })
  })

  describe('API Key input', () => {
    it('renders API key input with current value', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      const { container } = render(<SttPane />)
      const input = container.querySelector(
        'input[placeholder="Enter API Key"]',
      ) as HTMLInputElement
      expect(input.value).toBe('sk-test123')
      expect(input.type).toBe('password')
    })

    it('updates config and resets test state when API key changes', () => {
      const { container } = render(<SttPane />)
      const input = container.querySelector(
        'input[placeholder="Enter API Key"]',
      ) as HTMLInputElement

      fireEvent.change(input, { target: { value: 'sk-new-key' } })

      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({ stt_api_key: 'sk-new-key' })
      expect(mockAppStore.setSttTestStatus).toHaveBeenCalledWith('idle')
      expect(mockAppStore.setSttLatencyMs).toHaveBeenCalledWith(null)
    })
  })

  describe('Fun-ASR-Nano offline pack', () => {
    it('shows auto-indexed CPU components without an API key', async () => {
      mockAppStore.config.stt_provider = 'funasr-nano'
      vi.mocked(tauri.getFunAsrPaths).mockResolvedValue({
        runtime_path: 'runtimes/funasr/llama-funasr-pipe-host-avx2.exe',
        runtime_variant: 'AVX2',
        model_dir: 'models/funasr-nano',
        display_dir: '.\\models\\funasr-nano',
        source: 'package-relative',
        encoder_path: 'models/funasr-nano/funasr-encoder-f16.gguf',
        llm_path: 'models/funasr-nano/qwen3-0.6b-q4km.gguf',
        vad_path: 'models/funasr-nano/fsmn-vad.gguf',
        runtime_ready: true,
        encoder_ready: true,
        llm_ready: true,
        vad_ready: true,
        ready: true,
        is_custom: false,
        model_version: '2026.06-q4km-r1',
        revision: '51dcf492',
        quantization: 'Encoder F16 + Qwen3-0.6B Q4_K_M',
        installed_bytes: 955271296,
        expected_bytes: 955271296,
        verified: true,
        update_available: false,
      })
      vi.mocked(tauri.getFunAsrRuntimeStatus).mockResolvedValue({
        running: true,
        ready: true,
        pid: 1234,
        runtime_variant: 'AVX2',
        last_error: '',
      })

      const { container } = render(<SttPane />)
      expect(container.querySelector('input[placeholder="Enter API Key"]')).toBeNull()
      expect((await screen.findAllByText('已安装')).length).toBeGreaterThan(0)
      expect(screen.getByText('本地识别服务')).toBeInTheDocument()
      expect(screen.getByText('文字解码组件（462 MB）')).toBeInTheDocument()
      expect(container.textContent).not.toContain('models/funasr-nano/funasr-encoder-f16.gguf')
      expect(screen.getByText(/使用国内下载源/)).toBeInTheDocument()
      const ordinaryView = container.cloneNode(true) as HTMLElement
      ordinaryView.querySelectorAll('details').forEach((detail) => detail.remove())
      expect(ordinaryView.textContent).toMatch(/Fun-ASR-Nano/)
      expect(ordinaryView.textContent).not.toMatch(/Qwen3-0.6B|funasr-encoder/)
      expect(screen.getByText(/模型管理与高级设置/).closest('details')).not.toHaveAttribute('open')
    })
  })

  describe('Removed provider presets', () => {
    it('does not offer the removed audio-understanding preset', () => {
      render(<SttPane />)
      expect(screen.queryByRole('button', { name: 'Use Xiaomi Preset' })).not.toBeInTheDocument()
      expect(screen.queryByRole('button', { name: /Xiaomi|MiMo/ })).not.toBeInTheDocument()
    })
  })

  describe('Test button and latency display', () => {
    it('test button is disabled when API key is empty', () => {
      render(<SttPane />)
      const buttons = screen.getAllByRole('button', { name: /test/i })
      const button = buttons[0]
      expect(button).toBeDisabled()
    })

    it('test button is enabled when API key is present', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      render(<SttPane />)
      const buttons = screen.getAllByRole('button', { name: /test/i })
      const button = buttons[0]
      expect(button).not.toBeDisabled()
    })

    it('test button is disabled during testing', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'testing'
      render(<SttPane />)
      const buttons = screen.getAllByRole('button', { name: /test/i })
      const button = buttons[0]
      expect(button).toBeDisabled()
    })

    it('calls benchSttConnection on test button click', async () => {
      const mockBenchStt = vi.mocked(tauri.benchSttConnection)
      mockBenchStt.mockResolvedValue(234)

      mockAppStore.config.stt_api_key = 'sk-test123'
      render(<SttPane />)
      const buttons = screen.getAllByRole('button', { name: /test/i })
      const button = buttons[0]

      fireEvent.click(button)

      await waitFor(() => {
        expect(mockAppStore.setSttTestStatus).toHaveBeenCalledWith('testing')
        expect(mockAppStore.setSttLatencyMs).toHaveBeenCalledWith(null)
      })

      await waitFor(() => {
        expect(mockBenchStt).toHaveBeenCalledWith('sk-test123', 'deepgram', '', '', '', 'app-token')
      })
    })

    it('displays latency in milliseconds when test succeeds', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'success'
      mockAppStore.sttLatencyMs = 234

      render(<SttPane />)
      expect(screen.getByText('234ms')).toBeInTheDocument()
    })

    it('displays generic success message when latency is null', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'success'
      mockAppStore.sttLatencyMs = null

      render(<SttPane />)
      expect(screen.getByText('Connection successful')).toBeInTheDocument()
    })

    it('shows error state UI', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'error'

      render(<SttPane />)
      expect(screen.getByText('Connection failed')).toBeInTheDocument()
    })

    it('does not display latency when status is error', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'error'
      mockAppStore.sttLatencyMs = 234

      render(<SttPane />)
      expect(screen.queryByText('234ms')).not.toBeInTheDocument()
      expect(screen.getByText('Connection failed')).toBeInTheDocument()
    })
  })

  describe('Language selection', () => {
    it('renders language dropdown with current value', () => {
      render(<SttPane />)
      const selects = screen.getAllByRole('combobox')
      const languageSelect = selects[0]
      expect(languageSelect).toHaveValue('en')
    })

    it('updates config when language changes', () => {
      render(<SttPane />)
      const selects = screen.getAllByRole('combobox')
      const languageSelect = selects[0]

      fireEvent.change(languageSelect, { target: { value: 'zh' } })

      expect(mockAppStore.updateConfig).toHaveBeenCalledWith({ stt_language: 'zh' })
    })
  })

  describe('SenseVoice model directory', () => {
    it('opens the resolved model directory through the backend command', async () => {
      mockAppStore.config = {
        ...mockAppStore.config,
        stt_provider: 'sensevoice',
        sensevoice_use_custom_dir: false,
        sensevoice_model_dir: '',
        sensevoice_language: 'auto',
        sensevoice_num_threads: 4,
      }
      vi.mocked(tauri.getSenseVoicePaths).mockResolvedValue({
        model_dir: 'D:\\models\\sensevoice',
        display_dir: '.\\models\\sensevoice',
        source: 'package-relative',
        model_path: 'D:\\models\\sensevoice\\model.int8.onnx',
        tokens_path: 'D:\\models\\sensevoice\\tokens.txt',
        ready: true,
        is_custom: false,
      })
      vi.mocked(tauri.openSenseVoiceModelDirectory).mockResolvedValue()

      render(<SttPane />)
      const button = await screen.findByRole('button', { name: '打开目录' })
      fireEvent.click(button)

      await waitFor(() => {
        expect(tauri.openSenseVoiceModelDirectory).toHaveBeenCalledWith('')
      })
    })

    it('lets the user select and persist a custom model directory', async () => {
      mockAppStore.config = {
        ...mockAppStore.config,
        stt_provider: 'sensevoice',
        sensevoice_use_custom_dir: false,
      }
      vi.mocked(tauri.getSenseVoicePaths).mockResolvedValue({
        model_dir: 'D:\\PopSpeak\\models\\sensevoice',
        display_dir: '.\\models\\sensevoice',
        source: 'package-relative',
        model_path: 'D:\\PopSpeak\\models\\sensevoice\\model.int8.onnx',
        tokens_path: 'D:\\PopSpeak\\models\\sensevoice\\tokens.txt',
        ready: true,
        is_custom: false,
      })
      vi.mocked(selectDirectory).mockResolvedValue('E:\\MyModels\\sensevoice')

      render(<SttPane />)
      fireEvent.click(await screen.findByRole('button', { name: '选择目录' }))

      await waitFor(() => {
        expect(mockAppStore.updateConfig).toHaveBeenCalledWith({
          sensevoice_use_custom_dir: true,
          sensevoice_model_dir: 'E:\\MyModels\\sensevoice',
        })
      })
    })

    it('forces a verified replacement when re-downloading a ready model', async () => {
      mockAppStore.config = {
        ...mockAppStore.config,
        stt_provider: 'sensevoice',
        sensevoice_use_custom_dir: false,
      }
      vi.mocked(tauri.getSenseVoicePaths).mockResolvedValue({
        model_dir: 'D:\\PopSpeak\\models\\sensevoice',
        display_dir: '.\\models\\sensevoice',
        source: 'package-relative',
        model_path: 'D:\\PopSpeak\\models\\sensevoice\\model.int8.onnx',
        tokens_path: 'D:\\PopSpeak\\models\\sensevoice\\tokens.txt',
        ready: true,
        is_custom: false,
      })
      vi.mocked(tauri.downloadSenseVoice).mockResolvedValue()

      render(<SttPane />)
      fireEvent.click(await screen.findByRole('button', { name: '重新下载模型' }))

      expect(screen.getByText('正在准备下载')).toBeInTheDocument()
      await waitFor(() => {
        expect(tauri.downloadSenseVoice).toHaveBeenCalledWith('', true)
      })
    })
  })

  describe('Integration: state reset on config changes', () => {
    it('resets latency when API key changes after successful test', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'success'
      mockAppStore.sttLatencyMs = 234

      const { container } = render(<SttPane />)

      // Verify latency is displayed
      expect(screen.getByText('234ms')).toBeInTheDocument()

      // Change API key
      const input = container.querySelector(
        'input[placeholder="Enter API Key"]',
      ) as HTMLInputElement
      fireEvent.change(input, { target: { value: 'sk-new-key' } })

      // Verify state was reset
      expect(mockAppStore.setSttLatencyMs).toHaveBeenCalledWith(null)
      expect(mockAppStore.setSttTestStatus).toHaveBeenCalledWith('idle')
    })

    it('resets latency when provider changes after successful test', () => {
      mockAppStore.config.stt_api_key = 'sk-test123'
      mockAppStore.sttTestStatus = 'success'
      mockAppStore.sttLatencyMs = 234

      render(<SttPane />)

      // Change provider
      fireEvent.click(screen.getByRole('button', { name: /自定义云端接口/ }))

      // Verify state was reset
      expect(mockAppStore.setSttLatencyMs).toHaveBeenCalledWith(null)
      expect(mockAppStore.setSttTestStatus).toHaveBeenCalledWith('idle')
    })
  })
})
