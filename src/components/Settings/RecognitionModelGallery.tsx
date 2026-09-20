import { Check, Cloud, Download, HardDrive } from 'lucide-react'
import { ModelBrandMark, type ModelBrand } from './ModelBrandMark'

export type RecognitionChoice =
  | 'sensevoice'
  | 'funasr-nano'
  | 'qwen3-asr-1.7b'
  | 'cohere-transcribe-03-2026'
  | 'nemotron-3.5-asr-streaming-0.6b'
  | 'parakeet-unified-en-0.6b'
  | 'whisper-tiny'
  | 'whisper-base'
  | 'whisper-small'
  | 'whisper-turbo'
  | 'volcengine-seedasr'
  | 'custom-whisper'

export type ModelAvailability = Partial<Record<RecognitionChoice, boolean>>

const choices: {
  id: RecognitionChoice
  title: string
  brand: ModelBrand
  description: string
  detail: string
  tags: string[]
  recommended?: boolean
  cloud?: boolean
}[] = [
  {
    id: 'sensevoice',
    title: 'SenseVoice Small',
    brand: 'sensevoice',
    description: '日常中文输入首选，也支持粤语、英文、日语和韩语。',
    detail: '约 239 MB',
    tags: ['INT8', 'CPU 离线'],
    recommended: true,
  },
  {
    id: 'funasr-nano',
    title: 'Fun-ASR-Nano',
    brand: 'funasr',
    description: '中文识别与识别阶段热词；常驻进程复用模型，支持按需下载。',
    detail: '约 911 MB',
    tags: ['GGUF', '识别热词', 'CPU 离线'],
  },
  {
    id: 'whisper-tiny',
    title: 'Whisper Tiny',
    brand: 'whisper',
    description: '轻量多语种模型，占用较小；复杂语音建议选择更大模型。',
    detail: '约 77 MB',
    tags: ['多语种', 'CPU 离线'],
  },
  {
    id: 'qwen3-asr-1.7b',
    title: 'Qwen3-ASR 1.7B',
    brand: 'qwen',
    description:
      '通义多语种语音识别，支持中文与英文混合输入。CPU 可运行，耗时和内存高于 SenseVoice。',
    detail: '约 1.52 GB · 按需下载',
    tags: ['Q5_K_M', '中文 / 多语种', 'CPU 离线'],
  },
  {
    id: 'cohere-transcribe-03-2026',
    title: 'Cohere Transcribe 03-2026',
    brand: 'cohere',
    description: '支持中文等 14 种语言的离线转写模型；建议较大内存电脑，首次加载需要等待。',
    detail: '约 1.77 GB · 按需下载',
    tags: ['Q5_K_M', '14 种语言', 'CPU 离线'],
  },
  {
    id: 'nemotron-3.5-asr-streaming-0.6b',
    title: 'Nemotron 3.5 ASR Streaming 0.6B',
    brand: 'nvidia',
    description: '多语种语音模型。本应用先按整段离线识别接入，不承诺截图中的 80 ms 流式延迟。',
    detail: '约 751 MB · 按需下载',
    tags: ['Q8_0', '多语种', 'CPU 离线'],
  },
  {
    id: 'parakeet-unified-en-0.6b',
    title: 'Parakeet Unified EN 0.6B',
    brand: 'nvidia',
    description: '英语专用模型，不适合中文输入。本应用使用本地 CPU 整段识别。',
    detail: '约 731 MB · 按需下载',
    tags: ['Q8_0', '英语专用', 'CPU 离线'],
  },
  {
    id: 'whisper-base',
    title: 'Whisper Base',
    brand: 'whisper',
    description: '比 Tiny 容量更大，在模型体积和多语种识别之间平衡。',
    detail: '约 148 MB',
    tags: ['多语种', 'CPU 离线'],
  },
  {
    id: 'whisper-small',
    title: 'Whisper Small',
    brand: 'whisper',
    description: '量化多语种模型；CPU 耗时通常高于 Tiny / Base。',
    detail: '约 190 MB',
    tags: ['Q5_1', '多语种', '按需下载'],
  },
  {
    id: 'whisper-turbo',
    title: 'Whisper Large v3 Turbo',
    brand: 'whisper',
    description: '量化大模型，需要更多内存；普通 CPU 请先试用再选择。',
    detail: '约 574 MB',
    tags: ['Q5_0', '多语种', '按需下载'],
  },
  {
    id: 'volcengine-seedasr',
    title: '豆包 SeedASR 2.0',
    brand: 'volcengine',
    description: '使用自己的火山引擎账号，音频上传至服务商进行识别。',
    detail: '自备 API 凭证',
    tags: ['云端', '自费 API'],
    cloud: true,
  },
  {
    id: 'custom-whisper',
    title: '自定义云端接口',
    brand: 'custom',
    description: '接入字节、阿里、腾讯、讯飞、百度等已支持的接口。',
    detail: '自备服务商账号',
    tags: ['云端', '自费 API'],
    cloud: true,
  },
]

