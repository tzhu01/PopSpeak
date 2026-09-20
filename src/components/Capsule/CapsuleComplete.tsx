import { motion, useReducedMotion } from 'framer-motion'
import { Send } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { CapsuleStageProgress } from './CapsuleStageProgress'

export function CapsuleComplete() {
  const reduced = useReducedMotion()
  const { t } = useTranslation()
  const stageLabel = t('capsule.outputtingStage')

  return (
    <motion.div className="relative z-10 flex items-center gap-1.5 h-9 px-3">
      <motion.div
        animate={reduced ? undefined : { x: [0, 2, 0] }}
        transition={{ repeat: Infinity, duration: 0.8, ease: 'easeInOut' }}
      >
        <Send size={13} className="text-white" />
      </motion.div>
      <span className="text-[11px] text-white font-medium">{stageLabel}</span>
      <CapsuleStageProgress value={100} label={stageLabel} />
    </motion.div>
  )
}
