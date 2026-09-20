import { useEffect, useState } from 'react'
import { CheckCircle2, Cpu, Keyboard, Loader2, Mic, XCircle } from 'lucide-react'
import { getLocalLlmPaths, getSenseVoicePaths, listAudioInputDevices } from '../../lib/tauri'
import { useAppStore } from '../../stores/appStore'

interface Readiness {
  microphone: boolean
  senseVoice: boolean
  localLlm: boolean
}

export function QuickTestStep() {
  const config = useAppStore((state) => state.config)
  const pipelineState = useAppStore((state) => state.pipelineState)
  const [readiness, setReadiness] = useState<Readiness | null>(null)
  const [sample, setSample] = useState('')

  useEffect(() => {
    let cancelled = false
    Promise.allSettled([
      listAudioInputDevices(),
      getSenseVoicePaths(
        config.sensevoice_use_custom_dir ? config.sensevoice_model_dir : undefined,
      ),
      getLocalLlmPaths(config.local_llm_model_dir),
    ]).then(([microphones, senseVoice, localLlm]) => {
      if (cancelled) return
      setReadiness({
        microphone: microphones.status === 'fulfilled' && microphones.value.length > 0,
        senseVoice: senseVoice.status === 'fulfilled' && senseVoice.value.ready,
        localLlm: localLlm.status === 'fulfilled' && localLlm.value.default_model_ready,
      })
    })
    return () => {
      cancelled = true
    }
  }, [config.local_llm_model_dir, config.sensevoice_model_dir, config.sensevoice_use_custom_dir])

  const recognized = sample.trim().length > 0

  return (
    <div className="space-y-4 py-1">
      <div className="grid grid-cols-3 gap-2">
        <Status label="麦克风" ready={readiness?.microphone} loading={!readiness} icon={Mic} />
        <Status label="离线识别" ready={readiness?.senseVoice} loading={!readiness} icon={Cpu} />
        <Status
          label="本地润色"
          ready={readiness?.localLlm}
          loading={!readiness}
          icon={CheckCircle2}
          optional
        />
      </div>

      <p className="text-[11px] leading-6 text-text-tertiary">
        组件就绪不代表已激活。本页真实录音同样计入累计 200 次或 20
        分钟试用额度；文字润色需激活后开启。手动输入文字不能验证语音识别链路。
      </p>
      <div className="rounded-[12px] border border-border bg-bg-secondary p-4">
        <div className="mb-3 flex items-center gap-2 text-[12px] text-text-secondary">
          <Keyboard size={14} />
          <span>
            先点击下方输入框，再按住{' '}
            <kbd className="rounded bg-bg-tertiary px-1.5 py-0.5 font-mono">{config.hotkey}</kbd>{' '}
            说一句话，松开后文字应出现在框内。
          </span>
        </div>
        <textarea
          value={sample}
          onChange={(event) => setSample(event.target.value)}
          placeholder="点击这里，然后按住快捷键说：你好，PopSpeak。"
          className="h-24 w-full resize-none rounded-[9px] border border-border bg-bg-primary px-3 py-2 text-[13px] text-text-primary outline-none focus:border-accent"
        />
        <div className="mt-2 flex items-center justify-between text-[11px]">
          <span className="text-text-tertiary">当前状态：{stateLabel(pipelineState)}</span>
          {recognized && <span className="text-success">输入框已有文本，请核对识别结果</span>}
        </div>
      </div>

      {readiness && !readiness.senseVoice && (
        <p className="text-[11px] leading-relaxed text-error">
          默认离线识别组件未就绪。请重新解压完整离线版，或在“设置 → 语音识别”中修复组件后再测试。
        </p>
      )}
      {readiness && !readiness.localLlm && (
        <p className="text-[11px] leading-relaxed text-text-tertiary">
          本地润色模型未就绪不影响语音识别；结果会直接输出原始转写，可稍后在设置中安装。
        </p>
      )}
    </div>
  )
}

function Status({
  label,
  ready,
  loading,
  icon: Icon,
  optional = false,
}: {
  label: string
  ready?: boolean
  loading: boolean
  icon: React.ComponentType<{ size?: number; className?: string }>
  optional?: boolean
}) {
  return (
    <div className="rounded-[9px] border border-border bg-bg-secondary p-3 text-center">
      {loading ? (
        <Loader2 size={16} className="mx-auto animate-spin text-text-tertiary" />
      ) : ready ? (
        <Icon size={16} className="mx-auto text-success" />
      ) : (
        <XCircle
          size={16}
          className={`mx-auto ${optional ? 'text-text-tertiary' : 'text-error'}`}
        />
      )}
      <p className="mt-1.5 text-[11px] text-text-secondary">{label}</p>
      <p className="mt-0.5 text-[10px] text-text-tertiary">
        {loading ? '检测中' : ready ? '已就绪' : optional ? '可选' : '不可用'}
      </p>
    </div>
  )
}

function stateLabel(state: string) {
  const labels: Record<string, string> = {
    idle: '等待测试',
    recording: '正在录音',
    transcribing: '本地识别中',
    polishing: '本地润色中',
    outputting: '正在写入',
  }
  return labels[state] || state
}
