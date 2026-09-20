import { useState, useCallback, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import type { HotkeyMode, OutputMode } from '../../stores/appStore'
import {
  updateHotkey,
  pauseHotkey,
  resumeHotkey,
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  listAudioInputDevices,
  patchCapsulePreferences,
  type CapsulePreferencesPatch,
} from '../../lib/tauri'
import { SegmentedControl } from './shared/SegmentedControl'
import { Toggle } from './shared/Toggle'
import {
  AppWindow,
  AudioLines,
  Keyboard,
  Mic2,
  Rocket,
  SendHorizontal,
  TimerReset,
  type LucideIcon,
} from 'lucide-react'

// Keys that can be used as hotkeys without a modifier
const STANDALONE_KEYS = new Set([
  'Space',
  'Tab',
  'Enter',
  'Backspace',
  'Escape',
  'Delete',
  'Insert',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Up',
  'Down',
  'Left',
  'Right',
  'F1',
  'F2',
  'F3',
  'F4',
  'F5',
  'F6',
  'F7',
  'F8',
  'F9',
  'F10',
  'F11',
  'F12',
])

function HotkeyRecorder() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const { t } = useTranslation()
  const [recording, setRecording] = useState(false)
  const [pending, setPending] = useState<string | null>(null)
  const [modifierHint, setModifierHint] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const autoConfirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const confirmHotkey = useCallback(
    (hotkey: string) => {
      setRecording(false)
      setError(null)
      setModifierHint(null)
      updateHotkey(hotkey)
        .then(() => {
          updateConfig({ hotkey })
          setPending(null)
        })
        .catch((e) => {
          setError(String(e))
          setPending(null)
          resumeHotkey().catch(() => {})
        })
    },
    [updateConfig],
  )

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()

      // Build modifier prefix
      const parts: string[] = []
      if (e.ctrlKey) parts.push('Ctrl')
      if (e.altKey) parts.push('Alt')
      if (e.shiftKey) parts.push('Shift')
      if (e.metaKey) parts.push('Meta')

      // If only modifier keys are pressed, show hint like "Alt+..."
      if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) {
        setModifierHint(parts.length > 0 ? parts.join('+') + '+...' : null)
        return
      }

      setModifierHint(null)

      const keyMap: Record<string, string> = {
        ' ': 'Space',
        Tab: 'Tab',
        Enter: 'Enter',
        Backspace: 'Backspace',
        Escape: 'Escape',
        Delete: 'Delete',
        Insert: 'Insert',
        Home: 'Home',
        End: 'End',
        PageUp: 'PageUp',
        PageDown: 'PageDown',
        ArrowUp: 'Up',
        ArrowDown: 'Down',
        ArrowLeft: 'Left',
        ArrowRight: 'Right',
      }

      let keyName = keyMap[e.key] || e.key
      if (keyName.length === 1) keyName = keyName.toUpperCase()

      // Letters and digits require at least one modifier to avoid interfering with typing
      if (parts.length === 0 && !STANDALONE_KEYS.has(keyName)) return

      parts.push(keyName)
      const combo = parts.join('+')
      setPending(combo)

      // Auto-confirm after 1.5 seconds
      if (autoConfirmTimer.current) clearTimeout(autoConfirmTimer.current)
      autoConfirmTimer.current = setTimeout(() => {
        confirmHotkey(combo)
      }, 1500)
    },
    [confirmHotkey],
  )

  const handleKeyUp = useCallback(() => {
    setModifierHint(null)
  }, [])

  useEffect(() => {
    if (!recording) return
    window.addEventListener('keydown', handleKeyDown, true)
    window.addEventListener('keyup', handleKeyUp, true)
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
      window.removeEventListener('keyup', handleKeyUp, true)
      if (autoConfirmTimer.current) clearTimeout(autoConfirmTimer.current)
    }
  }, [recording, handleKeyDown, handleKeyUp])

  const handleClick = () => {
    if (recording && pending) {
      // Confirm immediately on click
      if (autoConfirmTimer.current) clearTimeout(autoConfirmTimer.current)
      confirmHotkey(pending)
    } else if (recording) {
      // Cancel recording — re-register the old hotkey
      setRecording(false)
      setPending(null)
      setModifierHint(null)
      if (autoConfirmTimer.current) clearTimeout(autoConfirmTimer.current)
      resumeHotkey().catch(() => {})
    } else {
      // Start recording — unregister global shortcut so webview can capture keys
      pauseHotkey().catch(() => {})
      setRecording(true)
      setPending(null)
      setError(null)
    }
  }

  return (
    <div>
      <button
        onClick={handleClick}
        className={`w-full px-3 py-2.5 rounded-[10px] text-[13px] font-mono text-left border transition-colors cursor-pointer ${
          recording
            ? 'bg-bg-tertiary border-text-secondary text-text-primary ring-2 ring-text-secondary/20'
            : 'bg-bg-secondary border-transparent text-text-primary hover:border-border'
        }`}
      >
        {recording ? pending || modifierHint || t('settings.pressKeyCombination') : config.hotkey}
      </button>
      {recording && pending && (
        <p className="text-[11px] text-text-tertiary mt-1.5">{t('settings.clickToConfirm')}</p>
      )}
      {error && <p className="text-[11px] text-error mt-1.5">{error}</p>}
    </div>
  )
}