export function RecognitionModelGallery({
  selected,
  onChoose,
  availability = {},
}: {
  selected: RecognitionChoice
  onChoose: (choice: RecognitionChoice) => void
  availability?: ModelAvailability
}) {
  return (
    <section aria-labelledby="recognition-model-title" className="model-gallery space-y-4">
      <div>
        <h3 id="recognition-model-title" className="text-[17px] font-semibold text-text-primary">
          语音转文字模型
        </h3>
        <p className="mt-1 text-[12px] leading-5 text-text-secondary">
          选一个模型即可开始。离线模型无需 API 费用，软件功能按当前激活状态开放。
        </p>
      </div>
      <div
        className="grid gap-3"
        style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 280px), 1fr))' }}
      >
        {choices.map(({ id, title, brand, description, detail, tags, recommended, cloud }) => {
          const active = selected === id
          const ready = availability[id]
          return (
            <button
              key={id}
              type="button"
              onClick={() => onChoose(id)}
              aria-pressed={active}
              className={`model-choice w-full rounded-[14px] border border-border p-4 text-left transition-colors ${active ? 'model-choice-active' : ''}`}
            >
              <span className="flex items-start gap-3">
                <ModelBrandMark brand={brand} />
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1">
                    <span className="text-[14px] font-semibold leading-5 text-text-primary">
                      {title}
                    </span>
                    {active && (
                      <span className="flex shrink-0 items-center gap-1 text-[11px] font-medium text-accent">
                        <Check size={12} />
                        已选择
                      </span>
                    )}
                  </span>
                  <span className="mt-2 flex flex-wrap items-center gap-1.5">
                    {recommended && (
                      <span className="rounded-md bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                        推荐
                      </span>
                    )}
                    {tags.map((tag) => (
                      <span
                        key={tag}
                        className="rounded-md bg-bg-secondary px-1.5 py-0.5 text-[10px] text-text-secondary"
                      >
                        {tag}
                      </span>
                    ))}
                  </span>
                  <span className="mt-2 block text-[12px] leading-5 text-text-secondary">
                    {description}
                  </span>
                  <span className="mt-3 flex flex-wrap items-center justify-between gap-2 text-[11px] text-text-tertiary">
                    <span>{detail}</span>
                    <span className={`flex items-center gap-1 ${ready ? 'text-success' : ''}`}>
                      {cloud ? (
                        <>
                          <Cloud size={12} />
                          需联网
                        </>
                      ) : ready === true ? (
                        <>
                          <Check size={12} />
                          已安装
                        </>
                      ) : ready === false ? (
                        <>
                          <Download size={12} />
                          待下载
                        </>
                      ) : (
                        <>
                          <HardDrive size={12} />
                          本地模型
                        </>
                      )}
                    </span>
                  </span>
                </span>
              </span>
            </button>
          )
        })}
      </div>
    </section>
  )
}
