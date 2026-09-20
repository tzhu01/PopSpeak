import { useState, useEffect, useCallback, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { useAuthStore } from '../../stores/authStore'
import {
  LLM_PROVIDERS,
  LLM_DEFAULT_CONFIG,
  TARGET_LANGUAGES,
  POLISH_MODES,
} from '../../lib/constants'
import {
  benchLlmConnection,
  fetchLlmModels,
  getLocalLlmPaths,
  localLlmHealth,
  type LocalLlmPaths,
} from '../../lib/tauri'
import { FormField } from './shared/FormField'
import { Toggle } from './shared/Toggle'
import { CheckCircle2, XCircle, Loader2, RefreshCw, Crown, HardDrive } from 'lucide-react'

export function LlmPane() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const llmTestStatus = useAppStore((s) => s.llmTestStatus)
  const setLlmTestStatus = useAppStore((s) => s.setLlmTestStatus)
  const llmLatencyMs = useAppStore((s) => s.llmLatencyMs)
  const setLlmLatencyMs = useAppStore((s) => s.setLlmLatencyMs)
  const { user, plan } = useAuthStore()
  const { t } = useTranslation()

  const isCloud = config.llm_provider === 'cloud'
  const isManagedLocal = config.llm_provider === 'local-llama'

  const models = useAppStore((s) => s.llmModels)
  const setModels = useAppStore((s) => s.setLlmModels)
  const [fetchingModels, setFetchingModels] = useState(false)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const doFetchModels = useCallback(
    async (apiKey: string, baseUrl: string) => {
      if (!baseUrl) return
      setFetchingModels(true)
      try {
        const list = await fetchLlmModels(apiKey, baseUrl)
        setModels(list)
      } catch {
        // Do not clear existing cache on failure — avoids infinite retry loop
        // (clearing would re-trigger the useEffect that checks models.length > 0)
      } finally {
        setFetchingModels(false)
      }
    },
    [setModels],
  )

  // Auto-fetch when API key or base URL changes (debounced); skips if models already cached
  useEffect(() => {
    if (isCloud || isManagedLocal) return
    if (!config.llm_api_key || !config.llm_base_url) return
    if (models.length > 0) return
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      doFetchModels(config.llm_api_key, config.llm_base_url)
    }, 500)
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current)
        debounceRef.current = null
      }
    }
  }, [
    config.llm_api_key,
    config.llm_base_url,
    doFetchModels,
    isCloud,
    isManagedLocal,
    models.length,
  ])

  const handleTest = async () => {
    setLlmTestStatus('testing')
    setLlmLatencyMs(null)
    try {
      const ms = await benchLlmConnection(
        config.llm_api_key,
        config.llm_provider,
        config.llm_base_url,
        config.llm_model,
      )
      setLlmLatencyMs(ms)
      setLlmTestStatus('success')
    } catch {
      setLlmTestStatus('error')
    }
  }

  return (
    <div className="space-y-5">
      <FormField label={t('settings.provider')}>
        <select
          value={config.llm_provider}
          onChange={(e) => {
            const provider = e.target.value as typeof config.llm_provider
            const defaults = LLM_DEFAULT_CONFIG[provider]
            updateConfig({
              llm_provider: provider,
              llm_base_url: defaults?.baseUrl ?? config.llm_base_url,
              llm_model: defaults?.model ?? config.llm_model,
            })
            setLlmTestStatus('idle')
            setLlmLatencyMs(null)
            setModels([])
          }}
          className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
        >
          {LLM_PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </FormField>

      <p className="text-[12px] leading-6 text-text-secondary">
        激活后可使用文字润色、翻译等后处理功能；云端服务的 API 费用由服务商另行结算。
      </p>

      {isCloud ? (
        <div className="border border-border rounded-[10px] px-3 py-3 space-y-2">
          <div className="flex items-center gap-2 text-[13px]">
            <Crown size={14} className="text-accent" />
            <span className="text-text-primary font-medium">{t('settings.cloudLlmPro')}</span>
          </div>
          {!user ? (
            <p className="text-[12px] text-text-secondary">{t('settings.llmSignInHint')}</p>
          ) : plan !== 'pro' ? (
            <p className="text-[12px] text-text-secondary">{t('settings.llmUpgradeHint')}</p>
          ) : (
            <p className="text-[12px] text-green-500">{t('settings.llmProActive')}</p>
          )}
        </div>
      ) : isManagedLocal ? null : (
        <>
          <FormField label={t('settings.apiKey')}>
            <div className="flex gap-2">
              <input
                type="password"
                value={config.llm_api_key}
                onChange={(e) => {
                  updateConfig({ llm_api_key: e.target.value })
                  setLlmTestStatus('idle')
                  setLlmLatencyMs(null)
                }}
                placeholder={t('settings.enterApiKey')}
                className="flex-1 px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
              />
              <button
                onClick={handleTest}
                disabled={!config.llm_api_key || llmTestStatus === 'testing'}
                className="px-4 py-2.5 bg-accent text-white rounded-[10px] text-[13px] border-none cursor-pointer hover:bg-accent-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
              >
                {llmTestStatus === 'testing' && <Loader2 size={14} className="animate-spin" />}
                {t('settings.test')}
              </button>
            </div>
            {llmTestStatus === 'success' && (
              <p className="flex items-center gap-1 text-[12px] text-success mt-2">
                <CheckCircle2 size={13} />{' '}
                {llmLatencyMs !== null ? `${llmLatencyMs}ms` : t('settings.connectionSuccess')}
              </p>
            )}
            {llmTestStatus === 'error' && (
              <p className="flex items-center gap-1 text-[12px] text-error mt-2">
                <XCircle size={13} /> {t('settings.connectionFailed')}
              </p>
            )}
            <p className="text-[11px] text-text-tertiary mt-1.5">{t('settings.storedLocally')}</p>
          </FormField>

          <FormField label={t('settings.model')}>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <input
                  list="llm-model-list"
                  value={config.llm_model}
                  onChange={(e) => {
                    updateConfig({ llm_model: e.target.value })
                    setLlmLatencyMs(null)
                  }}
                  placeholder="e.g. gpt-4o-mini"
                  className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
                />
                <datalist id="llm-model-list">
                  {models.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>
              <button
                onClick={() => doFetchModels(config.llm_api_key, config.llm_base_url)}
                disabled={fetchingModels || !config.llm_base_url}
                className="px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-secondary cursor-pointer hover:border-border-focus disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
                title={t('settings.fetchModels')}
              >
                <RefreshCw size={14} className={fetchingModels ? 'animate-spin' : ''} />
              </button>
            </div>
            {models.length > 0 && (
              <p className="text-[11px] text-text-tertiary mt-1">
                {t('settings.modelsAvailable', { count: models.length })}
              </p>
            )}
          </FormField>

          <FormField label={t('settings.baseUrl')}>
            <input
              value={config.llm_base_url}
              onChange={(e) => updateConfig({ llm_base_url: e.target.value })}
              placeholder="https://openrouter.ai/api/v1"
              className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
            />
          </FormField>
        </>
      )}

      {config.llm_provider === 'local-llama' && (
        <div className="mt-3">
          <LocalLlamaPanel />
        </div>
      )}

      <div className="space-y-3 pt-1">
        <Toggle
          checked={config.polish_enabled}
          onChange={(checked) => updateConfig({ polish_enabled: checked })}
          label={t('settings.enableAiPolish')}
        />
        {config.polish_enabled && (
          <FormField label="润色模式 (Polish Mode)">
            <select
              value={config.polish_mode}
              onChange={(e) => updateConfig({ polish_mode: e.target.value })}
              className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
            >
              {POLISH_MODES.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
          </FormField>
        )}
        <Toggle
          checked={config.translate_enabled}
          onChange={(checked) => updateConfig({ translate_enabled: checked })}
          label={t('settings.translationMode')}
        />
        <Toggle
          checked={config.selected_text_enabled}
          onChange={(checked) => updateConfig({ selected_text_enabled: checked })}
          label={t('settings.selectedTextContext')}
        />
        {config.selected_text_enabled && (
          <p className="text-[11px] text-text-tertiary -mt-1 ml-[52px]">
            {t('settings.selectedTextContextDesc')}
          </p>
        )}
      </div>

      {config.translate_enabled && (
        <FormField label={t('settings.targetLanguage')}>
          <select
            value={config.target_lang}
            onChange={(e) => updateConfig({ target_lang: e.target.value })}
            className="w-full px-3 py-2.5 bg-bg-secondary border border-border rounded-[10px] text-[13px] text-text-primary outline-none focus:border-border-focus transition-colors"
          >
            {TARGET_LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.label}
              </option>
            ))}
          </select>
        </FormField>
      )}
    </div>
  )
}

