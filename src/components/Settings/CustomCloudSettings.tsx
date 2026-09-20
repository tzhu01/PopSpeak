import { useState } from 'react'
import { CheckCircle2, ExternalLink, Loader2, ShieldCheck, XCircle } from 'lucide-react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppStore, type CustomCloudConfig } from '../../stores/appStore'
import { benchSttConnection } from '../../lib/tauri'
import { FormField } from './shared/FormField'

type CloudField = keyof Omit<CustomCloudConfig, 'vendor'>
interface FieldDefinition {
  key: CloudField
  label: string
  secret?: boolean
  placeholder?: string
  required?: boolean
  options?: { value: string; label: string }[]
}

interface CloudPreset {
  label: string
  description: string
  docs: string
  fields: FieldDefinition[]
  defaults: Partial<CustomCloudConfig>
  alternativeAuth?: { label: string; fields: FieldDefinition[] }
}

const PRESETS: Record<string, CloudPreset> = {
  bytedance: {
    label: '字节火山引擎 · 豆包语音',
    description: '官方语音识别接口；使用已开通语音资源的应用认证信息。',
    docs: 'https://www.volcengine.com/docs/6561/1354869',
    fields: [
      { key: 'app_id', label: 'APP ID', required: true },
      { key: 'access_token', label: 'Access Token', secret: true, required: true },
      { key: 'model', label: '语音资源 ID', required: true },
    ],
    defaults: { model: 'volc.seedasr.sauc.duration' },
    alternativeAuth: {
      label: '已授权的语音 API Key',
      fields: [
        { key: 'api_key', label: '语音 API Key', secret: true, required: true },
        { key: 'model', label: '语音资源 ID', required: true },
      ],
    },
  },
  aliyun: {
    label: '阿里云 · 智能语音交互',
    description:
      '使用智能语音交互项目的 AppKey 与有效 Token；Token 过期后需重新填写，不是百炼 API Key。',
    docs: 'https://help.aliyun.com/zh/isi/developer-reference/websocket',
    fields: [
      { key: 'app_id', label: '项目 AppKey', required: true },
      { key: 'access_token', label: '语音服务 Token', secret: true, required: true },
      {
        key: 'region',
        label: '服务地域',
        options: [
          { value: '', label: '默认 · 上海' },
          { value: 'cn-shanghai', label: '上海' },
          { value: 'cn-beijing', label: '北京' },
          { value: 'cn-shenzhen', label: '深圳' },
        ],
      },
    ],
    defaults: {},
  },
  tencent: {
    label: '腾讯云 · 语音识别',
    description: '官方实时 WebSocket 语音识别；使用已授权的云 API 密钥，不使用登录 Cookie。',
    docs: 'https://cloud.tencent.com/document/api/1093/48982',
    fields: [
      { key: 'app_id', label: 'APP ID', required: true },
      { key: 'api_key', label: 'SecretId', secret: true, required: true },
      { key: 'api_secret', label: 'SecretKey', secret: true, required: true },
      { key: 'model', label: '识别引擎型号', required: true },
    ],
    defaults: { model: '16k_zh' },
  },
  iflytek: {
    label: '科大讯飞 · 语音听写',
    description: '语音听写 IAT 接口，单段最多 60 秒；使用已开通服务的 APPID、APIKey 与 APISecret。',
    docs: 'https://www.xfyun.cn/doc/asr/voicedictation/API.html',
    fields: [
      { key: 'app_id', label: 'APPID', required: true },
      { key: 'api_key', label: 'APIKey', secret: true, required: true },
      { key: 'api_secret', label: 'APISecret', secret: true, required: true },
    ],
    defaults: {},
  },
  baidu: {
    label: '百度智能云 · 短语音识别',
    description: '标准短语音 REST 接口，停止录音后整段转写，单段最多 60 秒；不是实时字幕接口。',
    docs: 'https://ai.baidu.com/ai-doc/SPEECH/Jlbxdezuf',
    fields: [
      { key: 'api_key', label: 'API Key', secret: true, required: true },
      { key: 'api_secret', label: 'Secret Key', secret: true, required: true },
      { key: 'model', label: '识别模型编号（dev_pid）', required: true },
    ],
    defaults: { model: '1537' },
    alternativeAuth: {
      label: '已有 Access Token',
      fields: [
        { key: 'access_token', label: 'Access Token', secret: true, required: true },
        { key: 'model', label: '识别模型编号（dev_pid）', required: true },
      ],
    },
  },
  whisper: {
    label: '通用 · 音频转写兼容接口',
    description:
      '仅适用于兼容 multipart /audio/transcriptions 协议的接口；其他厂商请选对应适配器。',
    docs: '',
    fields: [
      {
        key: 'endpoint',
        label: '音频转写地址',
        required: true,
        placeholder: 'https://你的服务地址/v1/audio/transcriptions',
      },
      { key: 'api_key', label: 'API Key', secret: true, required: true },
      { key: 'model', label: '模型 ID', required: true, placeholder: '服务商提供的模型 ID' },
    ],
    defaults: { model: 'whisper-1' },
  },
}

