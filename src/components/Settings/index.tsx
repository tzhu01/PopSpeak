import { useEffect, useState, useRef } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'
import { SettingsSidebar, type PaneId } from './SettingsSidebar'
import { GeneralPane } from './GeneralPane'
import { SttPane } from './SttPane'
import { LlmPane } from './LlmPane'
import { DictionaryPane } from './DictionaryPane'
import { ScenesPane } from './ScenesPane'
import { AboutPane } from './AboutPane'
import { DirtyBar, useDirtyConfig } from './shared/DirtyBar'
import { ActivationNotice } from '../ActivationNotice'

const paneTitleKeys: Record<PaneId, string> = {
  general: 'settings.general',
  stt: 'settings.speechRecognition',
  llm: 'settings.aiPolish',
  dictionary: 'settings.dictionary',
  scenes: 'settings.scenes',
  about: 'settings.about',
}

export function Settings() {
  const [activePane, setActivePane] = useState<PaneId>(paneFromHash)
  const paneScrollRef = useRef<HTMLDivElement>(null)
  const configSaveError = useAppStore((s) => s.configSaveError)
  const isDirty = useDirtyConfig()
  const { t } = useTranslation()

  useEffect(() => {
    const syncPane = () => setActivePane(paneFromHash())
    window.addEventListener('hashchange', syncPane)
    return () => window.removeEventListener('hashchange', syncPane)
  }, [])

  const handlePaneSelect = (pane: PaneId) => {
    // Every settings pane owns its own visual starting point. Reusing the
    // previous pane's scroll position made shorter panes appear to have a
    // large blank header area after switching tabs.
    if (paneScrollRef.current) paneScrollRef.current.scrollTop = 0
    setActivePane(pane)
    window.location.hash = pane === 'general' ? '#/settings' : `#/settings/${pane}`
  }

  return (
    <div className="settings-page w-full h-full text-text-primary flex flex-col">
      <div className="settings-layout flex-1 flex min-h-0 p-4 gap-4">
        {/* Sidebar */}
        <SettingsSidebar activePane={activePane} onSelect={handlePaneSelect} />

        {/* Content */}
        <div className="flex-1 flex flex-col min-w-0 overflow-hidden rounded-[20px] border border-border bg-bg-elevated/70 shadow-sm">
          {/* Title bar */}
          <div className="flex items-center justify-between px-7 pt-5 pb-4 border-b border-border bg-bg-elevated/60">
            <div>
              <p className="brand-kicker mb-1">SETTINGS</p>
              <h2 className="brand-display text-[20px] font-semibold">
                {t(paneTitleKeys[activePane])}
              </h2>
            </div>
          </div>

          {/* Pane content */}
          <div
            ref={paneScrollRef}
            data-testid="settings-pane-scroll"
            className="flex-1 overflow-y-auto px-7 py-6"
          >
            {/* Keep exactly one pane in normal flow. AnimatePresence mode="sync"
                kept the exiting (often much taller) pane above the next pane. */}
            <motion.div
              key={activePane}
              data-testid="settings-pane-content"
              className="w-full max-w-[980px]"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.1, ease: 'easeOut' }}
            >
              {activePane === 'general' && <GeneralPane />}
              {['stt', 'llm', 'dictionary'].includes(activePane) && (
                <div className="mb-5">
                  <ActivationNotice
                    feature={
                      activePane === 'stt'
                        ? '默认试用以外的识别模式'
                        : activePane === 'dictionary'
                          ? '热词与纠错'
                          : '文字润色与翻译'
                    }
                  />
                </div>
              )}
              {activePane === 'stt' && <SttPane />}
              {activePane === 'llm' && <LlmPane />}
              {activePane === 'dictionary' && (
                <DictionaryPane onOpenRecognition={() => handlePaneSelect('stt')} />
              )}
              {activePane === 'scenes' && <ScenesPane />}
              {activePane === 'about' && <AboutPane />}
            </motion.div>
          </div>
        </div>
      </div>

      {/* Dirty bar */}
      <AnimatePresence>{(isDirty || configSaveError) && <DirtyBar />}</AnimatePresence>
    </div>
  )
}

function paneFromHash(): PaneId {
  const pane = window.location.hash.match(/^#\/settings\/(stt|llm|dictionary|scenes|about)$/)?.[1]
  return (pane as PaneId | undefined) ?? 'general'
}