function LocalLlamaPanel() {
  const config = useAppStore((s) => s.config)
  const [paths, setPaths] = useState<LocalLlmPaths | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [serverRunning, setServerRunning] = useState(false)
  const [checkingHealth, setCheckingHealth] = useState(false)

  const refresh = useCallback(async () => {
    setError(null)
    try {
      setPaths(await getLocalLlmPaths())
    } catch {
      setError('完整离线资源未找到，请重新解压完整 ZIP。')
    }
  }, [])

  const checkHealth = useCallback(async () => {
    setCheckingHealth(true)
    try {
      const healthy = await localLlmHealth()
      setServerRunning(healthy)
    } catch {
      setServerRunning(false)
    } finally {
      setCheckingHealth(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    checkHealth()
    const timer = window.setInterval(checkHealth, 2_000)
    return () => {
      window.clearInterval(timer)
    }
  }, [checkHealth, refresh])

  if (!paths) {
    return (
      <div className="border border-border rounded-[10px] px-3 py-3">
        <div
          className={`flex items-center gap-2 text-[13px] ${error ? 'text-error' : 'text-text-tertiary'}`}
        >
          {error ? <XCircle size={14} /> : <Loader2 size={14} className="animate-spin" />}
          <span>{error ?? '正在自动索引本地组件…'}</span>
        </div>
      </div>
    )
  }

  return (
    <div className="border border-border rounded-[10px] px-3 py-3 space-y-3">
      <div className="flex items-center gap-2 text-[13px]">
        <HardDrive size={14} className="text-accent" />
        <span className="text-text-primary font-medium">本地文字润色模型</span>
      </div>

      <div className="rounded-[8px] border border-warning/30 bg-warning/5 px-3 py-2 text-[11px] leading-relaxed text-text-secondary">
        对语音识别结果做文字整理，尝试去除口癖、重复字并修正明显错字。
      </div>

      <div className="rounded-[8px] border border-border bg-bg-secondary px-3 py-2.5 space-y-2">
        <div className="flex items-center justify-between gap-3 text-[11px]">
          <span className="text-text-secondary">CPU 推理运行时</span>
          <span className="text-success">已自动索引</span>
        </div>
        <div className="flex items-center justify-between gap-3 text-[11px]">
          <span className="text-text-secondary">本地文字润色组件（约 469 MB）</span>
          <span className={paths.default_model_ready ? 'text-success' : 'text-error'}>
            {paths.default_model_ready ? '已自动索引' : '组件缺失'}
          </span>
        </div>
        <p className="border-t border-border pt-2 text-[10px] text-text-tertiary">
          自动读取随包附带的本地组件；无需填写路径、URL 或端口。
        </p>
      </div>

      <div className="flex items-center gap-2 text-[11px]">
        {serverRunning ? (
          <>
            <CheckCircle2 size={13} className="text-success" />
            <span className="text-success">本地润色服务运行中</span>
          </>
        ) : checkingHealth ? (
          <>
            <Loader2 size={13} className="animate-spin text-accent" />
            <span className="text-text-secondary">正在检查本地服务…</span>
          </>
        ) : !paths.default_model_ready ? (
          <>
            <XCircle size={13} className="text-error" />
            <span className="text-error">完整离线资源缺失，请重新解压完整 ZIP</span>
          </>
        ) : config.polish_enabled ? (
          <>
            <Loader2 size={13} className="animate-spin text-accent" />
            <span className="text-text-secondary">保存设置后自动启动；首次加载可能需要数秒</span>
          </>
        ) : (
          <>
            <CheckCircle2 size={13} className="text-success" />
            <span className="text-text-secondary">模型已就绪；开启 AI 润色后由应用自动启动</span>
          </>
        )}
      </div>

      <details className="rounded-[8px] border border-border px-3 py-2 text-[11px] text-text-tertiary">
        <summary className="cursor-pointer text-text-secondary">高级技术信息</summary>
        <p className="mt-2 leading-relaxed">
          Qwen2.5-0.5B-Instruct，Q4_K_M 量化。组件目录：models/llm 与 runtimes/llama。
          原始识别文本可在历史记录中保留；润色效果受文本内容与电脑性能影响。
        </p>
      </details>

      {error && <p className="text-[11px] text-error">{error}</p>}
    </div>
  )
}
