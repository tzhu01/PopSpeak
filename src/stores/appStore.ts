import { create } from 'zustand'
import { updateConfig as persistConfig, setAutoStart } from '../lib/tauri'

export type PipelineState = 'idle' | 'recording' | 'transcribing' | 'polishing' | 'outputting'

export type SttProvider =
  | 'deepgram'
  | 'assemblyai'
  | 'glm-asr'
  | 'openai-whisper'
  | 'groq-whisper'
  | 'siliconflow'
  | 'custom-whisper'
  | 'xiaomi-mimo'
  | 'local-whisper'
  | 'funasr-nano'
  | 'native-asr'
  | 'sensevoice'
  | 'volcengine-seedasr'
  | 'cloud'
export type LlmProvider =
  | 'zhipu'
  | 'deepseek'
  | 'siliconflow'
  | 'openai'
  | 'gemini'
  | 'moonshot'
  | 'qwen'
  | 'groq'
  | 'claude'
  | 'ollama'
  | 'local-llama'
  | 'openrouter'
  | 'cloud'
export type OutputMode = 'keyboard' | 'clipboard' | 'editor'
export type HotkeyMode = 'hold' | 'toggle'
export type Theme = 'light' | 'dark' | 'system'

export interface HistoryEntry {
  id: number
  created_at: string
  app_name: string
  app_type: string
  raw_text: string
  polished_text: string
  language: string | null
  duration_ms: number | null
}

export interface DictionaryEntry {
  id: number
  word: string
  pronunciation: string | null
  correction_from?: string | null
}

export interface CustomCloudConfig {
  vendor: 'whisper' | 'bytedance' | 'aliyun' | 'tencent' | 'iflytek' | 'baidu'
  app_id: string
  api_key: string
  api_secret: string
  access_token: string
  endpoint: string
  model: string
  region: string
}

export interface AppConfig {
  native_asr: { model_id: string; model_dir: string; num_threads: number }
  custom_cloud: CustomCloudConfig
  stt_provider: SttProvider
  stt_api_key: string
  stt_base_url: string
  stt_model: string
  stt_language: string
  volcengine_auth_mode: 'app-token' | 'api-key'
  volcengine_app_id: string
  volcengine_credential: string
  llm_provider: LlmProvider
  llm_api_key: string
  llm_model: string
  llm_base_url: string
  polish_enabled: boolean
  polish_mode: string
  translate_enabled: boolean
  target_lang: string
  whisper_cli_path: string
  whisper_model_path: string
  whisper_lora_path: string
  sensevoice_language: string
  sensevoice_num_threads: number
  sensevoice_use_custom_dir: boolean
  sensevoice_model_dir: string
  funasr_use_custom_dir: boolean
  funasr_model_dir: string
  funasr_num_threads: number
  local_llm_model: string
  local_llm_port: number
  local_llm_threads: number
  local_llm_ctx_size: number
  local_llm_model_dir: string
  hotkey: string
  hotkey_mode: HotkeyMode
  output_mode: OutputMode
  editor_auto_hide_enabled: boolean
  editor_auto_hide_seconds: number
  selected_text_enabled: boolean
  theme: Theme
  auto_start: boolean
  close_to_tray: boolean
  start_minimized: boolean
  max_recording_seconds: number
  ui_language: string
  capsule_enabled: boolean
  capsule_always_on_top: boolean
  capsule_auto_hide: boolean
  capsule_preview_enabled: boolean
  audio_device_name: string
  vad_enabled: boolean
  noise_suppression_enabled: boolean
}

export type TestStatus = 'idle' | 'testing' | 'success' | 'error'

interface AppState {
  // Pipeline
  pipelineState: PipelineState
  setPipelineState: (state: PipelineState) => void
  activeSessionId: number | null
  lastPipelineStateRevision: number
  lastPipelineMessageRevision: number

