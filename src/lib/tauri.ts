import { invoke } from '@tauri-apps/api/core'

export const NATIVE_ASR_IDS = [
  'qwen3-asr-1.7b',
  'cohere-transcribe-03-2026',
  'nemotron-3.5-asr-streaming-0.6b',
  'parakeet-unified-en-0.6b',
] as const
export type NativeAsrModelId = (typeof NATIVE_ASR_IDS)[number]
export interface NativeAsrModelInfo {
  id: string
  name: string
  file_name: string
  bytes: number
  sha256: string
  revision: string
  license: string
  upstream_url: string
  sources: { name: string; url: string }[]
}
export interface NativeAsrPaths {
  model_id: string
  model_dir: string
  model_path: string
  display_dir: string
  source: string
  is_custom: boolean
  ready: boolean
  verified: boolean
  installed_bytes: number
  expected_bytes: number
  model_version: string
  update_available: boolean
}
export interface NativeAsrProgress {
  model_id: string
  current: number
  total: number
  percent: number
  status: string
  message: string
  speed_bytes_per_sec: number
  attempt: number
  source: string
}
export const getNativeAsrCatalog = () => invoke<NativeAsrModelInfo[]>('get_native_asr_catalog')
export const getNativeAsrPaths = (modelId: string, customDir = '') =>
  invoke<NativeAsrPaths>('get_native_asr_paths', { modelId, customDir: customDir || null })
export const downloadNativeAsr = (modelId: string, customDir = '') =>
  invoke<NativeAsrPaths>('download_native_asr_model', { modelId, customDir: customDir || null })
export const deleteNativeAsr = (modelId: string, customDir = '') =>
  invoke<NativeAsrPaths>('delete_native_asr_model', { modelId, customDir: customDir || null })
export const cancelNativeAsr = (modelId: string) =>
  invoke<void>('cancel_native_asr_download', { modelId })
import type {
  AppConfig,
  HistoryEntry,
  DictionaryEntry,
  CustomCloudConfig,
} from '../stores/appStore'

// Pipeline commands
export async function startRecording(): Promise<void> {
  return invoke('start_recording')
}

export async function stopRecording(): Promise<void> {
  return invoke('stop_recording')
}

export async function abortRecording(): Promise<void> {
  return invoke('abort_recording')
}

export async function listAudioInputDevices(): Promise<string[]> {
  return invoke('list_audio_input_devices')
}

export interface UpdateInfo {
  current_version: string
  latest_version: string
  available: boolean
  release_url: string
}

export async function checkForUpdates(): Promise<UpdateInfo> {
  return invoke('check_for_updates')
}

// Config commands
export async function getConfig(): Promise<AppConfig> {
  return invoke('get_config')
}

export async function updateConfig(config: AppConfig): Promise<AppConfig> {
  return invoke('update_config', { config })
}

export type CapsulePreferencesPatch = Partial<
  Pick<
    AppConfig,
    'capsule_enabled' | 'capsule_always_on_top' | 'capsule_auto_hide' | 'capsule_preview_enabled'
  >
>

export async function patchCapsulePreferences(patch: CapsulePreferencesPatch): Promise<AppConfig> {
  return invoke('patch_capsule_preferences', { patch })
}

// Connection test
export async function testSttConnection(
  apiKey: string,
  provider: string,
  sttBaseUrl?: string,
  sttModel?: string,
  volcengineAppId?: string,
  volcengineAuthMode?: string,
  customCloud?: CustomCloudConfig,
): Promise<boolean> {
  return invoke('test_stt_connection', {
    apiKey,
    provider,
    sttBaseUrl,
    sttModel,
    volcengineAppId,
    volcengineAuthMode,
    customCloud,
  })
}

export async function testLlmConnection(
  apiKey: string,
  provider: string,
  baseUrl: string,
  model: string,
): Promise<boolean> {
  return invoke('test_llm_connection', { apiKey, provider, baseUrl, model })
}

