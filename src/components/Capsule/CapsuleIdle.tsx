import { motion, useReducedMotion } from 'framer-motion'
import { spring } from '../../lib/animations'
import { CapsuleLogo } from './CapsuleLogo'

export function CapsuleIdle() {
  const reduced = useReducedMotion()

  return (
    <motion.div
      className="relative z-10 flex items-center justify-center w-9 h-9 cursor-pointer"
      whileHover={reduced ? undefined : { scale: 1.06 }}
      whileTap={reduced ? undefined : { scale: 0.94 }}
      transition={spring.smooth}
    >
      <CapsuleLogo size={21} className="signal-orbit-mark" />
    </motion.div>
  )
}
