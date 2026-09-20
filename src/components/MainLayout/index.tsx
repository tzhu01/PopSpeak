import { Home, Settings, History, UserRound } from 'lucide-react'
import { motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { spring } from '../../lib/animations'
import { useRoute, type Route } from '../../lib/router'
import { BrandMark } from '../Brand/BrandMark'
import { AccessibilityBanner } from './AccessibilityBanner'
import { TitleBar } from './TitleBar'
import { useActivationLifecycle, useActivationStore } from '../../lib/activation'

const navItems: { id: Route; labelKey: string; icon: typeof Home }[] = [
  { id: 'home', labelKey: 'nav.home', icon: Home },
  { id: 'settings', labelKey: 'nav.settings', icon: Settings },
  { id: 'history', labelKey: 'nav.history', icon: History },
  { id: 'account', labelKey: 'nav.account', icon: UserRound },
]

interface Props {
  children: React.ReactNode
}

export function MainLayout({ children }: Props) {
  useActivationLifecycle()
  const activation = useActivationStore((s) => s.status)
  const { route, navigate } = useRoute()
  const { t } = useTranslation()

  return (
    <div className="brand-shell w-full h-full flex flex-col bg-bg-primary text-text-primary">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <aside className="brand-sidebar w-[208px] flex flex-col border-r border-border jelly-surface-flat shrink-0">
          <div className="flex items-center gap-3 px-5 pt-5 pb-6" data-tauri-drag-region>
            <BrandMark size={34} className="shrink-0 drop-shadow-sm" />
            <div data-tauri-drag-region>
              <h1
                className="brand-wordmark text-[16px] font-semibold tracking-tight"
                data-tauri-drag-region
              >
                {t('app.name')}
              </h1>
              <p className="text-[10px] text-text-tertiary mt-0.5" data-tauri-drag-region>
                {t('app.tagline')}
              </p>
            </div>
          </div>

          <nav className="flex-1 px-3 space-y-1 relative" aria-label="Main navigation">
            {navItems.map(({ id, labelKey, icon: Icon }) => {
              const active = route === id
              const label = id === 'account' ? '公众号激活' : t(labelKey)
              return (
                <motion.button
                  key={id}
                  onClick={() => navigate(id)}
                  whileHover={{ x: 2 }}
                  whileTap={{ scale: 0.98 }}
                  transition={spring.jellyGentle}
                  aria-label={label}
                  aria-current={active ? 'page' : undefined}
                  className={`flex items-center gap-2.5 w-full px-3 py-2.5 text-[13px] rounded-[10px] transition-colors bg-transparent border-none cursor-pointer text-left relative ${
                    active
                      ? 'text-text-primary font-medium'
                      : 'text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {active && (
                    <motion.div
                      layoutId="nav-indicator"
                      className="absolute inset-0 jelly-nav-active"
                      transition={spring.jellyGentle}
                    />
                  )}
                  <span className="relative z-10 flex items-center gap-2.5">
                    <Icon size={16} />
                    {label}
                  </span>
                </motion.button>
              )
            })}
          </nav>

          <div className="mx-4 mb-4 rounded-[13px] border border-border bg-bg-elevated/60 px-3 py-2.5">
            <div className="flex items-center gap-2 text-[9px] font-semibold tracking-[0.16em] text-text-tertiary">
              <span className="h-1.5 w-1.5 rounded-full bg-success" />
              LOCAL · CPU
            </div>
            <p className="mt-1 text-[10px] leading-relaxed text-text-secondary">
              PRIVATE BY DESIGN
            </p>
            <button
              onClick={() => navigate('account')}
              className="mt-2 text-left text-[12px] font-medium text-accent"
            >
              {activation?.activated
                ? '本机已激活'
                : activation?.recording_in_progress
                  ? '录音中 · 额度预留'
                  : activation?.trial_exhausted
                    ? '试用已用完 · 去激活'
                    : activation
                      ? '试用中 · 去激活'
                      : '检查激活状态'}
            </button>
          </div>
        </aside>

        <main className="brand-main flex-1 min-w-0 flex flex-col">
          <AccessibilityBanner />
          <div className="flex-1 overflow-y-auto">{children}</div>
        </main>
      </div>
    </div>
  )
}