  // Recording
  audioVolume: number
  setAudioVolume: (v: number) => void
  previewTranscript: string
  setPreviewTranscript: (t: string) => void
  partialTranscript: string
  setPartialTranscript: (t: string) => void
  finalTranscript: string
  setFinalTranscript: (t: string) => void
  polishedText: string
  setPolishedText: (t: string) => void
  appendPolishedChunk: (chunk: string) => void
  recordingDuration: number
  setRecordingDuration: (d: number) => void
  targetApp: string
  setTargetApp: (app: string) => void
  resolvedTranscript: string
  setResolvedTranscript: (t: string) => void
  lastPreviewRevision: number
  lastPartialRevision: number
  lastFinalRevision: number
  lastLlmRevision: number
  lastResolvedRevision: number
  authoritativeFinalSeen: boolean

  // Config
  config: AppConfig
  setConfig: (config: AppConfig) => void
  updateConfig: (partial: Partial<AppConfig>) => void

  // History
  history: HistoryEntry[]
  setHistory: (h: HistoryEntry[]) => void
  patchHistoryEntry: (id: number, polishedText: string) => void
  removeHistoryEntry: (id: number) => void

  // Dictionary
  dictionary: DictionaryEntry[]
  setDictionary: (d: DictionaryEntry[]) => void

  // Onboarding
  onboardingCompleted: boolean
  setOnboardingCompleted: (done: boolean) => void
  onboardingStep: number
  setOnboardingStep: (step: number) => void
  onboardingMode: 'cloud' | 'byok' | 'local' | null
  setOnboardingMode: (mode: 'cloud' | 'byok' | 'local' | null) => void

  // Capsule
  capsuleExpanded: boolean
  setCapsuleExpanded: (expanded: boolean) => void

  // Connection test status
  sttTestStatus: TestStatus
  setSttTestStatus: (s: TestStatus) => void
  llmTestStatus: TestStatus
  setLlmTestStatus: (s: TestStatus) => void

  // Latency benchmark results (ms), null = not yet measured
  sttLatencyMs: number | null
  setSttLatencyMs: (ms: number | null) => void
  llmLatencyMs: number | null
  setLlmLatencyMs: (ms: number | null) => void

  // LLM model list cache (persists across tab switches)
  llmModels: string[]
  setLlmModels: (models: string[]) => void

  // Pipeline error
  pipelineError: string | null
  setPipelineError: (error: string | null) => void

  // macOS Accessibility permission
  accessibilityTrusted: boolean
  setAccessibilityTrusted: (trusted: boolean) => void

  // Context menu
  contextMenuOpen: boolean
  setContextMenuOpen: (open: boolean) => void
  contextMenuReady: boolean
  setContextMenuReady: (ready: boolean) => void

  // Reset recording state
  resetRecording: () => void

  // Config snapshot for dirty detection
  savedConfig: AppConfig | null
  setSavedConfig: (config: AppConfig) => void
  applyBackendConfig: (config: AppConfig, submitted?: AppConfig) => void
  applyCapsulePreferences: (
    confirmed: AppConfig,
    patch: Partial<
      Pick<
        AppConfig,
        | 'capsule_enabled'
        | 'capsule_always_on_top'
        | 'capsule_auto_hide'
        | 'capsule_preview_enabled'
      >
    >,
  ) => void
  capsulePreferencesSaving: boolean
  configSaving: boolean
  configSaveError: string | null
  autoStartSyncPending: boolean
  saveConfig: () => Promise<boolean>
  resetConfig: () => void
}

const isMac =
  typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0

