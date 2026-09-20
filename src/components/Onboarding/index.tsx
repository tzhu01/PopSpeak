import { AnimatePresence, motion } from 'framer-motion'
import { useAppStore } from '../../stores/appStore'
import { saveOnboardingCompleted, updateConfig as saveConfig } from '../../lib/tauri'
import { OnboardingLayout } from './OnboardingLayout'
import { WelcomeStep } from './WelcomeStep'
import { ModeSelectStep } from './ModeSelectStep'
import { QuickTestStep } from './QuickTestStep'
import { DoneStep } from './DoneStep'
import { slideRight } from '../../lib/animations'

const TOTAL_STEPS = 4

export function Onboarding() {
  const step = useAppStore((s) => s.onboardingStep)
  const setStep = useAppStore((s) => s.setOnboardingStep)
  const setOnboardingCompleted = useAppStore((s) => s.setOnboardingCompleted)
  const onboardingMode = useAppStore((s) => s.onboardingMode)

  const canNext = (() => {
    switch (step) {
      case 0:
        return true // Welcome
      case 1:
        return onboardingMode !== null
      case 2:
        return true
      case 3:
        return true // Done
      default:
        return false
    }
  })()

  const titles = [
    {
      title: '欢迎使用 PopSpeak',
      subtitle: '几步设置即可开始语音输入',
    },
    {
      title: '选择运行方式',
      subtitle: '默认推荐 CPU 本地离线模式，音频不会离开电脑',
    },
    {
      title: '使用方式',
      subtitle: '记住快捷键，说完后文字会直接写入当前光标',
    },
    { title: '设置完成', subtitle: undefined },
  ]

  const config = useAppStore((s) => s.config)

  const handleNext = async () => {
    if (step < TOTAL_STEPS - 1) {
      try {
        await saveConfig(config)
      } catch {
        // Best-effort save
      }
      setStep(step + 1)
    } else {
      await saveConfig(config)
      await saveOnboardingCompleted()
      setOnboardingCompleted(true)
    }
  }

  const handleBack = async () => {
    if (step > 0) {
      try {
        await saveConfig(config)
      } catch {
        // Best-effort save
      }
      setStep(step - 1)
    }
  }

  const handleSkip = async () => {
    await saveConfig(config)
    await saveOnboardingCompleted()
    setOnboardingCompleted(true)
  }

  return (
    <OnboardingLayout
      step={step}
      totalSteps={TOTAL_STEPS}
      title={titles[step].title}
      subtitle={titles[step].subtitle}
      canNext={canNext}
      canBack={step > 0}
      nextLabel={step === TOTAL_STEPS - 1 ? '开始使用' : '下一步'}
      onNext={handleNext}
      onBack={handleBack}
      onSkip={handleSkip}
    >
      <AnimatePresence mode="wait">
        <motion.div
          key={step}
          variants={slideRight}
          initial="initial"
          animate="animate"
          exit="exit"
          transition={{ duration: 0.2 }}
        >
          {step === 0 && <WelcomeStep />}
          {step === 1 && <ModeSelectStep />}
          {step === 2 && <QuickTestStep />}
          {step === 3 && <DoneStep />}
        </motion.div>
      </AnimatePresence>
    </OnboardingLayout>
  )
}
