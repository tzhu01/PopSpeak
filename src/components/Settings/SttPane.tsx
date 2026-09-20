import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { listen } from '@tauri-apps/api/event'
import { open as selectDirectory } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppStore } from '../../stores/appStore'
import { useAuthStore } from '../../stores/authStore'
import { LANGUAGES } from '../../lib/constants'
import {
  benchSttConnection,
  getLocalModelPaths,
  downloadLocalModel,
  getSenseVoicePaths,
  openSenseVoiceModelDirectory,
  downloadSenseVoice,
  getFunAsrPaths,
  getFunAsrRuntimeStatus,
  downloadFunAsr,
  cancelFunAsrDownload,
  verifyFunAsr,
  removeFunAsr,
  restartFunAsrRuntime,
  openFunAsrModelDirectory,
  type LocalModelPaths,
  type SenseVoicePaths,
  type FunAsrPaths,
  type FunAsrDownloadProgress,
  type FunAsrRuntimeStatus,
} from '../../lib/tauri'
import { FormField } from './shared/FormField'
import { CustomCloudSettings } from './CustomCloudSettings'
import { NativeAsrPanel } from './NativeAsrPanel'
import { NATIVE_ASR_IDS, getNativeAsrPaths } from '../../lib/tauri'
import {
  RecognitionModelGallery,
  type RecognitionChoice,
  type ModelAvailability,
} from './RecognitionModelGallery'
import {
  CheckCircle2,
  XCircle,
  Loader2,
  Crown,
  Download,
  HardDrive,
  AlertCircle,
  RotateCcw,
  Pause,
  Trash2,
  RefreshCw,
  ExternalLink,
  ShieldCheck,
} from 'lucide-react'

const WHISPER_VARIANTS = [
  {
    choice: 'whisper-tiny',
    filename: 'ggml-tiny.bin',
    label: 'Whisper Tiny',
    size: 77,
    pathKey: 'default_model_path',
    readyKey: 'default_model_ready',
  },
  {
    choice: 'whisper-base',
    filename: 'ggml-base.bin',
    label: 'Whisper Base',
    size: 148,
    pathKey: 'upgrade_model_path',
    readyKey: 'upgrade_model_ready',
  },
  {
    choice: 'whisper-small',
    filename: 'ggml-small-q5_1.bin',
    label: 'Whisper Small Q5_1',
    size: 190,
    pathKey: 'small_model_path',
    readyKey: 'small_model_ready',
  },
  {
    choice: 'whisper-turbo',
    filename: 'ggml-large-v3-turbo-q5_0.bin',
    label: 'Whisper Large v3 Turbo Q5_0',
    size: 574,
    pathKey: 'turbo_model_path',
    readyKey: 'turbo_model_ready',
  },
] as const

const COHERE_LANGUAGE_CODES = new Set([
  'multi',
  'zh',
  'en',
  'ja',
  'ko',
  'de',
  'fr',
  'es',
  'it',
  'pt',
  'el',
  'nl',
  'pl',
  'vi',
  'ar',
])

function configuredModelDirectory(modelPath: string | undefined): string | undefined {
  if (!modelPath || !/^(?:[a-z]:[\\/]|\/)/i.test(modelPath)) return undefined
  const separator = Math.max(modelPath.lastIndexOf('/'), modelPath.lastIndexOf('\\'))
  return separator > 0 ? modelPath.slice(0, separator) : undefined
}