// Latency benchmark — returns round-trip time in milliseconds
export async function benchSttConnection(
  apiKey: string,
  provider: string,
  sttBaseUrl?: string,
  sttModel?: string,
  volcengineAppId?: string,
  volcengineAuthMode?: string,
  customCloud?: CustomCloudConfig,
): Promise<number> {
  return invoke('bench_stt_connection', {
    apiKey,
    provider,
    sttBaseUrl,
    sttModel,
    volcengineAppId,
    volcengineAuthMode,
    customCloud,
  })
}

export async function benchLlmConnection(
  apiKey: string,
  provider: string,
  baseUrl: string,
  model: string,
): Promise<number> {
  return invoke('bench_llm_connection', { apiKey, provider, baseUrl, model })
}

// LLM models
export async function fetchLlmModels(apiKey: string, baseUrl: string): Promise<string[]> {
  return invoke('fetch_llm_models', { apiKey, baseUrl })
}

// Hotkey
export async function updateHotkey(hotkey: string): Promise<void> {
  return invoke('update_hotkey', { hotkey })
}

export async function pauseHotkey(): Promise<void> {
  return invoke('pause_hotkey')
}

export async function resumeHotkey(): Promise<void> {
  return invoke('resume_hotkey')
}

// History
export async function getHistory(limit: number, offset: number): Promise<HistoryEntry[]> {
  return invoke('get_history', { limit, offset })
}

export async function clearHistory(): Promise<void> {
  return invoke('clear_history')
}

export async function deleteHistoryEntry(id: number): Promise<void> {
  return invoke('delete_history_entry', { id })
}

export interface RewardSummary {
  total_points: number
  today_points: number
  daily_limit: number
  minimum_duration_ms: number
  local_day: string
  redemption_available: boolean
  recent_activity: { history_id: number; points: number; credited_at: string }[]
}

/** Read-only local experience points. No redemption or cloud balance is implied. */
export async function getRewardSummary(): Promise<RewardSummary> {
  return invoke('get_reward_summary')
}

// Dictionary
export async function getDictionary(): Promise<DictionaryEntry[]> {
  return invoke('get_dictionary')
}

export async function addDictionaryEntry(
  word: string,
  pronunciation: string | null,
  correctionFrom: string | null = null,
): Promise<void> {
  return invoke('add_dictionary_entry', { word, pronunciation, correctionFrom })
}

export async function updateDictionaryEntry(
  id: number,
  word: string,
  pronunciation: string | null,
  correctionFrom: string | null = null,
): Promise<void> {
  return invoke('update_dictionary_entry', { id, word, pronunciation, correctionFrom })
}

export async function removeDictionaryEntry(id: number): Promise<void> {
  return invoke('remove_dictionary_entry', { id })
}

// Auto-start
export async function setAutoStart(enabled: boolean): Promise<void> {
  return invoke('set_auto_start', { enabled })
}

// macOS Accessibility permission
export async function checkAccessibilityPermission(): Promise<boolean> {
  return invoke('check_accessibility_permission')
}

export async function requestAccessibilityPermission(): Promise<boolean> {
  return invoke('request_accessibility_permission')
}

// Onboarding persistence via tauri-plugin-store
export async function loadOnboardingCompleted(): Promise<boolean> {
  try {
    const { load } = await import('@tauri-apps/plugin-store')
    const store = await load('settings.json')
    const val = await store.get<boolean>('onboarding_completed')
    return val === true
  } catch {
    return false
  }
}

export async function saveOnboardingCompleted(): Promise<void> {
  try {
    const { load } = await import('@tauri-apps/plugin-store')
    const store = await load('settings.json')
    await store.set('onboarding_completed', true)
  } catch (e) {
    console.error('Failed to persist onboarding state:', e)
  }
}

// Local Whisper model management
export interface LocalModelPaths {
  cli_path: string
  model_dir: string
  default_model_path: string
  default_model_ready: boolean
  upgrade_model_path: string
  upgrade_model_ready: boolean
  small_model_path: string
  small_model_ready: boolean
  turbo_model_path: string
  turbo_model_ready: boolean
}

export async function getLocalModelPaths(customDir?: string): Promise<LocalModelPaths> {
  return invoke('get_local_model_paths', { customDir: customDir || null })
}

