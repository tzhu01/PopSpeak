import { motion, useReducedMotion } from 'framer-motion'

interface CapsuleStageProgressProps {
  value: number
  label: string
}

export function CapsuleStageProgress({ value, label }: CapsuleStageProgressProps) {
  const reduced = useReducedMotion()
  const normalized = Math.min(100, Math.max(0, value))

  return (
    <div
      className="capsule-stage-progress"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={normalized}
    >
      <motion.div
        className="capsule-stage-progress-fill"
        initial={reduced ? false : { width: 0 }}
        animate={{ width: `${normalized}%` }}
        transition={reduced ? { duration: 0 } : { duration: 0.25, ease: 'easeOut' }}
      />
    </div>
  )
}
