import { AudioLines, Cloud } from 'lucide-react'

export type ModelBrand =
  | 'sensevoice'
  | 'funasr'
  | 'whisper'
  | 'volcengine'
  | 'custom'
  | 'qwen'
  | 'cohere'
  | 'nvidia'

// Local typographic identifiers, not reproductions of a vendor's official logo.
// Names are verified against QwenAudio/SenseVoice, QwenAudio/Fun-ASR and openai/whisper.
const brands = {
  qwen: {
    label: 'Qwen',
    text: 'Q',
    style: 'bg-violet-500/10 text-violet-600 dark:text-violet-300',
  },
  cohere: { label: 'Cohere', text: 'Co', style: 'bg-teal-500/10 text-teal-700 dark:text-teal-300' },
  nvidia: { label: 'NVIDIA', text: 'N', style: 'bg-lime-500/10 text-lime-700 dark:text-lime-300' },
  sensevoice: {
    label: 'SenseVoice',
    text: 'SV',
    style: 'bg-violet-500/10 text-violet-600 dark:text-violet-300',
  },
  funasr: {
    label: 'Fun-ASR',
    text: 'Fun',
    style: 'bg-indigo-500/10 text-indigo-600 dark:text-indigo-300',
  },
  whisper: {
    label: 'OpenAI Whisper',
    text: 'W',
    style: 'bg-slate-500/10 text-slate-700 dark:text-slate-200',
  },
  volcengine: {
    label: '火山引擎',
    text: '',
    style: 'bg-blue-500/10 text-blue-600 dark:text-blue-300',
  },
  custom: {
    label: '自定义接口',
    text: '',
    style: 'bg-teal-500/10 text-teal-600 dark:text-teal-300',
  },
} as const

export function ModelBrandMark({ brand }: { brand: ModelBrand }) {
  const mark = brands[brand]
  return (
    <span
      aria-hidden="true"
      title={mark.label}
      className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] text-[13px] font-bold tracking-tight ${mark.style}`}
    >
      {mark.text || (brand === 'volcengine' ? <AudioLines size={20} /> : <Cloud size={19} />)}
    </span>
  )
}
