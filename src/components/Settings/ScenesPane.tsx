import { useMemo, useState } from 'react'
import { BookOpen, Check, Keyboard, RotateCcw, Sparkles } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useActivationStore } from '../../lib/activation'
import { addDictionaryEntry, getDictionary } from '../../lib/tauri'
import { useAppStore, type AppConfig } from '../../stores/appStore'

interface LocalScene {
  id: string
  nameKey: string
  descriptionKey: string
  icon: typeof Keyboard
  settings: Partial<AppConfig>
  terms: string[]
}

const DEFAULT_SCENE_ID = 'daily'

const LOCAL_SCENES: LocalScene[] = [
  {
    id: DEFAULT_SCENE_ID,
    nameKey: 'scenes.dailyName',
    descriptionKey: 'scenes.dailyDescription',
    icon: Keyboard,
    settings: {
      output_mode: 'clipboard',
      polish_enabled: false,
      translate_enabled: false,
      selected_text_enabled: false,
    },
    terms: [],
  },
  {
    id: 'chat',
    nameKey: 'scenes.chatName',
    descriptionKey: 'scenes.chatDescription',
    icon: Sparkles,
    settings: {
      output_mode: 'keyboard',
      polish_enabled: true,
      polish_mode: 'fast',
      translate_enabled: false,
      selected_text_enabled: false,
    },
    terms: [],
  },
  {
    id: 'meeting',
    nameKey: 'scenes.meetingName',
    descriptionKey: 'scenes.meetingDescription',
    icon: BookOpen,
    settings: {
      output_mode: 'editor',
      polish_enabled: true,
      polish_mode: 'deep',
      translate_enabled: false,
      selected_text_enabled: false,
    },
    terms: ['会议纪要', '议程', '参会人', '行动项', '待办事项'],
  },
]

function sceneMatches(config: AppConfig, scene: LocalScene) {
  return Object.entries(scene.settings).every(
    ([key, value]) => config[key as keyof AppConfig] === value,
  )
}

