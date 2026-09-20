import { X } from 'lucide-react'
import { motion } from 'framer-motion'
import { spring } from '../../lib/animations'
import { StepIndicator } from './StepIndicator'

interface Props {
  step: number
  totalSteps: number
  title: string
  subtitle?: string
  canNext: boolean
  canBack: boolean
  nextLabel?: string
  onNext: () => void
  onBack: () => void
  onSkip?: () => void
  children: React.ReactNode
}

export function OnboardingLayout({
  step,
  totalSteps,
  title,
  subtitle,
  canNext,
  canBack,
  nextLabel = 'Next',
  onNext,
  onBack,
  onSkip,
  children,
}: Props) {
  const handleClose = () => {
    import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke('plugin:process|exit', { code: 0 }))
      .catch(() => {})
  }

  return (
    <div className="brand-shell w-full h-full flex flex-col text-text-primary">
      {/* Drag region + close button */}
      <div className="flex items-center justify-end px-3 pt-3 pb-0" data-tauri-drag-region>
        <button
          onClick={handleClose}
          className="p-1.5 rounded-[6px] hover:bg-bg-tertiary transition-colors bg-transparent border-none cursor-pointer text-text-tertiary hover:text-text-primary"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </div>

      {/* Header */}
      <div className="flex items-center justify-between pt-1 px-8">
        <div className="w-16" />
        <StepIndicator total={totalSteps} current={step} />
        {onSkip ? (
          <button
            onClick={onSkip}
            className="w-16 text-right text-[12px] text-text-tertiary hover:text-text-primary bg-transparent border-none cursor-pointer transition-colors"
          >
            Skip
          </button>
        ) : (
          <div className="w-16" />
        )}
      </div>

      {/* Title */}
      <div className="text-center px-8 pt-3 pb-4">
        <p className="brand-kicker mb-2">POPSPEAK · LOCAL FIRST</p>
        <h1 className="brand-display text-[25px] font-semibold text-text-primary">{title}</h1>
        {subtitle && <p className="text-[13px] text-text-secondary mt-1.5">{subtitle}</p>}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-8">
        <div className="max-w-[380px] mx-auto rounded-[20px] border border-border bg-bg-elevated/60 p-5 shadow-sm mb-6">
          {children}
        </div>
      </div>

      {/* Navigation — jelly buttons */}
      <div className="flex items-center justify-between px-8 py-4">
        <motion.button
          onClick={onBack}
          disabled={!canBack}
          whileHover={canBack ? { scale: 1.04 } : undefined}
          whileTap={canBack ? { scaleX: 1.06, scaleY: 0.94 } : undefined}
          transition={spring.jellyGentle}
          className="px-4 py-2 text-[13px] text-text-secondary hover:text-text-primary bg-transparent border-none cursor-pointer disabled:opacity-0 disabled:cursor-default transition-colors"
        >
          Back
        </motion.button>
        <motion.button
          onClick={onNext}
          disabled={!canNext}
          whileHover={canNext ? { scale: 1.04 } : undefined}
          whileTap={canNext ? { scaleX: 1.06, scaleY: 0.94 } : undefined}
          transition={spring.jellyGentle}
          className="px-6 py-2 text-[13px] font-medium text-white bg-accent rounded-full border-none cursor-pointer hover:bg-accent-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors jelly-btn-accent"
        >
          {nextLabel}
        </motion.button>
      </div>
    </div>
  )
}