export function GeneralPane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const applyCapsulePreferences = useAppStore((s) => s.applyCapsulePreferences)
  const configSaving = useAppStore((s) => s.configSaving)
  const capsulePreferencesSaving = useAppStore((s) => s.capsulePreferencesSaving)
  const [capsuleSaving, setCapsuleSaving] = useState(false)
  const [capsuleMessage, setCapsuleMessage] = useState('')
  const [capsuleError, setCapsuleError] = useState(false)
  const capsuleRequest = useRef(false)
  const { t } = useTranslation()
  const isMac =
    typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0
  const [a11yTrusted, setA11yTrusted] = useState<boolean | null>(null)
  const [inputDevices, setInputDevices] = useState<string[]>([])

  const applyCapsuleChange = async (patch: CapsulePreferencesPatch) => {
    const current = useAppStore.getState()
    if (capsuleRequest.current || current.configSaving || current.capsulePreferencesSaving) return
    capsuleRequest.current = true
    setCapsuleSaving(true)
    useAppStore.setState({ capsulePreferencesSaving: true })
    setCapsuleMessage('正在保存并应用…')
    setCapsuleError(false)
    try {
      const confirmed = await patchCapsulePreferences(patch)
      applyCapsulePreferences(confirmed, patch)
      setCapsuleMessage('已保存并立即生效')
    } catch {
      setCapsuleError(true)
      setCapsuleMessage('未能保存，原设置保持不变。请重试。')
    } finally {
      capsuleRequest.current = false
      setCapsuleSaving(false)
      useAppStore.setState({ capsulePreferencesSaving: false })
    }
  }

  useEffect(() => {
    listAudioInputDevices()
      .then(setInputDevices)
      .catch(() => setInputDevices([]))
  }, [])

  useEffect(() => {
    if (isMac && config.output_mode === 'keyboard') {
      checkAccessibilityPermission().then(setA11yTrusted)
      const onFocus = () => checkAccessibilityPermission().then(setA11yTrusted)
      window.addEventListener('focus', onFocus)
      return () => window.removeEventListener('focus', onFocus)
    }
  }, [isMac, config.output_mode])

  const handleGrantPermission = useCallback(async () => {
    await requestAccessibilityPermission()
    const trusted = await checkAccessibilityPermission()
    setA11yTrusted(trusted)
  }, [])

  return (
    <div className="general-settings-pane space-y-5 pb-2">
      <SettingsGroup
        icon={Keyboard}
        title={t('settings.generalGroupRecording')}
        description={t('settings.generalGroupRecordingDesc')}
      >
        <SettingItem title={t('settings.hotkey')} description={t('settings.hotkeyDesc')}>
          <HotkeyRecorder />
        </SettingItem>

        <SettingItem title={t('settings.hotkeyMode')} description={t('settings.hotkeyModeDesc')}>
          <SegmentedControl
            options={[
              { value: 'hold', label: t('settings.holdToTalk') },
              { value: 'toggle', label: t('settings.toggleOnOff') },
            ]}
            value={config.hotkey_mode}
            onChange={(v) => updateConfig({ hotkey_mode: v as HotkeyMode })}
          />
        </SettingItem>

        <SettingItem
          title={t('settings.maxRecordingDuration', 'Max Recording Duration')}
          description={t('settings.maxRecordingDurationDesc')}
          icon={TimerReset}
        >
          <div className="flex items-center gap-4">
            <input
              type="range"
              min={10}
              max={300}
              step={10}
              value={config.max_recording_seconds}
              onChange={(e) => updateConfig({ max_recording_seconds: Number(e.target.value) })}
              className="flex-1 accent-accent"
            />
            <span className="min-w-[64px] rounded-[9px] bg-bg-secondary px-3 py-1.5 text-center text-[14px] font-semibold tabular-nums text-accent">
              {config.max_recording_seconds}s
            </span>
          </div>
        </SettingItem>
      </SettingsGroup>

      <SettingsGroup
        icon={SendHorizontal}
        title={t('settings.generalGroupOutput')}
        description={t('settings.generalGroupOutputDesc')}
      >
        <SettingItem title={t('settings.outputMode')} description={t('settings.outputModeDesc')}>
          <SegmentedControl
            options={[
              { value: 'keyboard', label: t('settings.keyboardSimulation') },
              { value: 'clipboard', label: t('settings.clipboardPaste') },
              { value: 'editor', label: t('settings.editorOverlay') },
            ]}
            value={config.output_mode}
            onChange={(v) => updateConfig({ output_mode: v as OutputMode })}
          />
        </SettingItem>

        {config.output_mode === 'editor' && (
          <SettingItem
            title={t('settings.editorOptions')}
            description={t('settings.editorOverlayHint')}
            icon={AppWindow}
          >
            <div className="space-y-4">
              <Toggle
                checked={config.editor_auto_hide_enabled}
                onChange={(checked) => updateConfig({ editor_auto_hide_enabled: checked })}
                label={t('settings.editorAutoHide')}
              />
              {config.editor_auto_hide_enabled && (
                <div className="rounded-[12px] bg-bg-secondary px-3.5 py-3">
                  <div className="mb-2 flex items-center justify-between text-[12px] font-medium text-text-secondary">
                    <span>{t('settings.editorAutoHideSeconds')}</span>
                    <span className="rounded-full bg-bg-elevated px-2.5 py-1 tabular-nums text-accent">
                      {config.editor_auto_hide_seconds}s
                    </span>
                  </div>
                  <input
                    type="range"
                    min={3}
                    max={60}
                    step={1}
                    value={config.editor_auto_hide_seconds}
                    onChange={(event) =>
                      updateConfig({ editor_auto_hide_seconds: Number(event.target.value) })
                    }
                    aria-label={t('settings.editorAutoHideSeconds')}
                    className="w-full accent-accent"
                  />
                </div>
              )}
              <p className="text-[11px] leading-5 text-text-tertiary">
                {config.editor_auto_hide_enabled
                  ? t('settings.editorAutoHideHint')
                  : t('settings.editorManualCloseHint')}
              </p>
            </div>
          </SettingItem>
        )}

        {isMac && config.output_mode === 'keyboard' && a11yTrusted !== null && (
          <SettingItem
            title={t('settings.accessibilityPermission')}
            description={t('settings.accessibilityRequired')}
          >
            <div className="flex items-center justify-between gap-4">
              <div className="flex items-center gap-2">
                <span
                  className={`h-2.5 w-2.5 rounded-full ${a11yTrusted ? 'bg-green-500' : 'bg-amber-500'}`}
                />
                <span className="text-[14px] font-medium text-text-primary">
                  {a11yTrusted
                    ? t('settings.accessibilityGranted')
                    : t('settings.accessibilityRequired')}
                </span>
              </div>
              {!a11yTrusted && (
                <button
                  onClick={handleGrantPermission}
                  className="rounded-full border-none bg-accent px-4 py-2 text-[13px] font-semibold text-white transition-colors hover:bg-accent-hover cursor-pointer"
                >
                  {t('settings.grantPermission')}
                </button>
              )}
            </div>
          </SettingItem>
        )}
      </SettingsGroup>

      <SettingsGroup
        icon={AudioLines}
        title={t('settings.generalGroupCapsule')}
        description={t('settings.generalGroupCapsuleDesc')}
      >
        <SettingItem title={t('settings.capsule')} description={t('settings.capsuleHint')}>
          <fieldset
            disabled={capsuleSaving || configSaving || capsulePreferencesSaving}
            className="space-y-4 disabled:opacity-60"
          >
            <Toggle
              checked={config.capsule_enabled}
              onChange={(checked) => void applyCapsuleChange({ capsule_enabled: checked })}
              label={t('settings.enableCapsule')}
            />
            {config.capsule_enabled && (
              <div className="grid gap-4 border-t border-border pt-4 sm:grid-cols-2">
                <Toggle
                  checked={config.capsule_always_on_top}
                  onChange={(checked) =>
                    void applyCapsuleChange({ capsule_always_on_top: checked })
                  }
                  label={t('settings.capsuleAlwaysOnTop')}
                />
                <Toggle
                  checked={config.capsule_auto_hide}
                  onChange={(checked) => void applyCapsuleChange({ capsule_auto_hide: checked })}
                  label={t('settings.hideCapsuleWhenIdle')}
                />
                <Toggle
                  checked={config.capsule_preview_enabled}
                  onChange={(checked) =>
                    void applyCapsuleChange({ capsule_preview_enabled: checked })
                  }
                  label="POP 录音文字预览"
                />
                <p className="text-sm leading-relaxed text-text-muted sm:col-span-2">
                  悬浮球默认位于桌面右下角，可拖动、右键关闭。录音时展开文字预览，可随时收起；SenseVoice
                  使用分段预览，结束后确认完整结果。关闭后可在此处或托盘菜单重新打开。
                </p>
              </div>
            )}
            <p
              role={capsuleError ? 'alert' : 'status'}
              className={`text-[12px] ${capsuleError ? 'text-error' : 'text-text-secondary'}`}
            >
              {capsuleMessage || '此组设置即时保存，无需点击底部“保存并应用”，无需激活。'}
            </p>
          </fieldset>
        </SettingItem>
      </SettingsGroup>

      <SettingsGroup
        icon={Mic2}
        title={t('settings.generalGroupAudio')}
        description={t('settings.generalGroupAudioDesc')}
      >
        <SettingItem
          title={t('settings.microphoneDevice')}
          description={t('settings.microphoneDeviceDesc')}
        >
          <select
            value={config.audio_device_name}
            onChange={(event) => updateConfig({ audio_device_name: event.target.value })}
            className="w-full rounded-[11px] border border-border bg-bg-secondary px-3.5 py-3 text-[14px] font-medium text-text-primary outline-none transition-colors focus:border-border-focus"
          >
            <option value="">{t('settings.systemDefaultMicrophone')}</option>
            {inputDevices.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </SettingItem>

        <SettingItem
          title={t('settings.audioEnhancement')}
          description={t('settings.audioCpuHint')}
        >
          <div className="grid gap-4 sm:grid-cols-2">
            <Toggle
              checked={config.vad_enabled}
              onChange={(checked) => updateConfig({ vad_enabled: checked })}
              label={t('settings.vad')}
            />
            <Toggle
              checked={config.noise_suppression_enabled}
              onChange={(checked) => updateConfig({ noise_suppression_enabled: checked })}
              label={t('settings.noiseSuppression')}
            />
          </div>
        </SettingItem>
      </SettingsGroup>

      <SettingsGroup
        icon={Rocket}
        title={t('settings.generalGroupStartup')}
        description={t('settings.generalGroupStartupDesc')}
      >
        <SettingItem
          title={t('settings.startupBehavior')}
          description={t('settings.startupBehaviorDesc')}
        >
          <div className="grid gap-4 sm:grid-cols-2">
            <Toggle
              checked={config.auto_start}
              onChange={(checked) => updateConfig({ auto_start: checked })}
              label={t('settings.launchAtStartup')}
            />
            {config.auto_start && (
              <Toggle
                checked={config.start_minimized}
                onChange={(checked) => updateConfig({ start_minimized: checked })}
                label={t('settings.startMinimized')}
              />
            )}
          </div>
        </SettingItem>
      </SettingsGroup>
    </div>
  )
}