export function SttPane() {
  const selectionSequence = useRef(0)
  const [sttTestError, setSttTestError] = useState<string | null>(null)
  const [modelSelectionHint, setModelSelectionHint] = useState<string | null>(null)
  const [availability, setAvailability] = useState<ModelAvailability>({})
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const sttTestStatus = useAppStore((s) => s.sttTestStatus)
  const setSttTestStatus = useAppStore((s) => s.setSttTestStatus)
  const sttLatencyMs = useAppStore((s) => s.sttLatencyMs)
  const setSttLatencyMs = useAppStore((s) => s.setSttLatencyMs)
  const { user, plan } = useAuthStore()
  const { t } = useTranslation()

  const senseVoiceDirectory = config.sensevoice_use_custom_dir ? config.sensevoice_model_dir : ''
  const funAsrDirectory = config.funasr_use_custom_dir ? config.funasr_model_dir : ''
  const whisperDirectory = configuredModelDirectory(config.whisper_model_path)
  useEffect(() => {
    let disposed = false
    const refresh = async () => {
      const [senseVoice, funAsr, whisper, ...native] = await Promise.allSettled([
        getSenseVoicePaths(senseVoiceDirectory),
        getFunAsrPaths(funAsrDirectory),
        getLocalModelPaths(whisperDirectory),
        ...NATIVE_ASR_IDS.map((id) => getNativeAsrPaths(id, config.native_asr.model_dir)),
      ])
      if (disposed) return
      const values: ModelAvailability = {}
      native.forEach((result, index) => {
        if (result.status === 'fulfilled' && result.value)
          values[NATIVE_ASR_IDS[index]] = (result.value as { ready: boolean }).ready
      })
      if (senseVoice.status === 'fulfilled' && senseVoice.value)
        values.sensevoice = senseVoice.value.ready
      if (funAsr.status === 'fulfilled' && funAsr.value) values['funasr-nano'] = funAsr.value.ready
      if (whisper.status === 'fulfilled' && whisper.value) {
        for (const variant of WHISPER_VARIANTS)
          values[variant.choice] = whisper.value[variant.readyKey]
      }
      setAvailability(values)
    }
    void refresh()
    const subscriptions = [
      listen<{ phase?: string }>('model:progress', ({ payload }) => {
        if (payload.phase === 'done' || payload.phase === 'ready') void refresh()
      }),
      listen<{ status?: string }>('funasr:download_progress', ({ payload }) => {
        if (payload.status === 'done' || payload.status === 'ready') void refresh()
      }),
      listen<{ status?: string }>('sensevoice:download_progress', ({ payload }) => {
        if (payload.status === 'done' || payload.status === 'ready') void refresh()
      }),
      listen<{ status?: string }>('native-asr:download-progress', ({ payload }) => {
        if (
          payload.status === 'done' ||
          payload.status === 'completed' ||
          payload.status === 'ready' ||
          payload.status === 'deleted'
        )
          void refresh()
      }),
    ]
    return () => {
      disposed = true
      for (const subscription of subscriptions)
        void subscription.then((unlisten) => unlisten()).catch(() => {})
    }
  }, [senseVoiceDirectory, funAsrDirectory, whisperDirectory, config.native_asr.model_dir])

  const isCloud = config.stt_provider === 'cloud'
  const isCustomWhisper = config.stt_provider === 'custom-whisper'
  const isVolcSeedAsr = config.stt_provider === 'volcengine-seedasr'
  const activeSttCredential = isVolcSeedAsr ? config.volcengine_credential : config.stt_api_key
  const isLocalProvider = ['sensevoice', 'local-whisper', 'funasr-nano', 'native-asr'].includes(
    config.stt_provider,
  )
  const selectedModel: RecognitionChoice =
    config.stt_provider === 'native-asr'
      ? (config.native_asr.model_id as RecognitionChoice)
      : config.stt_provider === 'local-whisper'
        ? (WHISPER_VARIANTS.find((variant) =>
            config.whisper_model_path?.toLowerCase().endsWith(variant.filename),
          )?.choice ?? 'whisper-tiny')
        : (config.stt_provider as RecognitionChoice)

  const selectProvider = (provider: typeof config.stt_provider) => {
    selectionSequence.current += 1
    // Choosing the current card must not erase an already configured cloud credential.
    if (provider === config.stt_provider) {
      setModelSelectionHint(null)
      return
    }
    updateConfig(
      provider === 'volcengine-seedasr'
        ? {
            stt_provider: provider,
            stt_api_key: '',
            stt_model: 'volc.seedasr.sauc.duration',
            stt_base_url: 'wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async',
            volcengine_auth_mode: 'app-token',
          }
        : { stt_provider: provider, stt_api_key: '' },
    )
    setSttTestStatus('idle')
    setSttLatencyMs(null)
    setSttTestError(null)
    setModelSelectionHint(null)
  }

  const selectModel = async (choice: RecognitionChoice) => {
    if (NATIVE_ASR_IDS.some((id) => id === choice)) {
      selectProvider('native-asr')
      updateConfig({ native_asr: { ...config.native_asr, model_id: choice } })
      if (choice === 'cohere-transcribe-03-2026' && !COHERE_LANGUAGE_CODES.has(config.stt_language))
        updateConfig({ stt_language: 'zh' })
      if (choice === 'parakeet-unified-en-0.6b') updateConfig({ stt_language: 'en' })
      setModelSelectionHint(
        availability[choice] ? null : '此模型需按需下载。下载并保存设置后，下一段录音生效。',
      )
      return
    }
    const variant = WHISPER_VARIANTS.find((item) => item.choice === choice)
    if (variant) {
      const modelDirectory = configuredModelDirectory(config.whisper_model_path)
      selectProvider('local-whisper')
      const selection = selectionSequence.current
      updateConfig({ whisper_model_path: variant.filename })
      try {
        const paths = await getLocalModelPaths(modelDirectory)
        if (selection !== selectionSequence.current) return
        updateConfig({ whisper_model_path: paths[variant.pathKey] })
        if (paths[variant.readyKey]) {
          setModelSelectionHint(null)
        } else {
          setModelSelectionHint(`${variant.label}尚未安装，请在下方下载后保存设置。`)
        }
      } catch (reason) {
        if (selection !== selectionSequence.current) return
        setModelSelectionHint(`模型目录检查失败：${String(reason)}。请在下方重试。`)
      }
      return
    }
    if (
      choice === 'sensevoice' ||
      choice === 'funasr-nano' ||
      choice === 'volcengine-seedasr' ||
      choice === 'custom-whisper'
    ) {
      selectProvider(choice)
    }
  }

  const handleTest = async () => {
    setSttTestStatus('testing')
    setSttLatencyMs(null)
    setSttTestError(null)
    try {
      const ms = await benchSttConnection(
        activeSttCredential,
        config.stt_provider,
        config.stt_base_url,
        config.stt_model,
        config.volcengine_app_id,
        config.volcengine_auth_mode,
      )
      setSttLatencyMs(ms)
      setSttTestStatus('success')
    } catch (reason) {
      const message = String(reason).replace(/^Error:\s*/i, '')
      setSttTestError(
        message.includes('requested resource not granted')
          ? '此 API Key 尚未获得该语音资源授权；请先在控制台开通资源，或使用拥有该资源的 APP ID + Access Token。'
          : message,
      )
      setSttTestStatus('error')
    }
  }

  return (
    <div className="space-y-5">
      <RecognitionModelGallery
        selected={selectedModel}
        onChoose={(choice) => void selectModel(choice)}
        availability={availability}
      />
      {modelSelectionHint && (
        <p
          role="status"
          className="rounded-[9px] border border-border bg-bg-secondary px-3 py-2 text-[12px] text-text-secondary"
        >
          {modelSelectionHint}
        </p>
      )}

      <p className="text-[12px] leading-6 text-text-secondary">
        {config.stt_provider === 'sensevoice'
          ? '开箱即可离线试用：累计 200 次或累计 20 分钟，任一额度用完后需激活；激活后解除试用限制。'
          : config.stt_provider === 'funasr-nano'
            ? '激活后可用；模型未安装时按需下载，支持识别阶段热词。'
            : config.stt_provider === 'local-whisper'
              ? 'Whisper 模型激活后可用，按电脑性能和磁盘空间选择。'
              : config.stt_provider === 'native-asr'
                ? '开源本地模型，激活后可用；按需下载，音频不会上传。首次加载与识别速度取决于电脑性能。'
                : isCloud
                  ? '云端识别需要网络；可用额度与服务状态以账号页面为准。'
                  : '使用你自己的云服务账号，音频会上传到所选服务商；额度与费用由服务商结算。'}
      </p>

      {isCustomWhisper ? (
        <CustomCloudSettings />
      ) : isCloud ? (
        <div className="border border-border rounded-[10px] px-3 py-3 space-y-2">
          <div className="flex items-center gap-2 text-[13px]">
            <Crown size={14} className="text-accent" />
            <span className="text-text-primary font-medium">{t('settings.cloudSttPro')}</span>
          </div>
          {!user ? (
            <p className="text-[12px] text-text-secondary">{t('settings.sttSignInHint')}</p>
          ) : plan !== 'pro' ? (
            <p className="text-[12px] text-text-secondary">{t('settings.sttUpgradeHint')}</p>
          ) : (
            <p className="text-[12px] text-green-500">{t('settings.sttProActive')}</p>
          )}
        </div>
      ) : !isLocalProvider ? (
        <>
          {isVolcSeedAsr && (
            <>
              <FormField label="认证方式">
                <select
                  value={config.volcengine_auth_mode || 'app-token'}
                  onChange={(e) => {
                    updateConfig({
                      volcengine_auth_mode: e.target.value as 'app-token' | 'api-key',
                    })
                    setSttTestStatus('idle')
                    setSttLatencyMs(null)
                    setSttTestError(null)
                  }}
                  className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
                >
                  <option value="app-token">APP ID + Access Token（应用认证，推荐）</option>
                  <option value="api-key">豆包语音 API Key（需已获得资源授权）</option>
                </select>
              </FormField>

              {config.volcengine_auth_mode !== 'api-key' && (
                <FormField label="APP ID">
                  <input
                    type="text"
                    value={config.volcengine_app_id}
                    onChange={(e) => {
                      updateConfig({ volcengine_app_id: e.target.value })
                      setSttTestStatus('idle')
                      setSttLatencyMs(null)
                      setSttTestError(null)
                    }}
                    placeholder="在语音技术控制台的服务接口认证信息中查看"
                    className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
                  />
                </FormField>
              )}
            </>
          )}

          <FormField
            label={
              isVolcSeedAsr && config.volcengine_auth_mode !== 'api-key'
                ? 'Access Token'
                : t('settings.apiKey')
            }
          >
            <div className="flex gap-2">
              <input
                type="password"
                value={activeSttCredential}
                onChange={(e) => {
                  updateConfig(
                    isVolcSeedAsr
                      ? { volcengine_credential: e.target.value }
                      : { stt_api_key: e.target.value },
                  )
                  setSttTestStatus('idle')
                  setSttLatencyMs(null)
                  setSttTestError(null)
                }}
                placeholder={
                  isVolcSeedAsr && config.volcengine_auth_mode !== 'api-key'
                    ? '输入 Access Token（保存后下次自动读入）'
                    : t('settings.enterApiKey')
                }
                className="flex-1 px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
              />
              <button
                onClick={handleTest}
                disabled={
                  !activeSttCredential ||
                  sttTestStatus === 'testing' ||
                  (isVolcSeedAsr &&
                    config.volcengine_auth_mode !== 'api-key' &&
                    !config.volcengine_app_id.trim())
                }
                className="px-4 py-2.5 bg-accent text-white rounded-[10px] text-[13px] border-none cursor-pointer hover:bg-accent-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
              >
                {sttTestStatus === 'testing' && <Loader2 size={14} className="animate-spin" />}
                {t('settings.test')}
              </button>
            </div>
            {sttTestStatus === 'success' && (
              <p className="flex items-center gap-1 text-[12px] text-success mt-2">
                <CheckCircle2 size={13} />{' '}
                {sttLatencyMs !== null ? `${sttLatencyMs}ms` : t('settings.connectionSuccess')}
              </p>
            )}
            {sttTestStatus === 'error' && (
              <div className="text-[12px] leading-5 text-error mt-2">
                <p className="flex items-start gap-1">
                  <XCircle size={13} className="mt-0.5 shrink-0" />
                  {t('settings.connectionFailed')}
                </p>
                {sttTestError && (
                  <details className="mt-1">
                    <summary className="cursor-pointer">查看连接详情</summary>
                    <p className="mt-1 break-all">{sttTestError}</p>
                  </details>
                )}
              </div>
            )}
            <p className="text-[11px] text-text-tertiary mt-1.5">
              {isVolcSeedAsr
                ? '保存设置后将明文写入本机 settings.json，下次启动自动读入；不会写入 Windows 凭据管理器。'
                : t('settings.storedLocally')}
            </p>
          </FormField>

          {isVolcSeedAsr && (
            <div className="border border-border rounded-[10px] px-3 py-3 space-y-3">
              <div className="flex items-center gap-2 text-[13px] text-text-primary">
                <ShieldCheck size={14} className="text-accent" />
                <span className="font-medium">火山引擎官方双鉴权</span>
              </div>
              <p className="text-[11px] leading-5 text-text-secondary">
                使用已开通语音识别资源的 APP ID + Access Token 或 API Key。 软件只调用官方 WebSocket
                API，不读取浏览器 Cookie。按你的设置，Token/Key 会明文保存在本机配置文件中。
              </p>
              <details className="rounded-[8px] border border-border px-3 py-2 text-[11px]">
                <summary className="cursor-pointer text-text-secondary">高级接口信息</summary>
                <FormField label="资源 ID（按识别时长计费）">
                  <input
                    type="text"
                    readOnly
                    value={config.stt_model || 'volc.seedasr.sauc.duration'}
                    className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[12px] text-text-primary outline-none"
                  />
                </FormField>
              </details>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() =>
                    void openUrl(
                      'https://console.volcengine.com/speech/new/setting/apikeys?projectName=default',
                    )
                  }
                  className="flex items-center gap-1.5 px-3 py-2 bg-accent text-white rounded-[8px] text-[12px] border-none cursor-pointer hover:bg-accent-hover"
                >
                  <ExternalLink size={13} /> 获取 API Key
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void openUrl(
                      'https://console.volcengine.com/ark/region:cn-beijing/tts/speechRecognition',
                    )
                  }
                  className="flex items-center gap-1.5 px-3 py-2 bg-bg-secondary text-text-primary rounded-[8px] text-[12px] border border-border cursor-pointer hover:bg-bg-tertiary"
                >
                  <ExternalLink size={13} /> 查看试用认证信息
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void openUrl(
                      'https://console.volcengine.com/ark/region:cn-beijing/experience/voice?model=seedasr-streaming',
                    )
                  }
                  className="flex items-center gap-1.5 px-3 py-2 bg-bg-secondary text-text-primary rounded-[8px] text-[12px] border border-border cursor-pointer hover:bg-bg-tertiary"
                >
                  <ExternalLink size={13} /> 查看网页体验
                </button>
              </div>
              <p className="text-[11px] leading-5 text-text-tertiary">
                网页体验分钟数与 API 免费资源包分开统计，不能作为软件接口转发；API
                免费额度及计费状态请在控制台核对。连接测试会发送约 0.2 秒静音，可能消耗额度；
                软件不承诺免费或自动阻止服务商扣费。
              </p>
            </div>
          )}
        </>
      ) : null}

      {!['sensevoice', 'funasr-nano', 'volcengine-seedasr', 'custom-whisper'].includes(
        config.stt_provider,
      ) && (
        <FormField label={t('settings.sttLanguage')}>
          <select
            value={config.stt_language}
            onChange={(e) => updateConfig({ stt_language: e.target.value })}
            className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
          >
            {LANGUAGES.filter(
              (l) =>
                config.stt_provider !== 'native-asr' ||
                config.native_asr.model_id !== 'cohere-transcribe-03-2026' ||
                COHERE_LANGUAGE_CODES.has(l.value),
            ).map((l) => (
              <option key={l.value} value={l.value}>
                {config.stt_provider === 'native-asr' &&
                config.native_asr.model_id === 'cohere-transcribe-03-2026' &&
                l.value === 'multi'
                  ? '中文（默认；此模型不自动检测语言）'
                  : l.label}
              </option>
            ))}
            {config.stt_provider === 'native-asr' &&
              config.native_asr.model_id === 'cohere-transcribe-03-2026' && (
                <option value="el">Ελληνικά (Greek)</option>
              )}
          </select>
        </FormField>
      )}

      {isLocalProvider && (
        <details
          key={selectedModel}
          open={availability[selectedModel] === false || undefined}
          className="rounded-[12px] border border-border bg-bg-elevated px-4 py-3 text-[13px]"
        >
          <summary className="cursor-pointer font-medium text-text-secondary">
            模型管理与高级设置{' '}
            <span className="ml-2 text-[11px] font-normal text-text-tertiary">
              下载 · 目录 · 识别参数
            </span>
          </summary>
          <div className="mt-4 space-y-4">
            {config.stt_provider === 'local-whisper' && (
              <>
                <LocalWhisperPanel onModelReady={() => setModelSelectionHint(null)} />
                <details className="rounded-[10px] border border-border px-3 py-3 text-[12px]">
                  <summary className="cursor-pointer text-text-secondary">高级兼容设置</summary>
                  <FormField label="外部适配文件（实验性）">
                    <input
                      type="text"
                      value={config.whisper_lora_path ?? ''}
                      onChange={(e) => updateConfig({ whisper_lora_path: e.target.value })}
                      placeholder="可选：方言 LoRA adapter 路径，如 models/cantonese.bin"
                      className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
                    />
                    <p className="text-[11px] text-text-tertiary mt-1">
                      通常留空。仅适用于与当前运行时兼容的适配文件，不保证支持任意方言模型。
                    </p>
                  </FormField>
                </details>
              </>
            )}

            {config.stt_provider === 'sensevoice' && <SenseVoicePanel />}
            {config.stt_provider === 'native-asr' && (
              <NativeAsrPanel
                key={selectedModel}
                onReadyChange={(ready) =>
                  setAvailability((current) => ({ ...current, [selectedModel]: ready }))
                }
              />
            )}
            {config.stt_provider === 'funasr-nano' && (
              <FunAsrPanel
                onReadyChange={(ready) =>
                  setAvailability((current) => ({ ...current, 'funasr-nano': ready }))
                }
              />
            )}
          </div>
        </details>
      )}
    </div>
  )
}

