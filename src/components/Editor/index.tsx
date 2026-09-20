import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { BookPlus, Check, ChevronDown, Copy, Languages, Minus, Pin, Type, X } from 'lucide-react'
import { addDictionaryEntry, hideEditorWindow, updateHistoryEntry } from '../../lib/tauri'
import { changedSpan } from '../../lib/correction'
import {
  formatEditorText,
  loadEditorPreferences,
  saveEditorPreferences,
  type EditorPreferences,
} from './preferences'

interface EditorPayload {
  id: number
  text: string
  raw_text?: string
  app_name: string
  language?: string
  auto_hide_enabled?: boolean
  auto_hide_seconds?: number
}

const DEFAULT_AUTO_HIDE_SECONDS = 5

function normalizeAutoHideSeconds(value?: number) {
  if (!Number.isFinite(value)) return DEFAULT_AUTO_HIDE_SECONDS
  return Math.min(60, Math.max(3, Math.round(value as number)))
}

function displayLanguage(language?: string) {
  const labels: Record<string, string> = {
    auto: '自动检测',
    multi: '自动 / 多语种',
    zh: '简体中文',
    'zh-CN': '简体中文',
    en: 'English',
    ja: '日本語',
    ko: '한국어',
    yue: '粤语',
  }
  return labels[language ?? ''] ?? language ?? '自动检测'
}

async function setEditorAlwaysOnTop(enabled: boolean) {
  try {
    await getCurrentWindow().setAlwaysOnTop(enabled)
  } catch (error) {
    console.warn('Unable to change editor topmost state:', error)
  }
}

async function minimizeEditorWindow() {
  try {
    await getCurrentWindow().minimize()
  } catch (error) {
    console.warn('Unable to minimize editor window:', error)
  }
}

