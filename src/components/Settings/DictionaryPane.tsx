import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useTranslation } from 'react-i18next'
import { Check, Mic, Pencil, Plus, Trash2, X } from 'lucide-react'
import { useAppStore, type DictionaryEntry } from '../../stores/appStore'
import {
  addDictionaryEntry,
  getDictionary,
  removeDictionaryEntry,
  updateDictionaryEntry,
} from '../../lib/tauri'
import { toast } from '../Toast'
import { selectRecognitionHotwords } from '../../lib/hotwords'
import { STT_PROVIDERS } from '../../lib/constants'
import { useActivationStore } from '../../lib/activation'
import { useRoute } from '../../lib/router'

interface DraftEntry {
  word: string
  pronunciation: string
  correctionFrom: string
}

export function DictionaryPane(_props: { onOpenRecognition?: () => void }) {
  const activated = useActivationStore((s) => s.status?.activated === true && !s.error)
  const { navigate } = useRoute()
  const dictionary = useAppStore((s) => s.dictionary)
  const setDictionary = useAppStore((s) => s.setDictionary)
  const provider = useAppStore((s) => (s.savedConfig ?? s.config).stt_provider)
  const selectedProvider = useAppStore((s) => s.config.stt_provider)
  const configSaving = useAppStore((s) => s.configSaving)
  const configSaveError = useAppStore((s) => s.configSaveError)
  const saveConfig = useAppStore((s) => s.saveConfig)
  const resetConfig = useAppStore((s) => s.resetConfig)
  const hasPendingProvider = selectedProvider !== provider
  const providerLabel = (value: string) =>
    STT_PROVIDERS.find((item) => item.value === value)?.label ?? '自定义服务'
  // Only these two providers receive the vocabulary before decoding. Every
  // other provider still uses the local dictionary after transcription.
  const usesDecoderHotwords = provider === 'funasr-nano' || provider === 'local-whisper'
  const recognitionWords = selectRecognitionHotwords(dictionary.map((entry) => entry.word))
  const recognitionWordSet = new Set(recognitionWords)
  const { t } = useTranslation()
  const [word, setWord] = useState('')
  const [pronunciation, setPronunciation] = useState('')
  const [correctionFrom, setCorrectionFrom] = useState('')
  const [editingId, setEditingId] = useState<number | null>(null)
  const [draft, setDraft] = useState<DraftEntry | null>(null)
  const [busy, setBusy] = useState(false)
  const mutationPending = useRef(false)

  const refresh = useCallback(() => {
    getDictionary()
      .then(setDictionary)
      .catch((error) => console.error('Failed to load dictionary:', error))
  }, [setDictionary])

  useEffect(() => {
    refresh()
    const unlisten = listen<void>('dictionary:updated', refresh)
    return () => {
      unlisten.then((dispose) => dispose()).catch(() => {})
    }
  }, [refresh])

  const handleAdd = async () => {
    if (!activated || mutationPending.current || !word.trim()) return
    mutationPending.current = true
    setBusy(true)
    try {
      await addDictionaryEntry(
        word.trim(),
        pronunciation.trim() || null,
        correctionFrom.trim() || null,
      )
      setWord('')
      setPronunciation('')
      setCorrectionFrom('')
      refresh()
      toast.success(t('dictionary.saved'))
    } catch (e) {
      console.error('Failed to add entry:', e)
      toast.error(t('dictionary.failedToAdd'))
    } finally {
      mutationPending.current = false
      setBusy(false)
    }
  }

  const beginEdit = (entry: DictionaryEntry) => {
    if (!activated) return
    setEditingId(entry.id)
    setDraft({
      word: entry.word,
      pronunciation: entry.pronunciation || '',
      correctionFrom: entry.correction_from || '',
    })
  }

  const cancelEdit = () => {
    setEditingId(null)
    setDraft(null)
  }

  const saveEdit = async () => {
    if (!activated || mutationPending.current || editingId == null || !draft?.word.trim()) return
    mutationPending.current = true
    setBusy(true)
    try {
      await updateDictionaryEntry(
        editingId,
        draft.word.trim(),
        draft.pronunciation.trim() || null,
        draft.correctionFrom.trim() || null,
      )
      cancelEdit()
      refresh()
      toast.success(t('dictionary.saved'))
    } catch (error) {
      console.error('Failed to update entry:', error)
      toast.error(t('dictionary.failedToUpdate'))
    } finally {
      mutationPending.current = false
      setBusy(false)
    }
  }

  const handleRemove = async (id: number) => {
    if (mutationPending.current) return
    mutationPending.current = true
    setBusy(true)
    try {
      await removeDictionaryEntry(id)
      if (editingId === id) cancelEdit()
      refresh()
    } catch (e) {
      console.error('Failed to remove entry:', e)
      toast.error(t('dictionary.failedToRemove'))
    } finally {
      mutationPending.current = false
      setBusy(false)
    }
  }

  return (
    <div className="space-y-5">
      <div className="rounded-[12px] border border-accent/25 bg-accent/5 px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="text-[18px] font-semibold text-text-primary">{t('dictionary.title')}</h3>
            <p className="mt-1 text-[13px] leading-relaxed text-text-secondary">
              {t('dictionary.firstPrinciple')}
            </p>
          </div>
          <span className="shrink-0 rounded-full bg-accent/10 px-2.5 py-1 text-[11px] text-accent">
            {t('dictionary.entryCount', { count: dictionary.length })}
          </span>
        </div>
      </div>

      <section className="rounded-[14px] border border-border bg-bg-elevated p-4">
        <div className="flex items-center gap-2 text-[16px] font-semibold text-text-primary">
          <Mic size={18} className="text-accent" />
          {t('dictionary.howItWorks')}
          <span className="ml-auto rounded-full bg-success/10 px-3 py-1 text-[12px] text-success">
            {t(
              usesDecoderHotwords
                ? 'dictionary.decoderHotwordsSupported'
                : 'dictionary.postprocessHotwordsSupported',
            )}
          </span>
          {!activated && (
            <span className="rounded-full bg-warning/10 px-3 py-1 text-[12px] text-warning">
              激活后生效
            </span>
          )}
          {hasPendingProvider && (
            <span className="rounded-full bg-warning/10 px-3 py-1 text-[12px] text-warning">
              识别模式待应用
            </span>
          )}
        </div>
        <p className="mt-3 text-[13px] font-medium text-text-primary">
          {activated ? '当前生效：' : '已保存模式（待激活）：'}
          {providerLabel(provider)}
        </p>
        {hasPendingProvider && (
          <div
            className="mt-3 rounded-xl border border-warning/30 bg-warning/5 p-3 text-[13px] leading-relaxed"
            role="status"
          >
            <p>
              你已选择“{providerLabel(selectedProvider)}
              ”，但尚未保存。下一段录音仍会使用上面的当前模式。
            </p>
            <p className="mt-1 text-text-secondary">
              保存会应用所有当前设置，包括其他页面的修改。词表已单独保存，不受放弃设置更改影响。
            </p>
            <div className="mt-3 flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => void saveConfig()}
                disabled={configSaving || (!activated && selectedProvider !== 'sensevoice')}
                className="rounded-[9px] bg-accent px-3 py-2 font-medium text-white disabled:opacity-50"
              >
                {configSaving ? '正在保存…' : '保存当前设置并应用'}
              </button>
              <button
                type="button"
                onClick={resetConfig}
                disabled={configSaving}
                className="rounded-[9px] border border-border px-3 py-2 text-text-secondary disabled:opacity-50"
              >
                撤销待应用设置
              </button>
            </div>
            {configSaveError && <p className="mt-2 text-error">{configSaveError}</p>}
          </div>
        )}
        <p className="mt-3 text-[13px] leading-relaxed text-text-secondary">
          {activated
            ? t(
                usesDecoderHotwords
                  ? 'dictionary.decoderHotwordsHint'
                  : 'dictionary.postprocessHotwordsHint',
              )
            : '未激活：热词与纠错均不会生效。可以查看或删除已保存的词条；激活后可添加、修改并用于识别。'}
        </p>
        {usesDecoderHotwords && activated && !hasPendingProvider && (
          <p className="mt-2 text-[12px] leading-relaxed text-text-tertiary">
            此状态表示已保存模式支持热词；识别组件仍需安装并成功加载。组件是否就绪可在“语音识别”中查看。
          </p>
        )}
        <p className="mt-2 text-[12px] leading-relaxed text-text-tertiary">
          {activated
            ? t('dictionary.lifecycleHint')
            : '词条不会因试用结束而删除，也不会在激活前偷偷参与后处理。'}
        </p>
        {usesDecoderHotwords && (
          <details className="mt-3 text-[12px] leading-relaxed text-text-secondary">
            <summary className="cursor-pointer font-medium">
              {activated
                ? t('dictionary.hotwordSummary', { count: recognitionWords.length })
                : `已保存 ${recognitionWords.length} 个候选热词（未启用）`}
            </summary>
            <p className="mt-2">
              {t('dictionary.hotwordBudget', { count: recognitionWords.length })}
            </p>
          </details>
        )}
        {!activated && (
          <button
            type="button"
            onClick={() => navigate('account')}
            className="mt-3 rounded-[10px] bg-accent px-4 py-2 text-[13px] font-medium text-white"
          >
            激活热词与纠错
          </button>
        )}
      </section>

      <div className="rounded-[12px] border border-border bg-bg-secondary/30 p-3">
        <div className="flex flex-col gap-3 md:flex-row md:items-end">
          <label className="flex-1 space-y-1">
            <span className="text-[12px] font-medium text-text-primary">
              {t('dictionary.hotwordLabel')}
            </span>
            <input
              value={word}
              disabled={busy || !activated}
              onChange={(event) => setWord(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void handleAdd()
              }}
              placeholder={t('dictionary.correctWordExample')}
              className="w-full rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] text-text-primary outline-none transition-colors focus:border-border-focus"
            />
          </label>
          <button
            onClick={() => void handleAdd()}
            disabled={busy || !activated || !word.trim()}
            className="flex items-center justify-center gap-1.5 rounded-[8px] border-none bg-accent px-4 py-2.5 text-[13px] text-white transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
          >
            <Plus size={14} />
            {t('dictionary.add')}
          </button>
        </div>
        <p className="mt-3 text-[12px] leading-relaxed text-text-secondary">
          {usesDecoderHotwords
            ? t('dictionary.addDecoderHotwordHint')
            : t('dictionary.addPostprocessHotwordHint')}
        </p>
        <details className="mt-3 rounded-[9px] border border-border bg-bg-primary px-3 py-2">
          <summary className="cursor-pointer text-[12px] font-medium text-text-secondary">
            {t('dictionary.advancedCorrection')}
          </summary>
          <p className="mt-2 text-[12px] leading-relaxed text-text-tertiary">
            {t('dictionary.advancedCorrectionHint')}
          </p>
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
            <label className="space-y-1">
              <span className="text-[11px] text-text-secondary">
                {t('dictionary.correctionFromLabel')}
              </span>
              <input
                value={correctionFrom}
                disabled={busy || !activated}
                onChange={(event) => setCorrectionFrom(event.target.value)}
                placeholder={t('dictionary.correctionFromExample')}
                className="w-full rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] text-text-primary outline-none transition-colors focus:border-border-focus"
              />
            </label>
            <label className="space-y-1">
              <span className="text-[11px] text-text-secondary">
                {t('dictionary.pronunciationOptional')}
              </span>
              <input
                value={pronunciation}
                disabled={busy || !activated}
                onChange={(event) => setPronunciation(event.target.value)}
                placeholder={t('dictionary.pronunciationExample')}
                className="w-full rounded-[8px] border border-border bg-bg-primary px-3 py-2.5 text-[13px] text-text-primary outline-none transition-colors focus:border-border-focus"
              />
            </label>
          </div>
        </details>
      </div>

      <div className="overflow-x-auto rounded-[10px] border border-border">
        <table className="w-full min-w-[620px] text-[13px]">
          <thead>
            <tr className="bg-bg-secondary">
              <th className="px-3 py-2.5 text-left text-[11px] font-medium uppercase tracking-wider text-text-secondary">
                {t('dictionary.correctWord')}
              </th>
              <th className="px-3 py-2.5 text-left text-[11px] font-medium uppercase tracking-wider text-text-secondary">
                {t('dictionary.correctionFromLabel')}
              </th>
              <th className="px-3 py-2.5 text-left text-[11px] font-medium uppercase tracking-wider text-text-secondary">
                {t('dictionary.pronunciation')}
              </th>
              <th className="w-24 px-3 py-2.5 text-right text-[11px] font-medium uppercase tracking-wider text-text-secondary">
                {t('dictionary.actions')}
              </th>
            </tr>
          </thead>
          <tbody>
            {dictionary.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-3 py-10 text-center text-[13px] text-text-tertiary">
                  {t('dictionary.noEntries')}
                </td>
              </tr>
            ) : (
              dictionary.map((entry) => {
                const isEditing = editingId === entry.id && draft
                return (
                  <tr
                    key={entry.id}
                    className="border-t border-border transition-colors hover:bg-bg-secondary/50"
                  >
                    <td className="px-3 py-2.5 font-medium text-text-primary">
                      {isEditing ? (
                        <input
                          aria-label={t('dictionary.correctWord')}
                          value={draft.word}
                          disabled={busy || !activated}
                          onChange={(event) => setDraft({ ...draft, word: event.target.value })}
                          className="w-full rounded-[6px] border border-border bg-bg-primary px-2 py-1.5 outline-none focus:border-accent"
                        />
                      ) : (
                        <>
                          {entry.word}
                          {usesDecoderHotwords && !recognitionWordSet.has(entry.word.trim()) && (
                            <span className="mt-1 block text-[11px] font-normal text-warning">
                              {t('dictionary.outsideHotwordBudget')}
                            </span>
                          )}
                        </>
                      )}
                    </td>
                    <td className="px-3 py-2.5 text-text-secondary">
                      {isEditing ? (
                        <input
                          aria-label={t('dictionary.correctionFromLabel')}
                          value={draft.correctionFrom}
                          disabled={busy || !activated}
                          onChange={(event) =>
                            setDraft({ ...draft, correctionFrom: event.target.value })
                          }
                          className="w-full rounded-[6px] border border-border bg-bg-primary px-2 py-1.5 outline-none focus:border-accent"
                        />
                      ) : (
                        entry.correction_from || t('dictionary.hotwordOnly')
                      )}
                    </td>
                    <td className="px-3 py-2.5 text-text-secondary">
                      {isEditing ? (
                        <input
                          aria-label={t('dictionary.pronunciation')}
                          value={draft.pronunciation}
                          disabled={busy || !activated}
                          onChange={(event) =>
                            setDraft({ ...draft, pronunciation: event.target.value })
                          }
                          className="w-full rounded-[6px] border border-border bg-bg-primary px-2 py-1.5 outline-none focus:border-accent"
                        />
                      ) : (
                        entry.pronunciation || '-'
                      )}
                    </td>
                    <td className="px-3 py-2.5">
                      <div className="flex justify-end gap-1">
                        {isEditing ? (
                          <>
                            <button
                              onClick={() => void saveEdit()}
                              disabled={busy || !activated || !draft.word.trim()}
                              title={t('dictionary.save')}
                              className="rounded-[6px] border-none bg-transparent p-1.5 text-success transition-colors hover:bg-bg-tertiary disabled:opacity-40"
                            >
                              <Check size={14} />
                            </button>
                            <button
                              onClick={cancelEdit}
                              disabled={busy}
                              title={t('dictionary.cancel')}
                              className="rounded-[6px] border-none bg-transparent p-1.5 text-text-tertiary transition-colors hover:bg-bg-tertiary"
                            >
                              <X size={14} />
                            </button>
                          </>
                        ) : (
                          <button
                            onClick={() => beginEdit(entry)}
                            disabled={busy || !activated}
                            title={t('dictionary.edit')}
                            className="rounded-[6px] border-none bg-transparent p-1.5 text-text-tertiary transition-colors hover:bg-bg-tertiary hover:text-accent"
                          >
                            <Pencil size={14} />
                          </button>
                        )}
                        <button
                          onClick={() => void handleRemove(entry.id)}
                          disabled={busy}
                          title={t('dictionary.delete')}
                          className="rounded-[6px] border-none bg-transparent p-1.5 text-text-tertiary transition-colors hover:bg-bg-tertiary hover:text-error"
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  )
}
