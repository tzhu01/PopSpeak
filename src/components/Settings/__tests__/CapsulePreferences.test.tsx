import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore, type AppConfig } from '../../../stores/appStore'
import { patchCapsulePreferences, updateConfig } from '../../../lib/tauri'
import { GeneralPane } from '../GeneralPane'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))
vi.mock('../../../lib/tauri', () => ({
  patchCapsulePreferences: vi.fn(),
  updateConfig: vi.fn(),
  setAutoStart: vi.fn(),
  updateHotkey: vi.fn(),
  pauseHotkey: vi.fn(),
  resumeHotkey: vi.fn(),
  checkAccessibilityPermission: vi.fn(),
  requestAccessibilityPermission: vi.fn(),
  listAudioInputDevices: vi.fn().mockResolvedValue([]),
}))

function toggle(label: string) {
  const button = screen.getByText(label).closest('label')?.querySelector('button')
  expect(button).toBeTruthy()
  return button as HTMLButtonElement
}

beforeEach(() => {
  useAppStore.setState(useAppStore.getInitialState())
  useAppStore.getState().setSavedConfig({ ...useAppStore.getState().config })
  vi.mocked(patchCapsulePreferences)
    .mockReset()
    .mockImplementation(async (patch) => ({
      ...useAppStore.getState().savedConfig!,
      ...patch,
    }))
  vi.mocked(updateConfig).mockReset()
})
afterEach(cleanup)

describe('immediate capsule preferences', () => {
  it('keeps the shared save guard after leaving and reopening general settings', async () => {
    let finish!: (config: AppConfig) => void
    vi.mocked(patchCapsulePreferences).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const first = render(<GeneralPane />)
    fireEvent.click(toggle('settings.enableCapsule'))
    first.unmount()
    render(<GeneralPane />)
    expect(toggle('settings.enableCapsule')).toBeDisabled()
    fireEvent.click(toggle('settings.enableCapsule'))
    expect(patchCapsulePreferences).toHaveBeenCalledTimes(1)
    expect(await useAppStore.getState().saveConfig()).toBe(false)
    await act(async () =>
      finish({ ...useAppStore.getState().savedConfig!, capsule_enabled: false }),
    )
    expect(toggle('settings.enableCapsule')).not.toBeDisabled()
    expect(useAppStore.getState().capsulePreferencesSaving).toBe(false)
  })

  it('persists enabled immediately, without full settings save or activation-dependent fields', async () => {
    const saved = useAppStore.getState().savedConfig!
    useAppStore.setState({
      config: { ...saved, hotkey: 'Alt+/', stt_provider: 'funasr-nano', polish_enabled: true },
    })
    render(<GeneralPane />)
    fireEvent.click(toggle('settings.enableCapsule'))
    await screen.findByText('已保存并立即生效')
    expect(patchCapsulePreferences).toHaveBeenCalledExactlyOnceWith({ capsule_enabled: false })
    expect(updateConfig).not.toHaveBeenCalled()
    expect(useAppStore.getState().savedConfig).toMatchObject({
      capsule_enabled: false,
      hotkey: saved.hotkey,
      stt_provider: saved.stt_provider,
    })
    expect(useAppStore.getState().config).toMatchObject({
      capsule_enabled: false,
      hotkey: 'Alt+/',
      stt_provider: 'funasr-nano',
      polish_enabled: true,
    })
  })

  it.each([
    ['settings.capsuleAlwaysOnTop', { capsule_always_on_top: false }],
    ['settings.hideCapsuleWhenIdle', { capsule_auto_hide: true }],
    ['POP 录音文字预览', { capsule_preview_enabled: false }],
  ] as const)('immediately applies %s independently', async (label, patch) => {
    render(<GeneralPane />)
    fireEvent.click(toggle(label))
    await screen.findByText('已保存并立即生效')
    expect(patchCapsulePreferences).toHaveBeenCalledExactlyOnceWith(patch)
    expect(useAppStore.getState().config).toMatchObject(patch)
    expect(useAppStore.getState().savedConfig).toMatchObject(patch)
  })

  it('keeps the original value on failure and offers a retry', async () => {
    vi.mocked(patchCapsulePreferences).mockRejectedValueOnce(new Error('disk full'))
    render(<GeneralPane />)
    fireEvent.click(toggle('settings.enableCapsule'))
    expect(await screen.findByRole('alert')).toHaveTextContent('原设置保持不变')
    expect(useAppStore.getState().config.capsule_enabled).toBe(true)
    fireEvent.click(toggle('settings.enableCapsule'))
    await screen.findByText('已保存并立即生效')
    expect(useAppStore.getState().config.capsule_enabled).toBe(false)
  })

  it('blocks conflicting full saves while a capsule patch is in flight', async () => {
    let finish!: (config: AppConfig) => void
    vi.mocked(patchCapsulePreferences).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    render(<GeneralPane />)
    fireEvent.click(toggle('settings.enableCapsule'))
    expect(toggle('settings.enableCapsule')).toBeDisabled()
    expect(await useAppStore.getState().saveConfig()).toBe(false)
    expect(updateConfig).not.toHaveBeenCalled()
    await act(async () =>
      finish({ ...useAppStore.getState().savedConfig!, capsule_enabled: false }),
    )
    await waitFor(() => expect(useAppStore.getState().capsulePreferencesSaving).toBe(false))
    expect(toggle('settings.enableCapsule')).not.toBeDisabled()
  })
})
