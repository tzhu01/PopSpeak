import { describe, it, expect, beforeEach, vi } from 'vitest'
import { useAppStore } from '../appStore'
import type { AppConfig, HistoryEntry, DictionaryEntry } from '../appStore'
import { setAutoStart, updateConfig } from '../../lib/tauri'

vi.mock('../../lib/tauri', () => ({
  updateConfig: vi.fn(async (config: AppConfig) => ({ ...config })),
  setAutoStart: vi.fn().mockResolvedValue(undefined),
}))

function getState() {
  return useAppStore.getState()
}

describe('appStore', () => {
  beforeEach(() => {
    // Reset store to initial state
    useAppStore.setState(useAppStore.getInitialState())
    vi.clearAllMocks()
  })

  describe('pipeline state', () => {
    it('defaults to idle', () => {
      expect(getState().pipelineState).toBe('idle')
    })

    it('updates pipeline state', () => {
      getState().setPipelineState('recording')
      expect(getState().pipelineState).toBe('recording')
    })
  })

  describe('config', () => {
    it('has sensible defaults', () => {
      const { config } = getState()
      expect(config.theme).toBe('system')
      expect(config.hotkey).toBe('Ctrl+/')
      expect(config.output_mode).toBe('clipboard')
      expect(config.editor_auto_hide_enabled).toBe(true)
      expect(config.editor_auto_hide_seconds).toBe(5)
      expect(config.polish_enabled).toBe(false)
      expect(config.sensevoice_use_custom_dir).toBe(false)
      expect(config.sensevoice_model_dir).toBe('')
    })

    it('setConfig replaces entire config', () => {
      const newConfig = { ...getState().config, theme: 'dark' as const }
      getState().setConfig(newConfig)
      expect(getState().config.theme).toBe('dark')
    })

    it('updateConfig merges partial config immutably', () => {
      const original = getState().config
      getState().updateConfig({ theme: 'dark' })
      const updated = getState().config

      expect(updated.theme).toBe('dark')
      expect(updated.hotkey).toBe('Ctrl+/') // unchanged
      expect(updated).not.toBe(original) // new object
    })
  })

  describe('history', () => {
    it('defaults to empty array', () => {
      expect(getState().history).toEqual([])
    })

    it('setHistory replaces history', () => {
      const entries: HistoryEntry[] = [
        {
          id: 1,
          created_at: '2025-01-01',
          app_name: 'Test',
          app_type: 'browser',
          raw_text: 'hello',
          polished_text: 'Hello.',
          language: 'en',
          duration_ms: 1200,
        },
      ]
      getState().setHistory(entries)
      expect(getState().history).toHaveLength(1)
      expect(getState().history[0].raw_text).toBe('hello')
    })
  })

  describe('dictionary', () => {
    it('defaults to empty array', () => {
      expect(getState().dictionary).toEqual([])
    })

    it('setDictionary replaces dictionary', () => {
      const entries: DictionaryEntry[] = [{ id: 1, word: 'API', pronunciation: null }]
      getState().setDictionary(entries)
      expect(getState().dictionary).toHaveLength(1)
      expect(getState().dictionary[0].word).toBe('API')
    })
  })

  describe('recording state', () => {
    it('resetRecording clears all recording fields', () => {
      useAppStore.setState({
        activeSessionId: 17,
        lastPipelineStateRevision: 4,
        lastPipelineMessageRevision: 3,
      })
      getState().setAudioVolume(0.8)
      getState().setPartialTranscript('partial')
      getState().setFinalTranscript('final')
      getState().setPolishedText('polished')
      getState().setRecordingDuration(5000)

      getState().resetRecording()

      expect(getState().audioVolume).toBe(0)
      expect(getState().partialTranscript).toBe('')
      expect(getState().finalTranscript).toBe('')
      expect(getState().polishedText).toBe('')
      expect(getState().recordingDuration).toBe(0)
      expect(getState().activeSessionId).toBe(17)
      expect(getState().lastPipelineStateRevision).toBe(4)
      expect(getState().lastPipelineMessageRevision).toBe(3)
    })

    it('appendPolishedChunk appends to existing text', () => {
      getState().setPolishedText('Hello')
      getState().appendPolishedChunk(' world')
      expect(getState().polishedText).toBe('Hello world')
    })
  })

  describe('savedConfig / resetConfig', () => {
    it('uses normalized backend values as the saved and displayed configuration', async () => {
      const initial = { ...getState().config }
      getState().setSavedConfig(initial)
      getState().updateConfig({
        stt_provider: 'funasr-nano',
        funasr_num_threads: 99,
        funasr_model_dir: 'obsolete-default-path',
      })
      const confirmed = { ...getState().config, funasr_num_threads: 16, funasr_model_dir: '' }
      vi.mocked(updateConfig).mockResolvedValueOnce(confirmed)

      expect(await getState().saveConfig()).toBe(true)
      expect(getState().savedConfig).toEqual(confirmed)
      expect(getState().config).toEqual(confirmed)
      expect(getState().configSaveError).toBeNull()
    })

    it('keeps only edits made after submission when a normalized event precedes the save response', async () => {
      const initial = { ...getState().config }
      getState().setSavedConfig(initial)
      getState().updateConfig({
        stt_provider: 'funasr-nano',
        funasr_num_threads: 99,
        funasr_model_dir: 'obsolete-default-path',
      })
      vi.mocked(updateConfig).mockImplementationOnce(async (submitted) => {
        const confirmed = { ...submitted, funasr_num_threads: 16, funasr_model_dir: '' }
        getState().applyBackendConfig(confirmed)
        getState().updateConfig({ funasr_num_threads: 8, theme: 'dark' })
        return confirmed
      })

      expect(await getState().saveConfig()).toBe(true)
      expect(getState().savedConfig?.funasr_num_threads).toBe(16)
      expect(getState().savedConfig?.theme).toBe('system')
      expect(getState().config.funasr_model_dir).toBe('')
      expect(getState().config.funasr_num_threads).toBe(8)
      expect(getState().config.theme).toBe('dark')
      expect(getState().config.stt_provider).toBe('funasr-nano')
    })

    it('retries system startup synchronization without denying an already saved engine change', async () => {
      getState().setSavedConfig({ ...getState().config })
      getState().updateConfig({ stt_provider: 'funasr-nano', auto_start: true })
      vi.mocked(setAutoStart).mockRejectedValueOnce(new Error('permission denied'))

      expect(await getState().saveConfig()).toBe(false)
      expect(getState().savedConfig?.stt_provider).toBe('funasr-nano')
      expect(getState().configSaveError).toContain('设置已保存，但开机启动同步失败')
      expect(getState().autoStartSyncPending).toBe(true)

      expect(await getState().saveConfig()).toBe(true)
      expect(setAutoStart).toHaveBeenCalledTimes(2)
      expect(updateConfig).toHaveBeenCalledTimes(2)
      expect(getState().autoStartSyncPending).toBe(false)
      expect(getState().configSaveError).toBeNull()
    })

    it('backend confirmation updates the active mode while retaining other unsaved draft fields', () => {
      const initial = { ...getState().config }
      getState().setSavedConfig(initial)
      getState().updateConfig({ stt_provider: 'funasr-nano', theme: 'dark' })

      getState().applyBackendConfig({ ...initial, stt_provider: 'funasr-nano' })

      expect(getState().savedConfig?.stt_provider).toBe('funasr-nano')
      expect(getState().savedConfig?.theme).toBe('system')
      expect(getState().config.theme).toBe('dark')
      getState().resetConfig()
      expect(getState().config.stt_provider).toBe('funasr-nano')
      expect(getState().config.theme).toBe('system')
    })

    it('syncs a backend engine change when there is no local draft', () => {
      const initial = { ...getState().config }
      getState().setSavedConfig(initial)
      getState().applyBackendConfig({ ...initial, stt_provider: 'local-whisper' })
      expect(getState().config.stt_provider).toBe('local-whisper')
      expect(getState().savedConfig?.stt_provider).toBe('local-whisper')
    })

    it('resetConfig restores to savedConfig', () => {
      const saved = { ...getState().config }
      getState().setSavedConfig(saved)

      getState().updateConfig({ theme: 'dark', polish_enabled: false })
      expect(getState().config.theme).toBe('dark')

      getState().resetConfig()
      expect(getState().config.theme).toBe('system')
      expect(getState().config.polish_enabled).toBe(false)
    })

    it('resetConfig is a no-op when savedConfig is null', () => {
      getState().updateConfig({ theme: 'dark' })
      getState().resetConfig()
      // Should remain dark since savedConfig is null
      expect(getState().config.theme).toBe('dark')
    })
  })

  describe('onboarding', () => {
    it('defaults to not completed', () => {
      expect(getState().onboardingCompleted).toBe(false)
      expect(getState().onboardingStep).toBe(0)
    })

    it('tracks onboarding progress', () => {
      getState().setOnboardingStep(2)
      getState().setOnboardingCompleted(true)
      expect(getState().onboardingStep).toBe(2)
      expect(getState().onboardingCompleted).toBe(true)
    })
  })
})
