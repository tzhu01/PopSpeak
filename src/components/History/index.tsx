import { useState, useMemo } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { Search, Copy, Trash2, Pencil, Check, X } from 'lucide-react'
import { spring } from '../../lib/animations'
import { useAppStore } from '../../stores/appStore'
import { clearHistory, deleteHistoryEntry, updateHistoryEntry } from '../../lib/tauri'
import { toast } from '../Toast'

export function History() {
  const history = useAppStore((s) => s.history)
  const setHistory = useAppStore((s) => s.setHistory)
  const patchHistoryEntry = useAppStore((s) => s.patchHistoryEntry)
  const removeHistoryEntry = useAppStore((s) => s.removeHistoryEntry)
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const [copiedId, setCopiedId] = useState<number | null>(null)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [draft, setDraft] = useState('')
  const [saving, setSaving] = useState(false)
  const [deletingId, setDeletingId] = useState<number | null>(null)

  const filtered = useMemo(
    () =>
      search
        ? history.filter(
            (h) =>
              h.polished_text.includes(search) ||
              h.raw_text.includes(search) ||
              h.app_name.includes(search),
          )
        : history,
    [history, search],
  )

  const handleCopy = (id: number, text: string) => {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopiedId(id)
        setTimeout(() => setCopiedId(null), 1500)
      })
      .catch(() => {
        toast.error(t('history.failedToCopy'))
      })
  }

  const handleClear = async () => {
    if (!window.confirm(t('history.clearConfirm'))) return
    try {
      await clearHistory()
      setHistory([])
    } catch (e) {
      console.error('Failed to clear history:', e)
      toast.error(t('history.failedToClear'))
    }
  }

  const startEdit = (id: number, text: string) => {
    setEditingId(id)
    setDraft(text)
  }

  const handleDelete = async (id: number) => {
    if (deletingId !== null || !window.confirm(t('history.deleteConfirm'))) return
    setDeletingId(id)
    try {
      await deleteHistoryEntry(id)
      removeHistoryEntry(id)
      if (editingId === id) cancelEdit()
      toast.success(t('history.deleted'))
    } catch (error) {
      console.error('Failed to delete history entry:', error)
      toast.error(t('history.failedToDelete'))
    } finally {
      setDeletingId(null)
    }
  }

  const cancelEdit = () => {
    setEditingId(null)
    setDraft('')
  }

  const saveEdit = async (id: number) => {
    setSaving(true)
    try {
      await updateHistoryEntry(id, draft)
      patchHistoryEntry(id, draft)
      setEditingId(null)
      setDraft('')
    } catch (e) {
      console.error('Failed to save edit:', e)
      toast.error(t('history.failedToSave') || 'Failed to save')
    } finally {
      setSaving(false)
    }
  }

  // Group by date
  const grouped = useMemo(() => {
    const map = new Map<string, typeof filtered>()
    for (const entry of filtered) {
      const date = entry.created_at.split('T')[0] || entry.created_at.split(' ')[0]
      // History timestamps are saved in local time, not UTC.
      const localDate = (value: Date) =>
        `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-${String(value.getDate()).padStart(2, '0')}`
      const today = localDate(new Date())
      const previousDay = new Date()
      previousDay.setDate(previousDay.getDate() - 1)
      const yesterday = localDate(previousDay)
      const label =
        date === today ? t('history.today') : date === yesterday ? t('history.yesterday') : date
      if (!map.has(label)) map.set(label, [])
      map.get(label)!.push(entry)
    }
    return map
  }, [filtered, t])

  return (
    <div className="history-page w-full h-full text-text-primary flex flex-col p-5">
      {/* Header */}
      <div className="flex items-center justify-between px-2 pt-1 pb-4">
        <div>
          <p className="brand-kicker mb-1">ARCHIVE</p>
          <h2 className="brand-display text-[22px] font-semibold">{t('history.title')}</h2>
        </div>
      </div>

      {/* Search — jelly focus */}
      <div className="pb-4">
        <div className="relative">
          <Search
            size={14}
            className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary"
          />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('history.searchPlaceholder')}
            className="w-full pl-8 pr-3 py-2.5 bg-bg-secondary border border-border rounded-[14px] text-[13px] text-text-primary outline-none focus:ring-2 focus:ring-jelly-primary focus:border-jelly-primary transition-all jelly-btn"
            style={{ transform: 'none' }}
          />
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto rounded-[18px] border border-border bg-bg-elevated/55 p-4 shadow-sm">
        {filtered.length === 0 ? (
          <p className="text-center text-text-tertiary text-[13px] py-12">
            {search ? (
              t('history.noResults')
            ) : (
              <>
                {t('history.noHistory')}
                <br />
                <span className="text-[12px]">{t('history.noHistoryHint')}</span>
              </>
            )}
          </p>
        ) : (
          <AnimatePresence>
            {Array.from(grouped.entries()).map(([label, entries]) => (
              <div key={label} className="mb-4">
                <h3 className="text-[11px] font-medium text-text-tertiary uppercase tracking-wider mb-2 px-1 pb-1 border-b border-border">
                  {label}
                </h3>
                <div className="space-y-0.5">
                  {entries.map((entry) => {
                    const isEditing = editingId === entry.id
                    return (
                      <motion.div
                        key={entry.id}
                        whileHover={isEditing ? undefined : { scale: 1.01 }}
                        transition={spring.jellyGentle}
                        className="group flex items-start gap-3 px-3 py-3 rounded-[12px] border border-transparent hover:border-border hover:bg-bg-primary/70 transition-colors"
                      >
                        <div className="flex-1 min-w-0">
                          {isEditing ? (
                            <div className="grid w-full">
                              <div
                                aria-hidden="true"
                                className="invisible col-start-1 row-start-1 min-h-[64px] whitespace-pre-wrap break-words [overflow-wrap:anywhere] border px-2 py-1.5 text-[13px] leading-relaxed"
                              >
                                {draft || ' '}
                                {'\n'}
                              </div>
                              <textarea
                                value={draft}
                                onChange={(e) => setDraft(e.target.value)}
                                autoFocus
                                rows={2}
                                className="col-start-1 row-start-1 h-full min-h-[64px] w-full resize-none overflow-hidden px-2 py-1.5 text-[13px] leading-relaxed bg-bg-primary border border-jelly-primary rounded-[8px] text-text-primary outline-none focus:ring-2 focus:ring-jelly-primary"
                                onKeyDown={(e) => {
                                  if (e.key === 'Escape') cancelEdit()
                                  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey))
                                    void saveEdit(entry.id)
                                }}
                              />
                            </div>
                          ) : (
                            <p className="text-[13px] text-text-primary leading-relaxed whitespace-pre-wrap">
                              {entry.polished_text}
                            </p>
                          )}
                          <p className="text-[11px] text-text-tertiary mt-1">
                            {entry.created_at.split('T')[1]?.slice(0, 5) || ''} · {entry.app_name}
                          </p>
                        </div>
                        {isEditing ? (
                          <div className="flex items-center gap-1 flex-shrink-0">
                            <motion.button
                              onClick={() => void saveEdit(entry.id)}
                              disabled={saving}
                              whileTap={{ scaleX: 1.1, scaleY: 0.9 }}
                              transition={spring.jelly}
                              className="p-1.5 rounded-[6px] hover:bg-bg-tertiary transition-all duration-200 bg-transparent border-none cursor-pointer text-success flex-shrink-0"
                              aria-label="Save"
                            >
                              <Check size={13} />
                            </motion.button>
                            <motion.button
                              onClick={cancelEdit}
                              whileTap={{ scaleX: 1.1, scaleY: 0.9 }}
                              transition={spring.jelly}
                              className="p-1.5 rounded-[6px] hover:bg-bg-tertiary transition-all duration-200 bg-transparent border-none cursor-pointer text-text-tertiary hover:text-error flex-shrink-0"
                              aria-label="Cancel"
                            >
                              <X size={13} />
                            </motion.button>
                          </div>
                        ) : (
                          <div className="flex items-center gap-1 flex-shrink-0">
                            <motion.button
                              onClick={() => startEdit(entry.id, entry.polished_text)}
                              whileTap={{ scaleX: 1.1, scaleY: 0.9 }}
                              transition={spring.jelly}
                              className="p-1.5 rounded-[6px] hover:bg-bg-tertiary bg-transparent border-none cursor-pointer text-text-tertiary hover:text-accent"
                              aria-label={`Edit: ${entry.polished_text.slice(0, 30)}`}
                            >
                              <Pencil size={13} />
                            </motion.button>
                            <motion.button
                              onClick={() => handleCopy(entry.id, entry.polished_text)}
                              whileTap={{ scaleX: 1.1, scaleY: 0.9 }}
                              transition={spring.jelly}
                              className="p-1.5 rounded-[6px] hover:bg-bg-tertiary bg-transparent border-none cursor-pointer text-text-tertiary hover:text-accent"
                              aria-label={`Copy text: ${entry.polished_text.slice(0, 30)}`}
                            >
                              <Copy size={13} />
                            </motion.button>
                            <motion.button
                              onClick={() => void handleDelete(entry.id)}
                              disabled={deletingId !== null}
                              whileTap={{ scaleX: 1.1, scaleY: 0.9 }}
                              transition={spring.jelly}
                              className="p-1.5 rounded-[6px] hover:bg-bg-tertiary bg-transparent border-none cursor-pointer text-text-tertiary hover:text-error disabled:opacity-40"
                              aria-label={`${t('history.delete')}: ${entry.polished_text.slice(0, 30)}`}
                              title={t('history.delete')}
                            >
                              <Trash2 size={13} />
                            </motion.button>
                          </div>
                        )}
                        {copiedId === entry.id && !isEditing && (
                          <span className="text-[11px] text-success flex-shrink-0 self-center">
                            {t('history.copied')}
                          </span>
                        )}
                      </motion.div>
                    )
                  })}
                </div>
              </div>
            ))}
          </AnimatePresence>
        )}
      </div>

      {/* Clear button — jelly */}
      {history.length > 0 && (
        <div className="pt-3">
          <motion.button
            onClick={handleClear}
            whileHover={{ scale: 1.04 }}
            whileTap={{ scaleX: 1.06, scaleY: 0.94 }}
            transition={spring.jellyGentle}
            className="flex items-center justify-center gap-1.5 w-full py-2 text-[12px] text-text-tertiary hover:text-error rounded-[10px] cursor-pointer transition-colors jelly-btn"
          >
            <Trash2 size={12} />
            {t('history.clearAll')}
          </motion.button>
        </div>
      )}
    </div>
  )
}
