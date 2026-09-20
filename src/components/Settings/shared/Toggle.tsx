import { motion } from 'framer-motion'

interface Props {
  checked: boolean
  onChange: (checked: boolean) => void
  label?: string
}

export function Toggle({ checked, onChange, label }: Props) {
  return (
    <label className="flex items-center gap-2.5 cursor-pointer">
      <button
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative w-[44px] h-[26px] rounded-full border cursor-pointer transition-colors duration-200 shadow-inner ${
          checked ? 'bg-accent border-accent' : 'bg-bg-tertiary border-border'
        }`}
      >
        <motion.div
          className="absolute top-[2px] w-[22px] h-[22px] rounded-full bg-white shadow-sm"
          animate={{ left: checked ? 20 : 2 }}
          transition={{ type: 'spring', stiffness: 500, damping: 30 }}
        />
      </button>
      {label && <span className="text-[13px] text-text-primary">{label}</span>}
    </label>
  )
}