export function EditorApp() {
  const [payload, setPayload] = useState<EditorPayload | null>(null)
  const [text, setText] = useState('')
  const [copied, setCopied] = useState(false)
  const [saved, setSaved] = useState(false)
  const [remaining, setRemaining] = useState(DEFAULT_AUTO_HIDE_SECONDS)
  const [paused, setPaused] = useState(false)
  const [showCorrection, setShowCorrection] = useState(false)
  const [showFormatMenu, setShowFormatMenu] = useState(false)
  const [correctionFrom, setCorrectionFrom] = useState('')
  const [correctionTo, setCorrectionTo] = useState('')
  const [pronunciation, setPronunciation] = useState('')
  const [correctionSaved, setCorrectionSaved] = useState(false)
  const [preferences, setPreferences] = useState(loadEditorPreferences)
  const preferencesRef = useRef(preferences)
  const savedIdRef = useRef<number | null>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const writeClipboard = async (value: string, highlight = false) => {
    if (!value) return
    try {
      await navigator.clipboard.writeText(value)
      if (highlight) {
        textareaRef.current?.focus()
        textareaRef.current?.select()
      }
      setCopied(true)
      setTimeout(() => setCopied(false), 1200)
    } catch (error) {
      console.error('Copy failed:', error)
    }
  }

  useEffect(() => {
    void setEditorAlwaysOnTop(preferencesRef.current.alwaysOnTop)
    const unlistenPromise = listen<EditorPayload>('editor:show', (event) => {
      const activePreferences = preferencesRef.current
      const nextPayload = {
        ...event.payload,
        auto_hide_enabled: event.payload.auto_hide_enabled ?? true,
        auto_hide_seconds: normalizeAutoHideSeconds(event.payload.auto_hide_seconds),
      }
      const formattedText = formatEditorText(event.payload.text, activePreferences)
      setPayload(nextPayload)
      setText(formattedText)
      setCopied(false)
      setSaved(false)
      setPaused(false)
      setRemaining(nextPayload.auto_hide_seconds)
      setShowCorrection(false)
      setShowFormatMenu(false)
      setCorrectionFrom('')
      setCorrectionTo('')
      setPronunciation('')
      setCorrectionSaved(false)
      savedIdRef.current = event.payload.id
      void setEditorAlwaysOnTop(activePreferences.alwaysOnTop)
      if (activePreferences.autoCopy) {
        void writeClipboard(formattedText, activePreferences.highlightOnCopy)
      }
    })
    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(() => {})
    }
    // The event listener must be installed once; preferencesRef always exposes
    // the latest persisted choices without recreating the native listener.
  }, [])

  useEffect(() => {
    if (paused || !payload || payload.auto_hide_enabled === false) {
      if (timerRef.current) clearInterval(timerRef.current)
      return
    }
    const autoHideMs = normalizeAutoHideSeconds(payload.auto_hide_seconds) * 1000
    const start = Date.now()
    timerRef.current = setInterval(() => {
      const elapsed = Date.now() - start
      const left = Math.max(0, autoHideMs - elapsed)
      setRemaining(Math.ceil(left / 1000))
      if (left <= 0) {
        if (timerRef.current) clearInterval(timerRef.current)
        void handleClose()
      }
    }, 100)
    return () => {
      if (timerRef.current) clearInterval(timerRef.current)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paused, payload?.id, payload?.auto_hide_enabled, payload?.auto_hide_seconds])

  const pauseTimer = () => setPaused(true)

  const updatePreferences = (patch: Partial<EditorPreferences>, reformat = false) => {
    const next = { ...preferencesRef.current, ...patch }
    preferencesRef.current = next
    setPreferences(next)
    saveEditorPreferences(next)
    if (reformat) setText((current) => formatEditorText(current, next))
  }

  const toggleTopmost = () => {
    pauseTimer()
    const enabled = !preferences.alwaysOnTop
    updatePreferences({ alwaysOnTop: enabled })
    void setEditorAlwaysOnTop(enabled)
  }

  const openCorrection = () => {
    pauseTimer()
    setShowFormatMenu(false)
    if (!showCorrection && payload && text !== payload.text) {
      const correction = changedSpan(payload.text, text)
      if (correction.from && correction.to) {
        setCorrectionFrom(correction.from)
        setCorrectionTo(correction.to)
      }
    }
    setShowCorrection((visible) => !visible)
  }

  const handleCopy = async () => {
    pauseTimer()
    await writeClipboard(text, preferences.highlightOnCopy)
  }

  const persistIfChanged = async () => {
    if (savedIdRef.current == null || !payload) return
    if (text === payload.text) return
    try {
      await updateHistoryEntry(savedIdRef.current, text)
      setSaved(true)
      setTimeout(() => setSaved(false), 1200)
    } catch (error) {
      console.error('Save failed:', error)
    }
  }

  const handleClose = async () => {
    if (preferencesRef.current.copyOnClose) await writeClipboard(text, false)
    await persistIfChanged()
    setPayload(null)
    await hideEditorWindow()
  }

  const saveCorrection = async () => {
    if (!correctionFrom.trim() || !correctionTo.trim()) return
    await addDictionaryEntry(
      correctionTo.trim(),
      pronunciation.trim() || null,
      correctionFrom.trim(),
    )
    setCorrectionSaved(true)
    setTimeout(() => setCorrectionSaved(false), 1500)
  }

  if (!payload) {
    return <div className="h-full w-full rounded-[14px] border border-border bg-bg-elevated" />
  }

  const layoutLabel = {
    auto: '自动排版',
    'single-line': '单行文本',
    'multi-line': '按句分行',
  }[preferences.layoutMode]

  return (
    <div className="editor-surface flex h-full w-full flex-col overflow-hidden rounded-[14px] border border-border bg-bg-elevated text-text-primary shadow-2xl">
      <header
        className="flex h-11 shrink-0 items-center justify-between border-b border-border px-4 select-none"
        data-tauri-drag-region
      >
        <div className="flex min-w-0 items-center gap-2" data-tauri-drag-region>
          <span className="flex h-6 w-6 items-center justify-center rounded-[7px] bg-accent text-[11px] font-bold text-white">
            P
          </span>
          <span className="truncate text-[13px] font-semibold" data-tauri-drag-region>
            PopSpeak · 识别结果
          </span>
          <span className="truncate text-[11px] text-text-tertiary" data-tauri-drag-region>
            {payload.app_name ? `来自 ${payload.app_name}` : ''}
          </span>
          {payload.auto_hide_enabled !== false && !paused && (
            <span className="text-[10px] text-text-tertiary" data-tauri-drag-region>
              {remaining}s 后关闭
            </span>
          )}
          {payload.auto_hide_enabled !== false && paused && (
            <span className="text-[10px] text-accent" data-tauri-drag-region>
              编辑中
            </span>
          )}
        </div>
        <div className="flex h-full items-center gap-0.5">
          <button
            type="button"
            onClick={toggleTopmost}
            aria-label={preferences.alwaysOnTop ? '取消置顶' : '窗口置顶'}
            title={preferences.alwaysOnTop ? '已置顶，点击取消' : '置顶窗口'}
            className={`flex h-8 w-8 items-center justify-center rounded-[7px] border-0 bg-transparent transition-colors ${preferences.alwaysOnTop ? 'text-accent' : 'text-text-tertiary hover:bg-bg-secondary hover:text-text-primary'}`}
          >
            <Pin size={15} fill={preferences.alwaysOnTop ? 'currentColor' : 'none'} />
          </button>
          <button
            type="button"
            onClick={() => void minimizeEditorWindow()}
            aria-label="最小化"
            title="最小化"
            className="flex h-8 w-8 items-center justify-center rounded-[7px] border-0 bg-transparent text-text-tertiary hover:bg-bg-secondary hover:text-text-primary"
          >
            <Minus size={15} />
          </button>
          <button
            type="button"
            onClick={() => void handleClose()}
            aria-label="关闭"
            title="关闭"
            className="flex h-8 w-8 items-center justify-center rounded-[7px] border-0 bg-transparent text-text-tertiary hover:bg-accent hover:text-white"
          >
            <X size={15} />
          </button>
        </div>
      </header>

      <div className="relative flex h-11 shrink-0 items-center gap-1 border-b border-border px-3">
        <button
          type="button"
          onClick={() => void handleCopy()}
          className="flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] font-medium text-accent hover:bg-accent-light"
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          {copied ? '已复制' : '复制'}
        </button>
        <button
          type="button"
          aria-expanded={showFormatMenu}
          onClick={() => {
            pauseTimer()
            setShowCorrection(false)
            setShowFormatMenu((visible) => !visible)
          }}
          className="flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] text-text-secondary hover:bg-bg-secondary hover:text-text-primary"
        >
          <Type size={14} />
          排版
          <ChevronDown size={12} />
        </button>
        <button
          type="button"
          onClick={openCorrection}
          className="flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] text-text-secondary hover:bg-bg-secondary hover:text-text-primary"
        >
          <BookPlus size={14} />
          纠错
        </button>
        <span className="ml-auto flex items-center gap-1.5 text-[11px] text-text-tertiary">
          <Languages size={13} />
          识别语言：{displayLanguage(payload.language)}
        </span>

        {showFormatMenu && (
          <div className="absolute top-10 left-[82px] z-20 w-52 rounded-[10px] border border-border bg-bg-elevated p-1.5 shadow-xl">
            <div className="px-2 py-1 text-[10px] font-semibold tracking-wide text-text-tertiary">
              排版方式
            </div>
            {(
              [
                ['auto', '自动整理段落'],
                ['single-line', '移除换行'],
                ['multi-line', '按句分行'],
              ] as const
            ).map(([mode, label]) => (
              <button
                type="button"
                key={mode}
                onClick={() => updatePreferences({ layoutMode: mode }, true)}
                className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
              >
                <span className="w-3 text-accent">
                  {preferences.layoutMode === mode ? '✓' : ''}
                </span>
                {label}
              </button>
            ))}
            <div className="my-1 border-t border-border" />
            <button
              type="button"
              onClick={() => updatePreferences({ removeSpaces: !preferences.removeSpaces }, true)}
              className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
            >
              <span className="w-3 text-accent">{preferences.removeSpaces ? '✓' : ''}</span>
              去掉空格
            </button>
            <button
              type="button"
              onClick={() =>
                updatePreferences({ removePunctuation: !preferences.removePunctuation }, true)
              }
              className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
            >
              <span className="w-3 text-accent">{preferences.removePunctuation ? '✓' : ''}</span>
              去除标点
            </button>
            <button
              type="button"
              onClick={() => updatePreferences({ copyOnClose: !preferences.copyOnClose })}
              className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
            >
              <span className="w-3 text-accent">{preferences.copyOnClose ? '✓' : ''}</span>
              关闭时复制
            </button>
            <button
              type="button"
              onClick={() => updatePreferences({ highlightOnCopy: !preferences.highlightOnCopy })}
              className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
            >
              <span className="w-3 text-accent">{preferences.highlightOnCopy ? '✓' : ''}</span>
              复制后选中文本
            </button>
            <p className="m-0 px-2 pt-1.5 text-[10px] leading-4 text-text-tertiary">
              竖排会降低编辑和粘贴兼容性，当前不提供。
            </p>
          </div>
        )}
      </div>

      <main className="min-h-0 flex-1 bg-bg-primary/50 p-3">
        <textarea
          ref={textareaRef}
          aria-label="识别结果"
          value={text}
          onChange={(event) => setText(event.target.value)}
          onFocus={pauseTimer}
          onKeyDown={pauseTimer}
          onMouseDown={pauseTimer}
          className="h-full w-full resize-none rounded-[10px] border border-border bg-bg-elevated px-4 py-3 text-[14px] leading-7 text-text-primary outline-none transition-colors focus:border-accent"
          spellCheck={false}
        />
      </main>

      {showCorrection && (
        <div className="grid shrink-0 grid-cols-[1fr_1fr_1fr_auto] gap-2 border-t border-border bg-bg-secondary/40 px-4 py-2">
          <input
            value={correctionFrom}
            onChange={(event) => setCorrectionFrom(event.target.value)}
            placeholder="错误识别"
            className="min-w-0 rounded-[7px] border border-border bg-bg-primary px-2 py-1.5 text-[12px] outline-none focus:border-accent"
          />
          <input
            value={correctionTo}
            onChange={(event) => setCorrectionTo(event.target.value)}
            placeholder="正确专业词汇"
            className="min-w-0 rounded-[7px] border border-border bg-bg-primary px-2 py-1.5 text-[12px] outline-none focus:border-accent"
          />
          <input
            value={pronunciation}
            onChange={(event) => setPronunciation(event.target.value)}
            placeholder="拼音（可选）"
            className="min-w-0 rounded-[7px] border border-border bg-bg-primary px-2 py-1.5 text-[12px] outline-none focus:border-accent"
          />
          <button
            type="button"
            onClick={() => void saveCorrection()}
            disabled={!correctionFrom.trim() || !correctionTo.trim()}
            className="cursor-pointer rounded-[7px] border-0 bg-accent px-3 py-1.5 text-[12px] text-white disabled:opacity-40"
          >
            {correctionSaved ? '已保存' : '学习'}
          </button>
        </div>
      )}

      <footer className="flex h-11 shrink-0 items-center gap-3 border-t border-border px-4">
        <label className="flex cursor-pointer items-center gap-2 text-[11px] text-text-secondary">
          <input
            type="checkbox"
            checked={preferences.autoCopy}
            onChange={(event) => updatePreferences({ autoCopy: event.target.checked })}
            className="h-3.5 w-3.5 accent-accent"
          />
          识别完成后自动复制
        </label>
        <span className="text-[10px] text-text-tertiary">当前排版：{layoutLabel}</span>
        {saved && <span className="ml-auto text-[11px] text-success">已保存到历史记录</span>}
        {!saved && text !== payload.text && (
          <span className="ml-auto text-[10px] text-text-tertiary">关闭时自动保存修改</span>
        )}
        <button
          type="button"
          onClick={() => void handleClose()}
          className="ml-auto rounded-[8px] border-0 bg-accent px-3 py-1.5 text-[12px] text-white hover:bg-accent-hover"
        >
          完成
        </button>
      </footer>
    </div>
  )
}