export interface FunAsrPaths {
  runtime_path: string
  runtime_variant: string
  model_dir: string
  display_dir: string
  source: string
  encoder_path: string
  llm_path: string
  vad_path: string
  runtime_ready: boolean
  encoder_ready: boolean
  llm_ready: boolean
  vad_ready: boolean
  ready: boolean
  is_custom: boolean
  model_version: string
  revision: string
  quantization: string
  installed_bytes: number
  expected_bytes: number
  verified: boolean
  update_available: boolean
}

export interface FunAsrDownloadProgress {
  current: number
  total: number
  percent: number
  status: string
  message: string
  file_name: string
  file_current: number
  file_total: number
  speed_bytes_per_sec: number
  average_speed_bytes_per_sec: number
  eta_seconds: number | null
  attempt: number
  source: string
}

export interface FunAsrRuntimeStatus {
  running: boolean
  ready: boolean
  pid: number | null
  runtime_variant: string
  last_error: string
}

export async function getFunAsrPaths(customDir?: string): Promise<FunAsrPaths> {
  return invoke('get_funasr_paths', { customDir: customDir || null })
}

export async function getFunAsrRuntimeStatus(): Promise<FunAsrRuntimeStatus> {
  return invoke('get_funasr_runtime_status')
}

export async function downloadFunAsr(customDir?: string, force = false): Promise<void> {
  return invoke('download_funasr', { customDir: customDir || null, force })
}

export async function cancelFunAsrDownload(): Promise<void> {
  return invoke('cancel_funasr_download')
}

export async function verifyFunAsr(customDir?: string): Promise<void> {
  return invoke('verify_funasr', { customDir: customDir || null })
}

export async function removeFunAsr(customDir?: string): Promise<number> {
  return invoke('remove_funasr', { customDir: customDir || null })
}

export async function restartFunAsrRuntime(): Promise<void> {
  return invoke('restart_funasr_runtime')
}

export async function openFunAsrModelDirectory(customDir?: string): Promise<void> {
  return invoke('open_funasr_model_directory', { customDir: customDir || null })
}

export async function downloadLocalModel(filename: string, customDir?: string): Promise<string> {
  return invoke('download_local_model', { filename, customDir: customDir || null })
}

export interface LocalLlmPaths {
  server_path: string
  model_dir: string
  default_model_path: string
  default_model_ready: boolean
  upgrade_model_path: string
  upgrade_model_ready: boolean
}

export async function getLocalLlmPaths(customDir?: string): Promise<LocalLlmPaths> {
  return invoke('get_local_llm_paths', { customDir: customDir || null })
}

export async function downloadLocalLlm(customDir?: string): Promise<void> {
  return invoke('download_local_llm', { customDir: customDir || null })
}

export async function cancelLocalLlmDownload(): Promise<void> {
  return invoke('cancel_local_llm_download')
}

export async function startLocalLlm(): Promise<void> {
  return invoke('start_local_llm')
}

export async function stopLocalLlm(): Promise<void> {
  return invoke('stop_local_llm')
}

export async function localLlmHealth(): Promise<boolean> {
  return invoke('local_llm_health')
}

// Editor overlay
export async function updateHistoryEntry(id: number, polishedText: string): Promise<void> {
  return invoke('update_history_entry', { id, polishedText })
}

export async function hideEditorWindow(): Promise<void> {
  return invoke('hide_editor_window')
}

// SenseVoice model management
export interface SenseVoicePaths {
  model_dir: string
  display_dir: string
  source: 'package-relative' | 'custom' | 'app-data' | 'development-resource'
  model_path: string
  tokens_path: string
  ready: boolean
  is_custom: boolean
}

export async function getSenseVoicePaths(customDir?: string): Promise<SenseVoicePaths> {
  return invoke('get_sensevoice_paths', { customDir: customDir || null })
}

export async function openSenseVoiceModelDirectory(customDir?: string): Promise<void> {
  return invoke('open_sensevoice_model_directory', { customDir: customDir || null })
}

export async function downloadSenseVoice(customDir?: string, force = false): Promise<void> {
  return invoke('download_sensevoice', { customDir: customDir || null, force })
}
