import { useEffect } from 'react'
import { Cpu, HardDrive, Languages, ShieldCheck, Zap } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'

export function ModeSelectStep() {
  const setOnboardingMode = useAppStore((state) => state.setOnboardingMode)
  const updateConfig = useAppStore((state) => state.updateConfig)

  useEffect(() => {
    setOnboardingMode('local')
    updateConfig({
      stt_provider: 'sensevoice',
      stt_api_key: '',
      stt_language: 'multi',
      sensevoice_language: 'auto',
      llm_provider: 'local-llama',
      llm_api_key: '',
      llm_base_url: 'http://127.0.0.1:11434/v1',
      llm_model: 'qwen2.5-0.5b-instruct',
      polish_enabled: false,
      hotkey_mode: 'hold',
      output_mode: 'clipboard',
      ui_language: 'zh',
    })
  }, [setOnboardingMode, updateConfig])

  return (
    <div className="space-y-4 py-2">
      <div className="rounded-[12px] border border-accent bg-accent/10 p-5">
        <div className="flex items-start gap-3">
          <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-[10px] bg-accent/15 text-accent">
            <HardDrive size={21} />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-[15px] font-semibold text-text-primary">Windows 本地离线方案</h3>
              <span className="rounded-full bg-accent px-2 py-0.5 text-[10px] font-medium text-white">
                默认
              </span>
            </div>
            <p className="mt-1 text-[12px] leading-relaxed text-text-secondary">
              默认离线识别可直接试用，累计 200 次或 20
              分钟任一用完后需激活；精确离线识别和多语种离线识别需激活后使用。
            </p>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Feature icon={Cpu} title="普通电脑可用" detail="无需独立显卡，打开即可离线输入" />
        <Feature
          icon={ShieldCheck}
          title="真正离线"
          detail="默认识别在本机完成；激活后可开启本地润色和纠错"
        />
        <Feature icon={Zap} title="按下就录" detail="按住快捷键说话，松开后自动转成文字" />
        <Feature icon={Languages} title="中文优先" detail="简体输出与粤语选项；热词和纠错需激活" />
      </div>

      <p className="text-center text-[11px] text-text-tertiary">
        离线试用无需登录或 API Key。关注公众号领取本机激活码后解除试用限制；云端 API 另行计费。
      </p>
    </div>
  )
}

function Feature({
  icon: Icon,
  title,
  detail,
}: {
  icon: React.ComponentType<{ size?: number; className?: string }>
  title: string
  detail: string
}) {
  return (
    <div className="rounded-[10px] border border-border bg-bg-secondary p-3">
      <Icon size={16} className="text-accent" />
      <p className="mt-2 text-[12px] font-medium text-text-primary">{title}</p>
      <p className="mt-1 text-[11px] leading-relaxed text-text-tertiary">{detail}</p>
    </div>
  )
}