function FunAsrPanel({ onReadyChange }: { onReadyChange: (ready: boolean) => void }) {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [paths, setPaths] = useState<FunAsrPaths | null>(null)
  const [runtime, setRuntime] = useState<FunAsrRuntimeStatus | null>(null)
  const [progress, setProgress] = useState<FunAsrDownloadProgress | null>(null)
  const [downloading, setDownloading] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)
  const [guideVisible, setGuideVisible] = useState(false)
  const [updateMessage, setUpdateMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const customDir = config.funasr_use_custom_dir ? config.funasr_model_dir : ''
  const refresh = () => {
    getFunAsrPaths(customDir)
      .then((value) => {
        setPaths(value)
        onReadyChange(value.ready)
      })
      .catch((reason) => setError(String(reason)))
    getFunAsrRuntimeStatus()
      .then(setRuntime)
      .catch(() => undefined)
  }

  useEffect(() => {
    refresh()
    const timer = window.setInterval(() => {
      getFunAsrRuntimeStatus()
        .then(setRuntime)
        .catch(() => undefined)
    }, 3000)
    const unlisten = listen<FunAsrDownloadProgress>('funasr:download_progress', (event) => {
      setProgress(event.payload)
      if (event.payload.status === 'done' || event.payload.status === 'ready') {
        setDownloading(false)
        setGuideVisible(true)
        refresh()
      }
    })
    return () => {
      window.clearInterval(timer)
      unlisten.then((fn) => fn())
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [customDir])

  const handleDownload = async () => {
    setDownloading(true)
    setGuideVisible(false)
    setError(null)
    try {
      await downloadFunAsr(customDir, Boolean(paths?.ready))
      refresh()
    } catch (reason) {
      const message = String(reason)
      if (message.includes('暂停')) {
        setProgress((current) =>
          current ? { ...current, status: 'paused', message: '下载已暂停，可从断点继续' } : current,
        )
      } else {
        setError(message)
      }
    } finally {
      setDownloading(false)
    }
  }

  const handlePause = async () => {
    await cancelFunAsrDownload()
    setProgress((current) =>
      current ? { ...current, status: 'pausing', message: '正在暂停，已下载分片会保留…' } : current,
    )
  }

  const handleSelectFolder = async () => {
    setError(null)
    try {
      const selected = await selectDirectory({
        directory: true,
        multiple: false,
        title: '选择 Fun-ASR-Nano 模型目录',
        defaultPath: paths?.model_dir,
      })
      if (typeof selected === 'string' && selected.trim()) {
        updateConfig({ funasr_use_custom_dir: true, funasr_model_dir: selected })
      }
    } catch (reason) {
      setError(`无法选择模型目录：${String(reason)}`)
    }
  }

  const runAction = async (name: string, action: () => Promise<unknown>) => {
    setBusy(name)
    setError(null)
    try {
      await action()
      refresh()
    } catch (reason) {
      setError(String(reason))
    } finally {
      setBusy(null)
    }
  }

  const handleCheckUpdate = async () => {
    await runAction('update', async () => {
      const latest = await getFunAsrPaths(customDir)
      setPaths(latest)
      setUpdateMessage(
        latest.update_available
          ? '发现新版 Fun-ASR-Nano 组件，请点击上方“更新模型”'
          : '当前已是应用支持的最新组件版本',
      )
    })
  }

  const formatBytes = (bytes: number) => {
    if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
    const units = ['B', 'KB', 'MB', 'GB']
    const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
    return `${(bytes / 1024 ** unit).toFixed(unit >= 2 ? 1 : 0)} ${units[unit]}`
  }

  const formatEta = (seconds: number | null) => {
    if (seconds === null || !Number.isFinite(seconds)) return '计算中'
    if (seconds < 60) return `${seconds} 秒`
    return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`
  }

  if (!paths) {
    return (
      <div className="border border-border rounded-[10px] px-3 py-3 text-[12px] text-text-tertiary">
        {error ? <ModelOperationError message={error} /> : '正在检查 Fun-ASR-Nano 模型…'}
      </div>
    )
  }

  const components = [
    ['本地识别服务', paths.runtime_ready],
    ['语音编码组件（469 MB）', paths.encoder_ready],
    [
      paths.quantization.includes('Q5')
        ? '文字解码组件（526 MB，建议更新）'
        : '文字解码组件（462 MB）',
      paths.llm_ready,
    ],
    ['语音分段组件（1.7 MB）', paths.vad_ready],
  ] as const

  return (
    <div className="border border-border rounded-[10px] px-3 py-3 space-y-3">
      <div className="flex items-center gap-2 text-[13px] text-text-primary">
        <HardDrive size={14} className="text-accent" />
        <span>Fun-ASR-Nano GGUF</span>
        <span className={paths.ready ? 'ml-auto text-success' : 'ml-auto text-error'}>
          {paths.ready ? '已安装' : '可选下载'}
        </span>
      </div>
      <div className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2">
        <div className="flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <div className="text-[11px] font-medium text-text-secondary">
              {paths.is_custom ? '自定义模型目录' : '默认相对模型目录'}
            </div>
            <div className="mt-0.5 text-[11px] text-text-primary">
              {paths.is_custom ? '使用你选择的本机文件夹' : '应用所在目录中的 models 文件夹'}
            </div>
          </div>
          <button
            onClick={() => runAction('open', () => openFunAsrModelDirectory(customDir))}
            disabled={busy !== null || !paths.model_dir}
            className="rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary disabled:opacity-50"
          >
            打开目录
          </button>
        </div>
        <div className="mt-2 flex gap-2 border-t border-border/70 pt-2">
          <button
            onClick={handleSelectFolder}
            disabled={downloading || busy !== null}
            className="rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary disabled:opacity-50"
          >
            自定义目录
          </button>
          {config.funasr_use_custom_dir && (
            <button
              onClick={() => updateConfig({ funasr_use_custom_dir: false, funasr_model_dir: '' })}
              disabled={downloading || busy !== null}
              className="flex items-center gap-1 rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary disabled:opacity-50"
            >
              <RotateCcw size={12} /> 恢复默认
            </button>
          )}
        </div>
      </div>

      <FormField label={`推理线程：${config.funasr_num_threads}`}>
        <input
          type="range"
          min={1}
          max={16}
          value={config.funasr_num_threads}
          onChange={(event) => updateConfig({ funasr_num_threads: Number(event.target.value) })}
          className="w-full accent-accent"
        />
        <p className="mt-1 text-[10px] text-text-tertiary">
          修改后会自动重启并重新预加载常驻进程。
        </p>
      </FormField>

      <div className="space-y-1 text-[11px] text-text-tertiary">
        {components.map(([label, ready]) => (
          <div key={label} className="flex justify-between gap-2">
            <span>{label}</span>
            <span className={ready ? 'text-success' : 'text-error'}>
              {ready ? '已索引' : '缺失'}
            </span>
          </div>
        ))}
      </div>
      {paths.ready && (
        <div className="rounded-[8px] bg-bg-secondary px-3 py-2 text-[10px] text-text-secondary space-y-1">
          <div className="flex justify-between gap-3">
            <span>组件状态</span>
            <span>{paths.update_available ? '可更新' : '已安装'}</span>
          </div>
          <div className="flex justify-between gap-3">
            <span>模型大小 / 完整性</span>
            <span>
              {formatBytes(paths.installed_bytes)} · {paths.verified ? '已校验' : '待完整校验'}
            </span>
          </div>
          <div className="flex justify-between gap-3">
            <span>常驻推理</span>
            <span className={runtime?.ready ? 'text-success' : 'text-yellow-500'}>
              {runtime?.ready ? '运行中' : '未运行'}
            </span>
          </div>
        </div>
      )}

      {downloading && progress && (
        <div className="space-y-2 rounded-[8px] border border-accent/30 bg-accent/5 px-3 py-2">
          <div className="flex items-center justify-between text-[11px]">
            <span className="truncate text-text-secondary">
              {downloadStatusLabel(progress.status)}
            </span>
            <span className="ml-2 text-text-primary">{progress.percent}%</span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-bg-secondary">
            <div
              className="h-full bg-accent transition-all"
              style={{ width: `${progress.percent}%` }}
            />
          </div>
          <div className="flex flex-wrap justify-between gap-2 text-[10px] text-text-tertiary">
            <span>
              {formatBytes(progress.current)} / {formatBytes(progress.total)}
            </span>
            <span>
              {formatBytes(progress.speed_bytes_per_sec)}/s · 剩余 {formatEta(progress.eta_seconds)}
            </span>
            {progress.attempt > 1 && <span>第 {progress.attempt} 次重试</span>}
          </div>
          <button
            onClick={handlePause}
            className="flex w-full items-center justify-center gap-1 rounded-[6px] border border-border bg-bg-primary py-1.5 text-[11px] text-text-primary"
          >
            <Pause size={12} /> 暂停下载
          </button>
        </div>
      )}

      {!downloading && (
        <button
          onClick={handleDownload}
          disabled={busy !== null}
          className="flex w-full items-center justify-center gap-2 rounded-[8px] bg-accent px-3 py-2 text-[12px] text-white disabled:opacity-50"
        >
          <Download size={14} />
          {progress?.status === 'paused'
            ? '继续下载（保留断点）'
            : paths.ready
              ? paths.update_available
                ? '更新模型'
                : '重新下载 / 修复模型'
              : '下载 Fun-ASR-Nano（约 911 MB）'}
        </button>
      )}

      {paths.ready && !downloading && (
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => runAction('verify', () => verifyFunAsr(customDir))}
            disabled={busy !== null}
            className="flex items-center justify-center gap-1 rounded-[7px] border border-border bg-bg-secondary py-1.5 text-[11px] text-text-primary disabled:opacity-50"
          >
            {busy === 'verify' ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <CheckCircle2 size={12} />
            )}
            校验模型
          </button>
          <button
            onClick={handleCheckUpdate}
            disabled={busy !== null}
            className="flex items-center justify-center gap-1 rounded-[7px] border border-border bg-bg-secondary py-1.5 text-[11px] text-text-primary disabled:opacity-50"
          >
            <RefreshCw size={12} /> 检查更新
          </button>
          <button
            onClick={() => runAction('restart', restartFunAsrRuntime)}
            disabled={busy !== null}
            className="flex items-center justify-center gap-1 rounded-[7px] border border-border bg-bg-secondary py-1.5 text-[11px] text-text-primary disabled:opacity-50"
          >
            {busy === 'restart' ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <RotateCcw size={12} />
            )}
            重启常驻进程
          </button>
          <button
            onClick={() => {
              if (
                window.confirm('确定删除当前目录中的 Fun-ASR-Nano 模型吗？下载断点也会一起删除。')
              ) {
                void runAction('remove', async () => {
                  await removeFunAsr(customDir)
                  setGuideVisible(false)
                })
              }
            }}
            disabled={busy !== null}
            className="flex items-center justify-center gap-1 rounded-[7px] border border-red-500/30 bg-red-500/5 py-1.5 text-[11px] text-red-500 disabled:opacity-50"
          >
            <Trash2 size={12} /> 删除模型
          </button>
        </div>
      )}

      {updateMessage && !downloading && (
        <p className="rounded-[7px] bg-bg-secondary px-2.5 py-1.5 text-[10px] text-text-secondary">
          {updateMessage}
        </p>
      )}

      {guideVisible && (
        <div className="rounded-[8px] border border-green-500/30 bg-green-500/10 px-3 py-2 text-[11px] text-text-secondary">
          <div className="flex items-center gap-1 font-medium text-success">
            <CheckCircle2 size={13} /> 安装完成
          </div>
          <p className="mt-1">Fun-ASR-Nano 已安装。保存设置后，下一段录音即可使用。</p>
        </div>
      )}

      <p className="text-[11px] text-text-secondary">
        识别在本机完成，无需独立显卡。应用启动时预加载组件，后续录音复用已准备好的识别服务。
      </p>
      <p className="text-[11px] text-text-secondary">
        这里管理语音识别组件；文字整理可在“AI 润色”页面单独开启。
      </p>
      <p className="text-[10px] text-text-tertiary">
        默认安装到应用所在目录的 models
        文件夹，也可选择自定义目录。使用国内下载源，失败自动重试并支持断点续传。
      </p>
      <details className="rounded-[8px] border border-border px-3 py-2 text-[11px] text-text-tertiary">
        <summary className="cursor-pointer text-text-secondary">高级技术信息</summary>
        <div className="mt-2 space-y-1 break-all">
          <p>Fun-ASR-Nano GGUF；Qwen3-0.6B 语音解码器；FSMN-VAD。</p>
          <p>
            版本：{paths.model_version} · {paths.quantization}；运行时：{paths.runtime_variant}
          </p>
          <p>目录：{paths.display_dir || paths.model_dir}</p>
          <p>完整路径：{paths.model_dir}</p>
          <p>通过本机命名管道传入 16 kHz PCM；进程 ID：{runtime?.pid ?? '未启动'}</p>
          {progress && <p>下载详情：{progress.message}</p>}
        </div>
      </details>
      {error && <ModelOperationError message={error} />}
    </div>
  )
}

const SENSEVOICE_LANGUAGES = [
  { value: 'auto', label: '自动检测' },
  { value: 'zh', label: '中文（普通话）' },
  { value: 'en', label: 'English' },
  { value: 'yue', label: '粤语 Cantonese' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' },
]

function SenseVoicePanel() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [paths, setPaths] = useState<SenseVoicePaths | null>(null)
  const [downloading, setDownloading] = useState(false)
  const [progress, setProgress] = useState<{
    current: number
    total: number
    status: string
    message: string
  } | null>(null)
  const [error, setError] = useState<string | null>(null)

  const customDir = config.sensevoice_use_custom_dir ? config.sensevoice_model_dir : ''

  const refresh = () => {
    getSenseVoicePaths(customDir)
      .then(setPaths)
      .catch((e) => setError(String(e)))
  }

  useEffect(() => {
    refresh()
    const unlisten = listen<{
      current: number
      total: number
      status: string
      message: string
    }>('sensevoice:download_progress', (event) => {
      const { current, total, status, message } = event.payload
      setProgress({ current, total, status, message })
      if (status === 'done' || status === 'ready') {
        setDownloading(false)
        refresh()
      }
    })
    return () => {
      unlisten.then((fn) => fn())
    }
    // 当自定义目录变化时重新加载
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [customDir])

  const handleDownload = async () => {
    setDownloading(true)
    setError(null)
    setProgress({
      current: 0,
      total: 100,
      status: 'preparing',
      message: paths?.ready ? '准备重新下载并校验模型...' : '准备下载模型...',
    })
    try {
      await downloadSenseVoice(customDir, Boolean(paths?.ready))
      refresh()
    } catch (e) {
      setError(String(e))
    } finally {
      setDownloading(false)
    }
  }

  const handleOpenFolder = async () => {
    setError(null)
    try {
      await openSenseVoiceModelDirectory(customDir)
    } catch (e) {
      setError(String(e))
    }
  }

  const handleSelectFolder = async () => {
    setError(null)
    try {
      const selected = await selectDirectory({
        directory: true,
        multiple: false,
        title: '选择 SenseVoice 模型目录',
        defaultPath: paths?.model_dir,
      })
      if (typeof selected === 'string' && selected.trim()) {
        updateConfig({
          sensevoice_use_custom_dir: true,
          sensevoice_model_dir: selected,
        })
      }
    } catch (e) {
      setError(`无法选择模型目录：${String(e)}`)
    }
  }

  const handleRestoreDefault = () => {
    setError(null)
    updateConfig({
      sensevoice_use_custom_dir: false,
      sensevoice_model_dir: '',
    })
  }

  return (
    <div className="border border-border rounded-[10px] px-3 py-3 space-y-3">
      <div className="flex items-center gap-2 text-[13px] text-text-primary">
        <HardDrive size={14} className="text-accent" />
        <span>SenseVoice Small INT8</span>
        <span className="ml-auto text-[10px] px-2 py-0.5 rounded-full bg-accent/10 text-accent">
          推荐
        </span>
      </div>

      <div className="text-[11px] text-text-tertiary leading-relaxed">
        占用空间较小，适合普通话、粤语和中英混说。识别在本机完成，音频不会上传。
      </div>

      <FormField label="模型目录">
        <div className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2">
          <div className="flex items-center gap-2">
            <div className="min-w-0 flex-1">
              <div
                className={`text-[11px] font-medium ${paths?.ready ? 'text-success' : 'text-text-secondary'}`}
              >
                {paths?.is_custom ? '自定义模型目录' : '自动索引 EXE 相对目录'}
              </div>
              <div className="mt-0.5 text-[11px] text-text-primary">
                {paths?.is_custom ? '使用你选择的本机文件夹' : '应用所在目录中的 models 文件夹'}
              </div>
            </div>
            <button
              onClick={handleOpenFolder}
              disabled={!paths?.model_dir}
              className="whitespace-nowrap rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary transition-colors hover:bg-bg-tertiary disabled:opacity-50"
              title="在资源管理器中打开当前模型目录"
            >
              打开目录
            </button>
          </div>
          <div className="mt-2 flex flex-wrap gap-2 border-t border-border/70 pt-2">
            <button
              onClick={handleSelectFolder}
              className="rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary transition-colors hover:bg-bg-tertiary"
              title="选择已安装组件的文件夹，或选择空文件夹后下载"
            >
              选择目录
            </button>
            {config.sensevoice_use_custom_dir && (
              <button
                onClick={handleRestoreDefault}
                className="flex items-center gap-1 rounded-[6px] border border-border bg-bg-primary px-2.5 py-1.5 text-[11px] text-text-primary transition-colors hover:bg-bg-tertiary"
              >
                <RotateCcw size={12} />
                恢复默认
              </button>
            )}
          </div>
        </div>
        <p className="mt-1 text-[10px] text-text-tertiary">
          默认读取应用同级的 models 文件夹；自定义目录会保存在本机设置中。
        </p>
      </FormField>

      {/* 模型状态 */}
      {paths && (
        <div className="flex items-center gap-2 text-[11px]">
          {paths.ready ? (
            <>
              <CheckCircle2 size={14} className="text-green-500" />
              <span className="text-text-secondary">模型已就绪</span>
            </>
          ) : (
            <>
              <AlertCircle size={14} className="text-yellow-500" />
              <span className="text-text-secondary">模型未就绪</span>
              <span className="text-text-tertiary ml-2">
                软件自带模型，如无法识别请点击下方下载按钮
              </span>
            </>
          )}
        </div>
      )}

      {/* 下载按钮（可选，用于更新模型或切换位置）*/}
      {paths && !downloading && (
        <button
          onClick={handleDownload}
          className="w-full px-3 py-2 bg-bg-secondary border border-border hover:bg-bg-tertiary text-[12px] text-text-primary rounded-[8px] transition-colors flex items-center justify-center gap-2"
        >
          <Download size={14} />
          {paths.ready ? '重新下载模型' : '下载 SenseVoice Small'}
        </button>
      )}

      {/* 下载进度 */}
      {downloading && progress && (
        <div className="space-y-2">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-text-secondary">{downloadStatusLabel(progress.status)}</span>
            <span className="text-text-tertiary">{progress.current}%</span>
          </div>
          <div className="h-2 bg-bg-secondary rounded-full overflow-hidden">
            <div
              className="h-full bg-accent transition-all duration-300"
              style={{ width: `${progress.current}%` }}
            />
          </div>
        </div>
      )}

      {/* 错误提示 */}
      {error && <ModelOperationError message={error} />}

      {/* 配置项 */}
      <FormField label="识别语言">
        <select
          value={config.sensevoice_language}
          onChange={(e) => updateConfig({ sensevoice_language: e.target.value })}
          disabled={!paths?.ready}
          className="w-full px-3 py-2 bg-bg-secondary border border-border rounded-[8px] text-[12px] text-text-primary outline-none focus:border-border-focus transition-colors disabled:opacity-50"
        >
          {SENSEVOICE_LANGUAGES.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
        <p className="text-[11px] text-text-tertiary mt-1">
          自动检测适合中英混说；如果只说中文可选「中文（普通话）」提升准确率。
        </p>
      </FormField>

      <FormField label={`推理线程数 (${config.sensevoice_num_threads})`}>
        <input
          type="range"
          min={1}
          max={8}
          step={1}
          value={config.sensevoice_num_threads}
          onChange={(e) => updateConfig({ sensevoice_num_threads: parseInt(e.target.value, 10) })}
          disabled={!paths?.ready}
          className="w-full disabled:opacity-50"
        />
        <p className="text-[11px] text-text-tertiary mt-1">
          控制本地识别使用的处理器线程数。可从 2–4 线程开始测试，线程更多不一定更快。
        </p>
      </FormField>
      <details className="rounded-[8px] border border-border px-3 py-2 text-[11px] text-text-tertiary">
        <summary className="cursor-pointer text-text-secondary">高级技术信息</summary>
        <div className="mt-2 space-y-1 break-all">
          <p>SenseVoice-Small INT8。组件文件：model.int8.onnx 与 tokens.txt。</p>
          <p>目录：{paths?.display_dir || '.\\models\\sensevoice'}</p>
          <p>完整路径：{paths?.model_dir || '正在检查'}</p>
          {progress && <p>下载详情：{progress.message}</p>}
        </div>
      </details>
    </div>
  )
}

interface ModelProgress {
  filename: string
  downloaded: number
  total: number
  phase: string
  speed_bytes_per_sec?: number
  attempt?: number
}

function LocalWhisperPanel({ onModelReady }: { onModelReady: () => void }) {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [paths, setPaths] = useState<LocalModelPaths | null>(null)
  const [downloading, setDownloading] = useState<string | null>(null)
  const [progress, setProgress] = useState<ModelProgress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [failedFilename, setFailedFilename] = useState<string | null>(null)
  const [installedHint, setInstalledHint] = useState<string | null>(null)
  const [customDirectory, setCustomDirectory] = useState<string | undefined>(() =>
    configuredModelDirectory(config.whisper_model_path),
  )

  const refresh = useCallback(() => {
    getLocalModelPaths(customDirectory)
      .then(setPaths)
      .catch((e) => setError(String(e)))
  }, [customDirectory])

  useEffect(() => {
    refresh()
    const unlistenPromise = listen<ModelProgress>('model:progress', (e) => {
      setProgress(e.payload)
      if (e.payload.phase === 'done' || e.payload.phase === 'ready') {
        setTimeout(() => refresh(), 250)
      }
    })
    return () => {
      unlistenPromise.then((u) => u()).catch(() => {})
    }
  }, [refresh])

  const handleDownload = async (filename: string) => {
    setError(null)
    setFailedFilename(null)
    setInstalledHint(null)
    setDownloading(filename)
    setProgress({ filename, downloaded: 0, total: 0, phase: 'starting' })
    try {
      const modelPath = await downloadLocalModel(filename, customDirectory)
      updateConfig({ whisper_model_path: modelPath })
      setInstalledHint('模型已安装并选中。点击“保存并应用”，下一段录音即可使用。')
      onModelReady()
    } catch (e) {
      setError(String(e))
      setFailedFilename(filename)
    } finally {
      setDownloading(null)
      refresh()
    }
  }

  if (!paths) {
    return (
      <div className="border border-border rounded-[10px] px-3 py-3 text-[12px] text-text-tertiary">
        {error ? <ModelOperationError message={error} /> : '正在检查 Whisper 模型…'}
      </div>
    )
  }

  const cliMissing = !paths.cli_path
  const activeVariant =
    WHISPER_VARIANTS.find((variant) =>
      config.whisper_model_path?.toLowerCase().endsWith(variant.filename),
    )?.choice ?? 'whisper-tiny'
  const activeProgress = downloading && progress?.filename === downloading ? progress : null
  const pct =
    activeProgress && activeProgress.total > 0
      ? Math.min(100, (activeProgress.downloaded / activeProgress.total) * 100)
      : null

  return (
    <div className="border border-border rounded-[10px] px-3 py-3 space-y-3">
      <div className="flex items-center gap-2 text-[13px] text-text-primary">
        <HardDrive size={14} className="text-accent" />
        <span>OpenAI Whisper</span>
      </div>

      <div className="rounded-[8px] bg-bg-secondary px-3 py-2.5 space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-[12px] font-medium text-text-primary">模型安装目录</span>
          <div className="flex gap-3 text-[11px]">
            <button
              type="button"
              disabled={!!downloading}
              className="text-accent disabled:opacity-50"
              onClick={async () => {
                try {
                  const selected = await selectDirectory({ directory: true, multiple: false })
                  if (typeof selected === 'string') {
                    setError(null)
                    setCustomDirectory(selected)
                  }
                } catch (reason) {
                  setError(String(reason))
                }
              }}
            >
              选择目录
            </button>
            {customDirectory && (
              <button
                type="button"
                disabled={!!downloading}
                className="text-text-secondary disabled:opacity-50"
                onClick={() => setCustomDirectory(undefined)}
              >
                恢复默认
              </button>
            )}
          </div>
        </div>
        <p className="break-all text-[11px] text-text-secondary">{paths.model_dir}</p>
        <p className="text-[10px] text-text-tertiary">
          默认安装在程序旁的 models/whisper；目录不可写时使用本机应用数据目录。
        </p>
      </div>

      <div className="space-y-1 text-[11px] text-text-tertiary">
        <div className="flex justify-between gap-2">
          <span>本地识别服务</span>
          <span className={cliMissing ? 'text-error' : 'text-success'}>
            {cliMissing ? '未打包' : '就绪'}
          </span>
        </div>
        {WHISPER_VARIANTS.map((variant) => {
          const ready = paths[variant.readyKey]
          const active = activeVariant === variant.choice
          return (
            <div key={variant.choice} className="flex justify-between gap-2">
              <span>
                {variant.label}（约 {variant.size} MB）
              </span>
              <button
                type="button"
                disabled={!ready || active}
                onClick={() => updateConfig({ whisper_model_path: paths[variant.pathKey] })}
                className={active ? 'text-success' : 'text-accent cursor-pointer'}
              >
                {!ready ? (active ? '已选择 · 待下载' : '未安装') : active ? '已选择' : '切换'}
              </button>
            </div>
          )
        })}
      </div>

      {WHISPER_VARIANTS.some(
        (variant) => variant.choice === activeVariant && paths[variant.readyKey],
      ) && (
        <button
          type="button"
          disabled={!!downloading}
          onClick={() => {
            const variant = WHISPER_VARIANTS.find((item) => item.choice === activeVariant)
            if (variant) void handleDownload(variant.filename)
          }}
          className="text-[11px] text-accent disabled:opacity-50"
        >
          校验当前模型，损坏时重新下载
        </button>
      )}

      {cliMissing && (
        <p className="text-[11px] text-error">本地识别服务缺失。请重新解压完整离线版后再试。</p>
      )}

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {WHISPER_VARIANTS.filter((variant) => !paths[variant.readyKey]).map((variant) => (
          <button
            key={variant.choice}
            type="button"
            disabled={!!downloading}
            onClick={() => handleDownload(variant.filename)}
            className="flex items-center justify-center gap-2 px-3 py-2 rounded-[8px] border border-border text-[12px] bg-bg-secondary hover:bg-bg-tertiary disabled:opacity-50 cursor-pointer text-text-primary"
          >
            {downloading === variant.filename ? (
              <Loader2 size={13} className="animate-spin" />
            ) : (
              <Download size={13} />
            )}
            下载{variant.label}（约 {variant.size} MB）
          </button>
        ))}
      </div>

      {activeProgress && (
        <div className="space-y-1">
          <div className="h-1.5 w-full bg-bg-secondary rounded overflow-hidden">
            <div
              className="h-full bg-accent transition-all"
              style={{ width: pct != null ? `${pct}%` : '30%' }}
            />
          </div>
          <div className="text-[10px] text-text-tertiary flex justify-between">
            <span>
              {downloadStatusLabel(activeProgress.phase)}
              {(activeProgress.attempt ?? 0) > 1 ? ` · 第 ${activeProgress.attempt} 次尝试` : ''}
            </span>
            <span>
              {activeProgress.phase === 'downloading' &&
                `${((activeProgress.speed_bytes_per_sec ?? 0) / (1024 * 1024)).toFixed(2)} MB/s · `}
              {(activeProgress.downloaded / (1024 * 1024)).toFixed(1)} /{' '}
              {activeProgress.total ? (activeProgress.total / (1024 * 1024)).toFixed(1) : '?'} MB
            </span>
          </div>
        </div>
      )}

      {installedHint && (
        <p role="status" className="text-[12px] text-success">
          {installedHint}
        </p>
      )}

      <details className="rounded-[8px] border border-border px-3 py-2 text-[11px] text-text-tertiary">
        <summary className="cursor-pointer text-text-secondary">高级技术信息</summary>
        <div className="mt-2 space-y-1 break-all">
          <p>
            Whisper，whisper.cpp 本地运行时。tiny / base 随包提供；small Q5 与 large-v3-turbo Q5
            按需下载。权重使用 MIT 许可。
          </p>
          <p>目录：{paths.model_dir}</p>
          <p>运行时：{paths.cli_path || '未安装'}</p>
        </div>
      </details>
      {error && <ModelOperationError message={error} />}
      {failedFilename && !downloading && (
        <button
          type="button"
          className="text-[12px] text-accent"
          onClick={() => handleDownload(failedFilename)}
        >
          重试下载
        </button>
      )}
    </div>
  )
}

function downloadStatusLabel(status: string) {
  const labels: Record<string, string> = {
    starting: '正在准备下载',
    preparing: '正在准备下载',
    downloading: '正在下载组件',
    verifying: '正在校验组件',
    extracting: '正在安装组件',
    installing: '正在安装组件',
    retrying: '下载重试中',
    pausing: '正在暂停下载',
    paused: '已暂停，可继续下载',
    done: '安装完成',
    ready: '组件已就绪',
    error: '下载未完成，可重试',
  }
  return labels[status] ?? '正在处理组件'
}

function ModelOperationError({ message }: { message: string }) {
  return (
    <div className="rounded-[8px] bg-red-500/10 px-3 py-2 text-[11px] text-red-500">
      <p className="flex items-start gap-2">
        <AlertCircle size={14} className="mt-0.5 shrink-0" />
        操作未完成。请检查网络、磁盘空间或组件目录后重试。
      </p>
      <details className="mt-2">
        <summary className="cursor-pointer">查看技术详情</summary>
        <p className="mt-1 break-all">{message}</p>
      </details>
    </div>
  )
}