const defaultConfig: AppConfig = {
  native_asr: { model_id: 'qwen3-asr-1.7b', model_dir: '', num_threads: 4 },
  custom_cloud: {
    vendor: 'whisper',
    app_id: '',
    api_key: '',
    api_secret: '',
    access_token: '',
    endpoint: '',
    model: '',
    region: '',
  },
  stt_provider: 'sensevoice',
  stt_api_key: '',
  stt_base_url: '',
  stt_model: '',
  stt_language: 'multi',
  volcengine_auth_mode: 'app-token',
  volcengine_app_id: '',
  volcengine_credential: '',
  llm_provider: 'local-llama',
  llm_api_key: '',
  llm_model: 'qwen2.5-0.5b-instruct',
  llm_base_url: 'http://127.0.0.1:11434/v1',
  polish_enabled: false,
  polish_mode: 'fast',
  translate_enabled: false,
  target_lang: 'en',
  whisper_cli_path: '',
  whisper_model_path: '',
  whisper_lora_path: '',
  sensevoice_language: 'auto',
  sensevoice_num_threads: 2,
  sensevoice_use_custom_dir: false,
  sensevoice_model_dir: '',
  funasr_use_custom_dir: false,
  funasr_model_dir: '',
  funasr_num_threads: 4,
  local_llm_model: 'qwen2.5-0.5b-instruct-q4_k_m.gguf',
  local_llm_port: 11434,
  local_llm_threads: 4,
  local_llm_ctx_size: 2048,
  local_llm_model_dir: '',
  hotkey: isMac ? 'Alt+/' : 'Ctrl+/',
  hotkey_mode: 'hold',
  output_mode: 'clipboard',
  editor_auto_hide_enabled: true,
  editor_auto_hide_seconds: 5,
  selected_text_enabled: false,
  theme: 'system',
  auto_start: false,
  close_to_tray: true,
  start_minimized: false,
  max_recording_seconds: 30,
  ui_language: 'zh',
  capsule_enabled: true,
  capsule_always_on_top: true,
  capsule_auto_hide: false,
  capsule_preview_enabled: true,
  audio_device_name: '',
  vad_enabled: true,
  noise_suppression_enabled: true,
}

