import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { openUrl } from '@tauri-apps/plugin-opener'
import i18n from '../../i18n'
import { useAppStore } from '../../stores/appStore'
import { APP_NAME, APP_VERSION } from '../../lib/constants'
import { checkForUpdates, type UpdateInfo } from '../../lib/tauri'

const UI_LANGUAGES = [
  { code: 'en', label: 'English', native: 'English' },
  { code: 'zh', label: 'Chinese', native: '中文' },
] as const

export function AboutPane() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [updateError, setUpdateError] = useState('')
  const [checking, setChecking] = useState(false)

  const currentLang = config.ui_language || i18n.language || 'en'

  const handleSelectLanguage = (code: string) => {
    i18n.changeLanguage(code)
    localStorage.setItem('ui_language', code)
    updateConfig({ ui_language: code })
  }

  const handleCheckUpdates = async () => {
    setChecking(true)
    setUpdateError('')
    try {
      setUpdateInfo(await checkForUpdates())
    } catch (error) {
      setUpdateError(String(error))
    } finally {
      setChecking(false)
    }
  }

  return (
    <div className="space-y-5 text-[13px]">
      {/* Header */}
      <div className="text-center py-6">
        <h2 className="text-[22px] font-semibold text-text-primary">{APP_NAME}</h2>
        <p className="text-text-secondary mt-1 text-[13px]">{APP_VERSION}</p>
      </div>

      <p className="text-text-secondary leading-relaxed">{t('settings.aboutDescription')}</p>

      <SectionCard title={t('settings.updates')}>
        <div className="p-3 space-y-2">
          <button
            type="button"
            disabled={checking}
            onClick={handleCheckUpdates}
            className="w-full px-3 py-2 rounded-[8px] bg-accent text-white disabled:opacity-60 cursor-pointer"
          >
            {checking ? t('settings.checkingUpdates') : t('settings.checkUpdates')}
          </button>
          {updateInfo && (
            <div className="text-text-secondary text-[12px] leading-relaxed">
              {updateInfo.available
                ? t('settings.updateAvailable', { version: updateInfo.latest_version })
                : t('settings.upToDate')}
              {updateInfo.available && (
                <button
                  type="button"
                  className="ml-2 text-accent cursor-pointer"
                  onClick={() => openUrl(updateInfo.release_url)}
                >
                  {t('settings.openDownloadPage')}
                </button>
              )}
            </div>
          )}
          {updateError && <p className="text-red-500 text-[12px]">{updateError}</p>}
          <p className="text-text-tertiary text-[11px]">{t('settings.offlineUpdateNote')}</p>
        </div>
      </SectionCard>

      {/* Language */}
      <SectionCard title={t('settings.language')}>
        <div className="grid grid-cols-2 gap-3 p-3">
          {UI_LANGUAGES.map((lang) => (
            <button
              key={lang.code}
              onClick={() => handleSelectLanguage(lang.code)}
              className={`px-4 py-3 rounded-[8px] text-[13px] border cursor-pointer transition-all ${
                currentLang === lang.code
                  ? 'bg-accent/10 border-accent text-accent font-medium'
                  : 'bg-bg-secondary border-border text-text-primary hover:border-text-tertiary'
              }`}
            >
              <div className="font-medium">{lang.native}</div>
              <div className="text-[11px] text-text-tertiary mt-0.5">{lang.label}</div>
            </button>
          ))}
        </div>
      </SectionCard>

      {/* Open Source */}
      <SectionCard title={t('settings.openSource')}>
        <InfoRow label={t('settings.license')} value={t('settings.mit')} />
      </SectionCard>
    </div>
  )
}

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="border border-border rounded-[10px] overflow-hidden">
      <div className="px-3 py-2.5 bg-bg-secondary/50 border-b border-border">
        <h3 className="text-[13px] font-medium text-text-primary">{title}</h3>
      </div>
      {children}
    </div>
  )
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between px-3 py-2.5 border-b border-border last:border-b-0">
      <span className="text-text-secondary">{label}</span>
      <span className="text-text-primary">{value}</span>
    </div>
  )
}
