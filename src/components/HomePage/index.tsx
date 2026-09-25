import {
  Mic,
  Settings,
  History,
  KeyRound,
  Cpu,
  ShieldCheck,
  ArrowUpRight,
  ArrowRight,
  ScanText,
} from 'lucide-react'
import { motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { spring } from '../../lib/animations'
import { useAppStore } from '../../stores/appStore'
import { useActivationStore } from '../../lib/activation'
import { ActivationNotice } from '../ActivationNotice'
import { RewardsCard } from '../RewardsCard'
import { useRoute } from '../../lib/router'
import { LLM_PROVIDERS, STT_PROVIDERS } from '../../lib/constants'

export function HomePage() {
  // The dashboard describes the native recorder, not an unsubmitted settings draft.
  const config = useAppStore((s) => s.savedConfig ?? s.config)
  const history = useAppStore((s) => s.history)
  const { navigate } = useRoute()
  const activation = useActivationStore((s) => s.status)
  const { t } = useTranslation()

  const now = new Date()
  const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  const todayCount = history.filter((h) => h.created_at.startsWith(today)).length

  return (
    <div className="home-page mx-auto w-full max-w-[1120px] space-y-5 px-8 py-8">
      {/* Welcome */}
      <div className="brand-hero jelly-card rounded-[18px] px-7 py-8">
        <div className="brand-kicker mb-4">POP SPEAK · VOICE INPUT</div>
        <h2 className="text-[28px] font-semibold leading-tight tracking-[-0.035em] text-text-primary">
          开口说话，文字即刻到位
        </h2>
        <p className="mt-3 max-w-[650px] text-[13px] leading-6 text-text-secondary">
          {t(config.hotkey_mode === 'toggle' ? 'home.descriptionToggle' : 'home.description', {
            hotkey: config.hotkey,
          })}
        </p>
        <div className="mt-6 flex flex-wrap items-center gap-3">
          <span className="home-hotkey inline-flex items-center gap-2 rounded-[9px] px-3 py-2 text-[13px] font-semibold">
            <Mic size={14} /> {config.hotkey}
          </span>
          <span className="inline-flex items-center gap-1.5 text-[11px] text-text-secondary">
            <Cpu size={13} /> 本地 CPU 可用
          </span>
          <span className="inline-flex items-center gap-1.5 text-[11px] text-text-secondary">
            <ShieldCheck size={13} /> 离线模式不上传录音
          </span>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        {[
          ['01', '按下快捷键', '在任意输入框中启动录音'],
          ['02', '自然说话', '悬浮球显示录音和处理状态'],
          ['03', '直接输入', '结果进入光标或编辑浮层'],
        ].map(([number, title, description]) => (
          <div key={number} className="home-step rounded-[14px] border border-border p-4">
            <span className="text-[11px] font-semibold text-accent">{number}</span>
            <p className="mt-2 text-[13px] font-semibold text-text-primary">{title}</p>
            <p className="mt-1 text-[11px] leading-5 text-text-secondary">{description}</p>
          </div>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1.45fr_0.75fr]">
        <div className="space-y-4">
          {/* Current config */}
          <div className="rounded-[18px] p-5 jelly-card">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <p className="brand-kicker">CURRENT SETUP</p>
                <h3 className="mt-1 text-[18px] font-semibold">{t('home.currentConfig')}</h3>
              </div>
              <button
                type="button"
                onClick={() => {
                  window.location.hash = '#/settings/stt'
                }}
                className="inline-flex items-center gap-1 text-[12px] font-medium text-accent hover:underline"
              >
                管理识别模型 <ArrowRight size={13} />
              </button>
            </div>
            <div className="grid grid-cols-2 overflow-hidden rounded-[14px] border border-border bg-bg-primary/55">
              <ConfigItem
                label={t('home.sttProvider')}
                value={
                  config.stt_provider === 'native-asr'
                    ? ({
                        'qwen3-asr-1.7b': 'Qwen3-ASR 1.7B',
                        'cohere-transcribe-03-2026': 'Cohere Transcribe 03-2026',
                        'nemotron-3.5-asr-streaming-0.6b': 'Nemotron 3.5 ASR Streaming 0.6B',
                        'parakeet-unified-en-0.6b': 'Parakeet Unified EN 0.6B',
                      }[config.native_asr.model_id] ?? config.native_asr.model_id)
                    : providerLabel(STT_PROVIDERS, config.stt_provider)
                }
              />
              <ConfigItem
                label={t('home.llmProvider')}
                value={providerLabel(LLM_PROVIDERS, config.llm_provider)}
                borderLeft
              />
              <ConfigItem
                label={t('home.aiPolish')}
                value={
                  config.polish_enabled
                    ? activation?.activated
                      ? t('home.enabled')
                      : '待激活，当前不生效'
                    : t('home.disabled')
                }
                borderTop
              />
              <ConfigItem
                label={t('home.outputMode')}
                value={t(`settings.${outputModeTranslationKey(config.output_mode)}`)}
                borderLeft
                borderTop
              />
            </div>
          </div>

          <ActivationNotice />
          <RewardsCard />
        </div>

        <div className="space-y-4">
          {/* Stats */}
          <div className="grid grid-cols-2 gap-3">
            <StatCard label={t('home.totalRecordings')} value={history.length} />
            <StatCard label={t('home.today')} value={todayCount} accent />
          </div>

          {/* Quick actions */}
          <div className="rounded-[18px] p-4 jelly-card">
            <p className="brand-kicker mb-3">QUICK ACCESS</p>
            <div className="space-y-2">
              <motion.button
                onClick={() => navigate('image')}
                whileHover={{ x: 2 }}
                whileTap={{ scaleX: 1.02, scaleY: 0.97 }}
                transition={spring.jellyGentle}
                className="flex w-full items-center gap-3 rounded-[12px] p-3 cursor-pointer text-left jelly-btn"
              >
                <span className="flex h-8 w-8 items-center justify-center rounded-[9px] bg-accent/10 text-accent">
                  <ScanText size={15} />
                </span>
                <span className="flex-1 text-[13px] font-medium">{t('nav.image')}</span>
                <ArrowUpRight size={14} className="text-text-tertiary" />
              </motion.button>
              <motion.button
                onClick={() => navigate('settings')}
                whileHover={{ x: 2 }}
                whileTap={{ scaleX: 1.02, scaleY: 0.97 }}
                transition={spring.jellyGentle}
                className="flex w-full items-center gap-3 rounded-[12px] p-3 cursor-pointer text-left jelly-btn"
              >
                <span className="flex h-8 w-8 items-center justify-center rounded-[9px] bg-accent/10 text-accent">
                  <Settings size={15} />
                </span>
                <span className="flex-1 text-[13px] font-medium">{t('nav.settings')}</span>
                <ArrowUpRight size={14} className="text-text-tertiary" />
              </motion.button>
              <motion.button
                onClick={() => navigate('account')}
                whileHover={{ x: 2 }}
                transition={spring.jellyGentle}
                className="flex w-full items-center gap-3 rounded-[12px] p-3 cursor-pointer text-left jelly-btn"
              >
                <span className="flex h-8 w-8 items-center justify-center rounded-[9px] bg-accent/10 text-accent">
                  <KeyRound size={15} />
                </span>
                <span className="flex-1 text-[13px] font-medium">
                  {activation?.activated ? '查看本机激活' : '公众号激活'}
                </span>
                <ArrowUpRight size={14} className="text-text-tertiary" />
              </motion.button>
              <motion.button
                onClick={() => navigate('history')}
                whileHover={{ x: 2 }}
                whileTap={{ scaleX: 1.02, scaleY: 0.97 }}
                transition={spring.jellyGentle}
                className="flex w-full items-center gap-3 rounded-[12px] p-3 cursor-pointer text-left jelly-btn"
              >
                <span className="flex h-8 w-8 items-center justify-center rounded-[9px] bg-bg-tertiary/70 text-text-secondary">
                  <History size={15} />
                </span>
                <span className="flex-1 text-[13px] font-medium">{t('nav.history')}</span>
                <ArrowUpRight size={14} className="text-text-tertiary" />
              </motion.button>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

function ConfigItem({
  label,
  value,
  borderLeft = false,
  borderTop = false,
}: {
  label: string
  value: string
  borderLeft?: boolean
  borderTop?: boolean
}) {
  return (
    <div
      className={`min-w-0 p-3.5 ${borderLeft ? 'border-l border-border' : ''} ${borderTop ? 'border-t border-border' : ''}`}
    >
      <p className="text-[13px] uppercase tracking-[0.06em] text-text-tertiary">{label}</p>
      <p className="mt-1 truncate text-[15px] font-medium text-text-primary" title={value}>
        {value}
      </p>
    </div>
  )
}

function providerLabel(providers: readonly { value: string; label: string }[], provider: string) {
  if (provider === 'cloud') return 'PopSpeak 云端服务'
  return providers.find((option) => option.value === provider)?.label ?? '自定义服务'
}

function outputModeTranslationKey(mode: string) {
  if (mode === 'keyboard') return 'keyboardSimulation'
  if (mode === 'editor') return 'editorOverlay'
  return 'clipboardPaste'
}

function StatCard({
  label,
  value,
  accent = false,
}: {
  label: string
  value: number
  accent?: boolean
}) {
  return (
    <div className="rounded-[18px] p-4 jelly-card">
      <div className={`mb-3 h-1 w-8 rounded-full ${accent ? 'bg-accent' : 'bg-text-primary/20'}`} />
      <p className="brand-display text-[27px] font-semibold leading-none">{value}</p>
      <p className="mt-2 text-[10px] uppercase tracking-[0.1em] text-text-tertiary">{label}</p>
    </div>
  )
}
