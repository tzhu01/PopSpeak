import { motion } from 'framer-motion'
import { Loader2 } from 'lucide-react'
import { useAppStore } from '../../../stores/appStore'

// eslint-disable-next-line react-refresh/only-export-components
export function useDirtyConfig() {
  const config = useAppStore((s) => s.config)
  const savedConfig = useAppStore((s) => s.savedConfig)
  return savedConfig !== null && JSON.stringify(config) !== JSON.stringify(savedConfig)
}

export function DirtyBar() {
  const resetConfig = useAppStore((s) => s.resetConfig)
  const saveConfig = useAppStore((s) => s.saveConfig)
  const saving = useAppStore((s) => s.configSaving)
  const errorMsg = useAppStore((s) => s.configSaveError)
  const bgClass = errorMsg
    ? 'bg-error/10 border-t border-error/20'
    : 'bg-warning/10 border-t border-warning/20'

  return (
    <motion.div
      className={`flex items-center justify-between px-5 py-3 ${bgClass}`}
      initial={{ y: 20, opacity: 0 }}
      animate={{ y: 0, opacity: 1 }}
      exit={{ y: 20, opacity: 0 }}
      transition={{ type: 'spring', stiffness: 400, damping: 30 }}
    >
      <span
        className={`${errorMsg ? 'text-error' : 'text-warning'} text-[13px] mr-3`}
        role={errorMsg ? 'alert' : 'status'}
      >
        {errorMsg || '有未保存的设置；保存后应用于下一段录音'}
      </span>
      <div className="flex items-center gap-2 flex-shrink-0">
        <button
          onClick={resetConfig}
          disabled={saving}
          className="px-3 py-1.5 text-[12px] text-text-secondary hover:text-text-primary bg-transparent border-none cursor-pointer rounded-[10px] hover:bg-bg-tertiary transition-colors disabled:opacity-50"
        >
          放弃更改
        </button>
        <button
          onClick={() => void saveConfig()}
          disabled={saving}
          className="flex items-center gap-1.5 px-3 py-1.5 text-[12px] text-white bg-accent rounded-[10px] border-none cursor-pointer hover:opacity-90 transition-opacity disabled:opacity-70"
        >
          {saving && (
            <motion.div
              animate={{ rotate: 360 }}
              transition={{ repeat: Infinity, duration: 0.8, ease: 'linear' }}
            >
              <Loader2 size={12} />
            </motion.div>
          )}
          {saving ? '正在保存…' : '保存并应用'}
        </button>
      </div>
    </motion.div>
  )
}
