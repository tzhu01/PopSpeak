import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { open } from '@tauri-apps/plugin-dialog'
import { useTranslation } from 'react-i18next'
import {
  Check,
  ChevronDown,
  Copy,
  FileImage,
  ImagePlus,
  RefreshCw,
  RotateCcw,
  Type,
} from 'lucide-react'
import { recognizeImage } from '../../lib/tauri'
import { formatEditorText, type EditorPreferences, type LayoutMode } from '../Editor/preferences'

const SUPPORTED_EXTENSIONS = /\.(png|jpe?g|bmp|tiff?)$/i
const FORMAT_OPTIONS: { mode: LayoutMode; labelKey: string }[] = [
  { mode: 'auto', labelKey: 'imageRecognition.layoutAuto' },
  { mode: 'single-line', labelKey: 'imageRecognition.layoutSingleLine' },
  { mode: 'multi-line', labelKey: 'imageRecognition.layoutBySentence' },
]

const IMAGE_FORMAT_PREFERENCES: EditorPreferences = {
  autoCopy: false,
  copyOnClose: false,
  highlightOnCopy: false,
  alwaysOnTop: false,
  layoutMode: 'auto',
  removeSpaces: false,
  removePunctuation: false,
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() || path
}

export function ImageRecognition() {
  const { t } = useTranslation()
  const [filePath, setFilePath] = useState('')
  const [pendingName, setPendingName] = useState('')
  const [recognizedText, setRecognizedText] = useState('')
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const [dragging, setDragging] = useState(false)
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState('')
  const [layoutMode, setLayoutMode] = useState<LayoutMode>('auto')
  const [showFormatMenu, setShowFormatMenu] = useState(false)
  const operationRef = useRef(false)
  const editVersionRef = useRef(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const hasUncopiedEdits = text !== recognizedText && !!text.trim() && !copied

  const runRecognition = useCallback(
    async (path: string) => {
      if (operationRef.current) return
      if (!SUPPORTED_EXTENSIONS.test(path)) {
        setError(t('imageRecognition.unsupportedFormat'))
        return
      }
      if (hasUncopiedEdits && !window.confirm(t('imageRecognition.confirmReplace'))) {
        return
      }

      operationRef.current = true
      setBusy(true)
      setPendingName(fileName(path))
      setShowFormatMenu(false)
      setError('')
      try {
        const result = await recognizeImage(path)
        setFilePath(path)
        setRecognizedText(result)
        editVersionRef.current += 1
        setText(result)
        setLayoutMode('auto')
        setCopied(false)
        textareaRef.current?.focus()
      } catch (reason) {
        setError(String(reason))
      } finally {
        operationRef.current = false
        setBusy(false)
        setPendingName('')
      }
    },
    [hasUncopiedEdits, t],
  )

  useEffect(() => {
    if (!hasUncopiedEdits) return
    const onHashChange = (event: HashChangeEvent) => {
      if (window.location.hash === '#/image') return
      if (window.confirm(t('imageRecognition.confirmLeave'))) return
      event.stopImmediatePropagation()
      window.history.replaceState(window.history.state, '', '#/image')
    }
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = ''
    }
    window.addEventListener('hashchange', onHashChange, { capture: true })
    window.addEventListener('beforeunload', onBeforeUnload)
    return () => {
      window.removeEventListener('hashchange', onHashChange, { capture: true })
      window.removeEventListener('beforeunload', onBeforeUnload)
    }
  }, [hasUncopiedEdits, t])

  useEffect(() => {
    if (!hasUncopiedEdits) return
    let active = true
    const listener = getCurrentWindow()
      .onCloseRequested((event) => {
        if (active && !window.confirm(t('imageRecognition.confirmLeave'))) {
          event.preventDefault()
        }
      })
      .catch((reason) => {
        console.warn('Unable to guard unsaved image recognition edits:', reason)
        return null
      })
    return () => {
      active = false
      void listener.then((unlisten) => unlisten?.())
    }
  }, [hasUncopiedEdits, t])

  useEffect(() => {
    let active = true
    const listener = getCurrentWindow()
      .onDragDropEvent(({ payload }) => {
        if (!active) return
        if (payload.type === 'enter') setDragging(true)
        if (payload.type === 'leave') setDragging(false)
        if (payload.type === 'drop') {
          setDragging(false)
          const path = payload.paths[0]
          if (path) void runRecognition(path)
        }
      })
      .catch((reason) => {
        console.warn('Unable to listen for image drops:', reason)
        return null
      })
    return () => {
      active = false
      void listener.then((unlisten) => unlisten?.())
    }
  }, [runRecognition])

  async function chooseImage() {
    if (operationRef.current) return
    try {
      const selected = await open({
        title: t('imageRecognition.chooseImage'),
        multiple: false,
        directory: false,
        filters: [
          {
            name: t('imageRecognition.imageFiles'),
            extensions: ['png', 'jpg', 'jpeg', 'bmp', 'tif', 'tiff'],
          },
        ],
      })
      if (typeof selected === 'string') await runRecognition(selected)
    } catch (reason) {
      setError(String(reason))
    }
  }

  async function copyResult() {
    if (!text) return
    const currentText = text
    const version = editVersionRef.current
    try {
      await writeText(currentText)
      // The user may have edited again while the clipboard write was pending.
      setCopied(editVersionRef.current === version)
      setError('')
    } catch (reason) {
      setError(`${t('imageRecognition.copyFailed')}${String(reason)}`)
    }
  }

  function applyFormat(mode: LayoutMode) {
    editVersionRef.current += 1
    setText((current) =>
      formatEditorText(current, { ...IMAGE_FORMAT_PREFERENCES, layoutMode: mode }),
    )
    setLayoutMode(mode)
    setShowFormatMenu(false)
    setCopied(false)
  }

  const edited = text !== recognizedText
  const currentName = pendingName || (filePath ? fileName(filePath) : '')

  return (
    <div className="mx-auto flex w-full max-w-[980px] flex-col gap-4 px-6 py-6">
      <div>
        <p className="brand-kicker">IMAGE TO TEXT · LOCAL OCR</p>
        <h2 className="mt-1 text-[24px] font-semibold tracking-tight text-text-primary">
          {t('imageRecognition.title')}
        </h2>
        <p className="mt-1 text-[12px] leading-5 text-text-secondary">
          {t('imageRecognition.description')}
        </p>
      </div>

      <section
        aria-label={t('imageRecognition.source')}
        className={`rounded-[15px] border border-dashed px-4 py-4 transition-colors ${dragging ? 'border-accent bg-accent-light' : 'border-border bg-bg-elevated'}`}
      >
        <div className="flex flex-wrap items-center gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[11px] bg-accent-light text-accent">
            {currentName ? <FileImage size={20} /> : <ImagePlus size={20} />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-medium text-text-primary" title={filePath}>
              {dragging
                ? t('imageRecognition.dropHere')
                : currentName || t('imageRecognition.noImage')}
            </p>
            <p className="mt-0.5 text-[11px] text-text-tertiary">
              {busy ? t('imageRecognition.recognizing') : t('imageRecognition.sourceHint')}
            </p>
          </div>
          <button
            type="button"
            onClick={() => void chooseImage()}
            disabled={busy}
            className="inline-flex cursor-pointer items-center gap-2 rounded-[9px] border-0 bg-accent px-3.5 py-2 text-[12px] font-medium text-white hover:bg-accent-hover disabled:cursor-wait disabled:opacity-60"
          >
            <ImagePlus size={15} />
            {t('imageRecognition.chooseImage')}
          </button>
        </div>
      </section>

      {error && (
        <div
          role="alert"
          className="rounded-[9px] border border-error/20 bg-error/10 px-3 py-2 text-[12px] text-error"
        >
          {error}
        </div>
      )}

      <section
        className="min-h-[350px] overflow-hidden rounded-[15px] border border-border bg-bg-elevated shadow-sm"
        aria-label={t('imageRecognition.result')}
      >
        <div className="relative flex flex-wrap items-center gap-1 border-b border-border px-3 py-2">
          <span className="mr-auto px-2 text-[13px] font-semibold text-text-primary">
            {t('imageRecognition.result')}
          </span>
          <button
            type="button"
            onClick={() => void copyResult()}
            disabled={!text}
            className="inline-flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] font-medium text-accent hover:bg-accent-light disabled:opacity-40"
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
            {copied ? t('imageRecognition.copied') : t('imageRecognition.copy')}
          </button>
          <button
            type="button"
            aria-expanded={showFormatMenu}
            onClick={() => setShowFormatMenu((visible) => !visible)}
            disabled={!text}
            className="inline-flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] text-text-secondary hover:bg-bg-secondary disabled:opacity-40"
          >
            <Type size={14} />
            {t('imageRecognition.format')}
            <ChevronDown size={12} />
          </button>
          <button
            type="button"
            onClick={() => void runRecognition(filePath)}
            disabled={!filePath || busy}
            className="inline-flex items-center gap-1.5 rounded-[8px] border-0 bg-transparent px-2.5 py-1.5 text-[12px] text-text-secondary hover:bg-bg-secondary disabled:opacity-40"
          >
            <RefreshCw size={14} className={busy ? 'animate-spin' : ''} />
            {t('imageRecognition.retry')}
          </button>
          {showFormatMenu && (
            <div className="absolute top-10 right-3 z-20 w-44 rounded-[10px] border border-border bg-bg-elevated p-1.5 shadow-xl">
              {FORMAT_OPTIONS.map(({ mode, labelKey }) => (
                <button
                  type="button"
                  key={mode}
                  onClick={() => applyFormat(mode)}
                  className="flex w-full items-center gap-2 rounded-[7px] border-0 bg-transparent px-2 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
                >
                  <span className="w-3 text-accent">{layoutMode === mode ? '✓' : ''}</span>
                  {t(labelKey)}
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="p-3">
          <textarea
            ref={textareaRef}
            aria-label={t('imageRecognition.editResult')}
            value={text}
            onChange={(event) => {
              editVersionRef.current += 1
              setText(event.target.value)
              setCopied(false)
            }}
            placeholder={
              busy ? t('imageRecognition.recognizing') : t('imageRecognition.resultPlaceholder')
            }
            className="h-[275px] w-full resize-y select-text rounded-[10px] border border-border bg-bg-primary/40 px-4 py-3 text-[14px] leading-7 text-text-primary outline-none focus:border-accent"
            spellCheck={false}
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t border-border px-4 py-2.5 text-[11px] text-text-tertiary">
          <span>{t('imageRecognition.characterCount', { count: text.length })}</span>
          {edited && <span className="text-accent">{t('imageRecognition.edited')}</span>}
          {!busy && filePath && !text.trim() && <span>{t('imageRecognition.noTextFound')}</span>}
          {edited && (
            <button
              type="button"
              onClick={() => {
                if (hasUncopiedEdits && !window.confirm(t('imageRecognition.confirmRestore')))
                  return
                editVersionRef.current += 1
                setText(recognizedText)
                setCopied(false)
                setLayoutMode('auto')
              }}
              className="ml-auto inline-flex items-center gap-1 rounded-[7px] border-0 bg-transparent px-2 py-1 text-[11px] text-text-secondary hover:bg-bg-secondary"
            >
              <RotateCcw size={12} />
              {t('imageRecognition.restore')}
            </button>
          )}
        </div>
      </section>
      <p className="px-1 text-[11px] leading-5 text-text-tertiary">
        {t('imageRecognition.retentionHint')}
      </p>
    </div>
  )
}
