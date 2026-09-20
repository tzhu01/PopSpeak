import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Square, X, GripHorizontal, Copy, Check } from 'lucide-react'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { useAppStore } from '../../stores/appStore'
import { abortRecording } from '../../lib/tauri'
import { DurationTimer } from './DurationTimer'
import { Waveform } from './Waveform'

export function CapsulePop({ onCollapse, onStop }: { onCollapse: () => void; onStop: () => void }) {
  const state = useAppStore((s) => s.pipelineState)
  const preview = useAppStore((s) => s.previewTranscript)
  const partial = useAppStore((s) => s.partialTranscript)
  const final = useAppStore((s) => s.finalTranscript)
  const polished = useAppStore((s) => s.polishedText)
  const resolved = useAppStore((s) => s.resolvedTranscript)
  const body = useRef<HTMLDivElement>(null)
  const [copyState, setCopyState] = useState<'idle' | 'copying' | 'success' | 'error'>('idle')
  const copyReset = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const providerText = final ? final + partial : partial
  const text = resolved || polished || providerText || preview
  const recording = state === 'recording'
  const label = recording
    ? '录入中'
    : state === 'transcribing'
      ? '确认完整结果'
      : state === 'polishing'
        ? '润色中'
        : '正在输出'

  useEffect(() => {
    if (body.current) body.current.scrollTop = body.current.scrollHeight
  }, [text])
  useEffect(() => () => clearTimeout(copyReset.current), [])

  const copyVisibleText = async () => {
    if (!text.trim() || copyState === 'copying') return
    clearTimeout(copyReset.current)
    setCopyState('copying')
    try {
      // Snapshot only the visible transcript. Recording and output are untouched.
      await writeText(text)
      setCopyState('success')
    } catch {
      setCopyState('error')
    }
    copyReset.current = setTimeout(() => setCopyState('idle'), 2000)
  }

  return (
    <section
      style={{ maxWidth: 'calc(100vw - 24px)', maxHeight: 'calc(100vh - 24px)' }}
      aria-label="POP 录音预览"
      className="flex h-[248px] w-[320px] flex-col overflow-hidden rounded-[22px] border border-white/10 bg-[#19191d] text-white shadow-lg"
    >
      <header
        className="flex h-8 shrink-0 cursor-move items-center justify-between px-4 text-[10px] tracking-wide text-white/45"
        data-tauri-drag-region
      >
        <span className="pointer-events-none">
          PopSpeak · {recording ? '文字预览' : '语音输入'}
        </span>
        <GripHorizontal size={14} className="pointer-events-none" />
      </header>
      <div
        ref={body}
        className="min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap break-words px-4 pb-3 text-[16px] leading-[1.7] select-text"
        aria-live="polite"
        aria-atomic="true"
      >
        {text || (
          <span className="text-white/45">
            {recording ? '请说话，本地近似预览会实时刷新…' : '正在处理，请稍候…'}
          </span>
        )}
      </div>
      <div
        role="status"
        className={`px-4 pb-2 text-[10px] ${copyState === 'error' ? 'text-rose-300' : 'text-white/40'}`}
      >
        {copyState === 'success'
          ? '已复制当前文字，录音不受影响'
          : copyState === 'error'
            ? '复制失败，请重试'
            : recording
              ? '本地近似预览可能变化，以结束录音后的完整结果为准'
              : '最终文本将按你的输出设置送出'}
      </div>
      <footer className="flex h-11 shrink-0 items-center gap-2 border-t border-white/10 px-3">
        {state !== 'outputting' && (
          <button
            type="button"
            className="rounded-md p-1.5 text-white/60 hover:bg-white/10 hover:text-white"
            aria-label="取消录音"
            title="取消本段，不输出"
            onClick={() => {
              void abortRecording().catch(console.error)
            }}
          >
            <X size={14} />
          </button>
        )}
        <button
          type="button"
          className="rounded-md bg-white/5 p-1.5 text-white/70 hover:bg-white/15"
          aria-label="收起预览"
          title="收起预览，继续录音"
          onClick={onCollapse}
        >
          <ChevronDown size={14} />
        </button>
        <DurationTimer />
        <span
          className={`h-1.5 w-1.5 rounded-full ${recording ? 'bg-emerald-400' : 'bg-amber-300'}`}
        />
        <span className="whitespace-nowrap text-xs text-white/85">{label}</span>
        <div className="flex-1" />
        <button
          type="button"
          aria-label="复制当前文字"
          title="复制当前可见文字，不结束录音"
          disabled={!text.trim() || copyState === 'copying'}
          onClick={() => void copyVisibleText()}
          className="shrink-0 rounded-md p-1.5 text-white/70 hover:bg-white/15 disabled:cursor-not-allowed disabled:opacity-30"
        >
          {copyState === 'success' ? <Check size={14} /> : <Copy size={14} />}
        </button>
        {recording && (
          <div className="max-w-6 overflow-hidden">
            <Waveform />
          </div>
        )}
        {recording && (
          <button
            type="button"
            className="rounded-md bg-white/10 p-1.5 hover:bg-white/20"
            aria-label="结束录音"
            title="结束并识别"
            onClick={onStop}
          >
            <Square size={12} fill="currentColor" />
          </button>
        )}
      </footer>
    </section>
  )
}