function SettingsGroup({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon
  title: string
  description: string
  children: React.ReactNode
}) {
  return (
    <section
      data-testid="general-settings-group"
      className="overflow-hidden rounded-[18px] border border-border bg-bg-elevated/60 shadow-sm"
    >
      <header className="flex items-center gap-3.5 border-b border-border bg-bg-secondary/75 px-5 py-4">
        <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-[13px] border border-accent/20 bg-accent/10 text-accent shadow-sm">
          <Icon size={21} strokeWidth={2} />
        </span>
        <div className="min-w-0">
          <h3 className="text-[17px] font-semibold leading-6 tracking-[0.01em] text-text-primary">
            {title}
          </h3>
          <p className="mt-0.5 text-[12px] leading-5 text-text-secondary">{description}</p>
        </div>
      </header>
      <div className="space-y-3.5 p-4">{children}</div>
    </section>
  )
}

function SettingItem({
  title,
  description,
  icon: Icon,
  children,
}: {
  title: string
  description: string
  icon?: LucideIcon
  children: React.ReactNode
}) {
  return (
    <div className="rounded-[14px] border border-border bg-bg-elevated px-4 py-3.5">
      <div className="mb-3 flex items-start gap-2.5">
        {Icon && (
          <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-[8px] bg-bg-secondary text-accent">
            <Icon size={15} />
          </span>
        )}
        <div>
          <h4 className="text-[14px] font-semibold leading-5 text-text-primary">{title}</h4>
          <p className="mt-0.5 text-[11px] leading-[18px] text-text-tertiary">{description}</p>
        </div>
      </div>
      {children}
    </div>
  )
}