export const useAppStore = create<AppState>((set, get) => ({
  pipelineState: 'idle',
  setPipelineState: (pipelineState) => set({ pipelineState }),
  activeSessionId: null,
  lastPipelineStateRevision: -1,
  lastPipelineMessageRevision: -1,

  audioVolume: 0,
  setAudioVolume: (audioVolume) => set({ audioVolume }),
  previewTranscript: '',
  setPreviewTranscript: (previewTranscript) => set({ previewTranscript }),
  partialTranscript: '',
  setPartialTranscript: (partialTranscript) => set({ partialTranscript }),
  finalTranscript: '',
  setFinalTranscript: (finalTranscript) => set({ finalTranscript }),
  polishedText: '',
  setPolishedText: (polishedText) => set({ polishedText }),
  appendPolishedChunk: (chunk) => set((s) => ({ polishedText: s.polishedText + chunk })),
  recordingDuration: 0,
  setRecordingDuration: (recordingDuration) => set({ recordingDuration }),
  targetApp: '',
  setTargetApp: (targetApp) => set({ targetApp }),
  resolvedTranscript: '',
  setResolvedTranscript: (resolvedTranscript) => set({ resolvedTranscript }),
  lastPreviewRevision: -1,
  lastPartialRevision: -1,
  lastFinalRevision: -1,
  lastLlmRevision: -1,
  lastResolvedRevision: -1,
  authoritativeFinalSeen: false,

  config: defaultConfig,
  setConfig: (config) => set({ config }),
  updateConfig: (partial) => set((s) => ({ config: { ...s.config, ...partial } })),

  history: [],
  setHistory: (history) => set({ history }),
  patchHistoryEntry: (id, polishedText) =>
    set((s) => ({
      history: s.history.map((h) => (h.id === id ? { ...h, polished_text: polishedText } : h)),
    })),
  removeHistoryEntry: (id) => set((s) => ({ history: s.history.filter((h) => h.id !== id) })),

  dictionary: [],
  setDictionary: (dictionary) => set({ dictionary }),

  onboardingCompleted: false,
  setOnboardingCompleted: (onboardingCompleted) => set({ onboardingCompleted }),
  onboardingStep: 0,
  setOnboardingStep: (onboardingStep) => set({ onboardingStep }),
  onboardingMode: 'local',
  setOnboardingMode: (onboardingMode) => set({ onboardingMode }),

  capsuleExpanded: false,
  setCapsuleExpanded: (capsuleExpanded) => set({ capsuleExpanded }),

  sttTestStatus: 'idle',
  setSttTestStatus: (sttTestStatus) => set({ sttTestStatus }),
  llmTestStatus: 'idle',
  setLlmTestStatus: (llmTestStatus) => set({ llmTestStatus }),

  sttLatencyMs: null,
  setSttLatencyMs: (sttLatencyMs) => set({ sttLatencyMs }),
  llmLatencyMs: null,
  setLlmLatencyMs: (llmLatencyMs) => set({ llmLatencyMs }),

  llmModels: [],
  setLlmModels: (llmModels) => set({ llmModels }),

  pipelineError: null,
  setPipelineError: (pipelineError) => set({ pipelineError }),

  accessibilityTrusted: true,
  setAccessibilityTrusted: (accessibilityTrusted) => set({ accessibilityTrusted }),

  contextMenuOpen: false,
  setContextMenuOpen: (contextMenuOpen) => set({ contextMenuOpen }),
  contextMenuReady: false,
  setContextMenuReady: (contextMenuReady) => set({ contextMenuReady }),

  resetRecording: () =>
    set({
      // Keep the last accepted session/revision as a high-water mark. Clearing
      // them here would let a delayed `recording` event from that old session
      // become current again while the next native start is still pending.
      audioVolume: 0,
      previewTranscript: '',
      partialTranscript: '',
      finalTranscript: '',
      polishedText: '',
      resolvedTranscript: '',
      lastPreviewRevision: -1,
      lastPartialRevision: -1,
      lastFinalRevision: -1,
      lastLlmRevision: -1,
      lastResolvedRevision: -1,
      authoritativeFinalSeen: false,
      recordingDuration: 0,
    }),

  savedConfig: null,
  setSavedConfig: (savedConfig) => set({ savedConfig }),
  applyCapsulePreferences: (confirmed, patch) => {
    get().applyBackendConfig(confirmed)
    // Immediate controls are authoritative only for the fields just requested.
    // Preserve every unrelated unsaved setting in the main window.
    set((state) => ({
      config: {
        ...state.config,
        ...Object.fromEntries(
          Object.keys(patch).map((key) => [key, confirmed[key as keyof AppConfig]]),
        ),
      },
    }))
  },
  applyBackendConfig: (confirmed, submitted) =>
    set((state) => {
      // Backend events confirm the active configuration. Keep any draft values
      // edited after a save began, rather than replacing them with that snapshot.
      const baseline = submitted ?? state.savedConfig
      const config = baseline
        ? (Object.fromEntries(
            Object.entries(confirmed).map(([name, value]) => {
              const key = name as keyof AppConfig
              const draft = state.config[key]
              return [key, draft === baseline[key] || draft === value ? value : draft]
            }),
          ) as unknown as AppConfig)
        : { ...confirmed }
      return { config, savedConfig: { ...confirmed } }
    }),
  configSaving: false,
  capsulePreferencesSaving: false,
  configSaveError: null,
  autoStartSyncPending: false,
  saveConfig: async () => {
    if (get().configSaving || get().capsulePreferencesSaving) return false
    const snapshot = { ...get().config }
    const previous = get().savedConfig
    set({ configSaving: true, configSaveError: null })
    let persisted = false
    try {
      const confirmed = await persistConfig(snapshot)
      persisted = true
      get().applyBackendConfig(confirmed, snapshot)
      if (previous?.auto_start !== confirmed.auto_start) set({ autoStartSyncPending: true })
      if (get().autoStartSyncPending) {
        await setAutoStart(confirmed.auto_start)
        set({ autoStartSyncPending: false })
      }
      return true
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error)
      set({
        configSaveError: persisted
          ? `设置已保存，但开机启动同步失败：${detail}`
          : `设置未保存，仍使用原来的识别模式：${detail}`,
      })
      return false
    } finally {
      set({ configSaving: false })
    }
  },
  resetConfig: () =>
    set((s) =>
      s.configSaving
        ? {}
        : { ...(s.savedConfig ? { config: { ...s.savedConfig } } : {}), configSaveError: null },
    ),
}))