export function ScenesPane() {
  const { t } = useTranslation()
  const activated = useActivationStore((state) => state.status?.activated === true && !state.error)
  const config = useAppStore((state) => state.config)
  const savedConfig = useAppStore((state) => state.savedConfig)
  const updateConfig = useAppStore((state) => state.updateConfig)
  const saveConfig = useAppStore((state) => state.saveConfig)
  const setDictionary = useAppStore((state) => state.setDictionary)
  const [applyingId, setApplyingId] = useState<string | null>(null)
  const [message, setMessage] = useState<{ kind: 'success' | 'error'; text: string } | null>(null)

  const effectiveConfig = savedConfig ?? config
  const activeScene = useMemo(
    () => LOCAL_SCENES.find((scene) => sceneMatches(effectiveConfig, scene)) ?? null,
    [effectiveConfig],
  )

  const applyScene = async (scene: LocalScene) => {
    if (applyingId) return
    if (!activated && (scene.terms.length > 0 || scene.settings.polish_enabled)) {
      setMessage({ kind: 'error', text: t('scenes.activationRequired') })
      return
    }

    setApplyingId(scene.id)
    setMessage(null)
    try {
      if (scene.terms.length > 0) {
        const current = await getDictionary()
        const existing = new Set(current.map((entry) => entry.word.trim().toLocaleLowerCase()))
        for (const term of scene.terms) {
          if (!existing.has(term.toLocaleLowerCase())) {
            await addDictionaryEntry(term, null, null)
            existing.add(term.toLocaleLowerCase())
          }
        }
        setDictionary(await getDictionary())
      }

      updateConfig(scene.settings)
      const saved = await saveConfig()
      if (!saved) throw new Error('settings were not saved')
      setMessage({
        kind: 'success',
        text: t('scenes.applied', { name: t(scene.nameKey) }),
      })
    } catch (error) {
      console.error('Failed to apply local scene:', error)
      setMessage({ kind: 'error', text: t('scenes.failedToApply') })
    } finally {
      setApplyingId(null)
    }
  }

  const defaultScene = LOCAL_SCENES.find((scene) => scene.id === DEFAULT_SCENE_ID)!

  return (
    <div className="space-y-5">
      <section className="rounded-[14px] border border-accent/25 bg-accent/5 p-4">
        <h3 className="text-[18px] font-semibold text-text-primary">{t('scenes.howItWorks')}</h3>
        <p className="mt-2 max-w-[760px] text-[13px] leading-relaxed text-text-secondary">
          {t('scenes.explanation')}
        </p>
        <div className="mt-3 flex flex-wrap items-center gap-2 text-[12px]">
          <span className="rounded-full bg-bg-elevated px-3 py-1.5 text-text-secondary">
            {t('scenes.currentScene')}：
            <strong className="ml-1 text-text-primary">
              {activeScene ? t(activeScene.nameKey) : t('scenes.customScene')}
            </strong>
          </span>
          {activeScene?.id !== DEFAULT_SCENE_ID && (
            <button
              type="button"
              onClick={() => void applyScene(defaultScene)}
              disabled={applyingId !== null}
              className="flex items-center gap-1.5 rounded-[9px] border border-border bg-bg-elevated px-3 py-1.5 text-text-secondary hover:border-accent/40 hover:text-accent disabled:opacity-50"
            >
              <RotateCcw size={13} />
              {t('scenes.restoreDefault')}
            </button>
          )}
        </div>
      </section>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-3">
        {LOCAL_SCENES.map((scene) => {
          const Icon = scene.icon
          const isActive = activeScene?.id === scene.id
          const needsActivation =
            !activated && (scene.terms.length > 0 || scene.settings.polish_enabled === true)
          return (
            <article
              key={scene.id}
              className={`rounded-[14px] border p-4 ${
                isActive ? 'border-accent bg-accent/5' : 'border-border bg-bg-elevated'
              }`}
            >
              <div className="flex items-start gap-3">
                <span className="rounded-[10px] bg-bg-secondary p-2 text-accent">
                  <Icon size={18} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <h4 className="text-[15px] font-semibold text-text-primary">
                      {t(scene.nameKey)}
                    </h4>
                    {isActive && (
                      <span className="flex items-center gap-1 rounded-full bg-success/10 px-2 py-0.5 text-[11px] text-success">
                        <Check size={11} /> {t('scenes.inUse')}
                      </span>
                    )}
                  </div>
                  <p className="mt-1.5 text-[12px] leading-relaxed text-text-secondary">
                    {t(scene.descriptionKey)}
                  </p>
                </div>
              </div>

              <ul className="mt-4 space-y-1.5 text-[12px] text-text-secondary">
                <li>• {t(`scenes.output.${scene.settings.output_mode}`)}</li>
                <li>
                  •{' '}
                  {scene.settings.polish_enabled
                    ? t(`scenes.polish.${scene.settings.polish_mode}`)
                    : t('scenes.polish.off')}
                </li>
                <li>
                  •{' '}
                  {scene.terms.length > 0
                    ? t('scenes.addHotwords', { count: scene.terms.length })
                    : t('scenes.keepDictionary')}
                </li>
              </ul>

              {scene.terms.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {scene.terms.map((term) => (
                    <span
                      key={term}
                      className="rounded-full border border-border bg-bg-secondary px-2 py-0.5 text-[11px] text-text-secondary"
                    >
                      {term}
                    </span>
                  ))}
                </div>
              )}

              <button
                type="button"
                onClick={() => void applyScene(scene)}
                disabled={isActive || applyingId !== null || needsActivation}
                className="mt-4 w-full rounded-[9px] bg-accent px-3 py-2 text-[13px] font-medium text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-45"
              >
                {needsActivation
                  ? t('scenes.requiresActivation')
                  : applyingId === scene.id
                    ? t('scenes.applying')
                    : isActive
                      ? t('scenes.inUse')
                      : t('scenes.apply')}
              </button>
            </article>
          )
        })}
      </div>

      <p className="text-[12px] leading-relaxed text-text-tertiary">
        {t('scenes.safeRestoreHint')}
      </p>
      {message && (
        <p
          role="status"
          className={`rounded-[9px] px-3 py-2 text-[12px] ${
            message.kind === 'success' ? 'bg-success/10 text-success' : 'bg-error/10 text-error'
          }`}
        >
          {message.text}
        </p>
      )}
    </div>
  )
}