function emptyConfig(vendor: CustomCloudConfig['vendor']): CustomCloudConfig {
  return {
    vendor,
    app_id: '',
    api_key: '',
    api_secret: '',
    access_token: '',
    endpoint: '',
    model: '',
    region: '',
    ...PRESETS[vendor]?.defaults,
  }
}

function redactError(reason: unknown, config: CustomCloudConfig) {
  let message = String(reason).replace(/^Error:\s*/i, '')
  for (const secret of [config.api_key, config.api_secret, config.access_token]) {
    if (secret) message = message.split(secret).join('[已隐藏凭证]')
  }
  return message
}

export function CustomCloudSettings() {
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const cloud = config.custom_cloud ?? emptyConfig('whisper')
  const preset = PRESETS[cloud.vendor]
  const [status, setStatus] = useState<'idle' | 'testing' | 'success' | 'error'>('idle')
  const [latency, setLatency] = useState<number | null>(null)
  const [error, setError] = useState('')
  const [authVariant, setAuthVariant] = useState<'primary' | 'alternative' | null>(null)
  const activeAuth =
    authVariant ??
    ((cloud.vendor === 'bytedance' && cloud.api_key) ||
    (cloud.vendor === 'baidu' && cloud.access_token && !cloud.api_key)
      ? 'alternative'
      : 'primary')
  const fields =
    activeAuth === 'alternative' && preset?.alternativeAuth
      ? preset.alternativeAuth.fields
      : (preset?.fields ?? [])

  const update = (next: CustomCloudConfig) => {
    updateConfig({ custom_cloud: next })
    setStatus('idle')
    setLatency(null)
    setError('')
  }

  const test = async () => {
    setStatus('testing')
    setLatency(null)
    setError('')
    try {
      const ms = await benchSttConnection('', 'custom-whisper', '', '', '', '', cloud)
      setLatency(ms)
      setStatus('success')
    } catch (reason) {
      setError(redactError(reason, cloud))
      setStatus('error')
    }
  }

  const missingFields = fields.filter((field) => field.required && !cloud[field.key]?.trim())

  return (
    <section className="rounded-[14px] border border-border bg-bg-primary/60 p-4 space-y-4">
      <h3 className="flex items-center gap-2 text-[17px] font-semibold text-text-primary">
        <ShieldCheck size={19} className="text-accent" /> 云端接口配置
      </h3>
      <fieldset
        disabled={status === 'testing'}
        className="m-0 min-w-0 space-y-4 border-0 p-0 disabled:opacity-70"
      >
        <FormField label="服务商与接口类型">
          <select
            aria-label="服务商与接口类型"
            value={cloud.vendor}
            onChange={(event) => {
              setAuthVariant(null)
              update(emptyConfig(event.target.value as CustomCloudConfig['vendor']))
            }}
            className="w-full rounded-[10px] border border-border bg-bg-secondary px-3 py-2.5 text-[14px] text-text-primary outline-none focus:border-border-focus"
          >
            {!preset && <option value={cloud.vendor}>未知接口类型，请重新选择</option>}
            {Object.entries(PRESETS).map(([value, item]) => (
              <option value={value} key={value}>
                {item.label}
              </option>
            ))}
          </select>
          <p className="mt-2 text-[12px] leading-5 text-text-secondary">{preset?.description}</p>
          <p className="mt-1 text-[11px] text-text-tertiary">
            切换厂商会清空本页凭证，避免把密钥发送给其他厂商。
          </p>
        </FormField>

        {preset?.alternativeAuth && (
          <FormField label="认证方式">
            <select
              aria-label="认证方式"
              value={activeAuth}
              onChange={(event) => {
                setAuthVariant(event.target.value as 'primary' | 'alternative')
                update({ ...cloud, app_id: '', api_key: '', api_secret: '', access_token: '' })
              }}
              className="w-full rounded-[10px] border border-border bg-bg-secondary px-3 py-2.5 text-[14px] text-text-primary outline-none focus:border-border-focus"
            >
              <option value="primary">
                {cloud.vendor === 'bytedance' ? 'APP ID + Access Token' : 'API Key + Secret Key'}
              </option>
              <option value="alternative">{preset.alternativeAuth.label}</option>
            </select>
          </FormField>
        )}

        {fields.map((field) => (
          <FormField key={field.key} label={field.label}>
            {field.options ? (
              <select
                aria-label={field.label}
                value={cloud[field.key]}
                onChange={(event) => update({ ...cloud, [field.key]: event.target.value })}
                className="w-full rounded-[10px] border border-border bg-bg-secondary px-3 py-2.5 text-[14px] text-text-primary outline-none focus:border-border-focus"
              >
                {field.options.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            ) : (
              <input
                aria-label={field.label}
                type={field.secret ? 'password' : 'text'}
                autoComplete="off"
                spellCheck={false}
                value={cloud[field.key]}
                onChange={(event) => update({ ...cloud, [field.key]: event.target.value })}
                placeholder={field.placeholder ?? `输入${field.label}`}
                className="w-full rounded-[10px] border border-border bg-bg-secondary px-3 py-2.5 text-[14px] text-text-primary outline-none focus:border-border-focus"
              />
            )}
          </FormField>
        ))}
      </fieldset>

      <p className="rounded-[10px] border border-warning/25 bg-warning/5 px-3 py-2 text-[12px] leading-5 text-text-secondary">
        保存并应用后，凭证将明文写入本机 settings.json，下次启动自动读取。请勿分享个人配置文件。
        音频仅在你使用该云端模式时上传给所选服务商；API 额度、费用与时长限制由服务商决定。
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => void test()}
          disabled={!preset || missingFields.length > 0 || status === 'testing'}
          className="flex items-center gap-2 rounded-[10px] bg-accent px-4 py-2.5 text-[14px] text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
        >
          {status === 'testing' && <Loader2 size={15} className="animate-spin" />}
          测试云端接口
        </button>
        {preset?.docs && (
          <button
            type="button"
            onClick={() => void openUrl(preset.docs)}
            className="flex items-center gap-1.5 text-[13px] text-text-secondary hover:text-accent"
          >
            <ExternalLink size={14} /> 官方接口文档
          </button>
        )}
      </div>
      <p className="text-[11px] leading-5 text-text-tertiary">
        测试会发送短静音验证鉴权与协议，可能消耗 API 额度；测试成功不代表语音准确率测试通过。
        测试使用当前填写内容，实际录音使用“保存并应用”后的配置。
      </p>
      {status === 'success' && (
        <p role="status" className="flex items-center gap-1.5 text-[13px] text-success">
          <CheckCircle2 size={15} /> 接口验证成功{latency !== null ? ` · ${latency} ms` : ''}
        </p>
      )}
      {status === 'error' && (
        <div role="alert" className="text-[13px] leading-6 text-error">
          <p className="flex items-center gap-1.5">
            <XCircle size={15} /> 接口验证失败
          </p>
          <p className="mt-1 break-all">{error}</p>
        </div>
      )}
    </section>
  )
}
