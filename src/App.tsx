import { useEffect, useState } from 'react'
import i18n from './i18n'
import { useTauriEvents } from './hooks/useTauriEvents'
import { useTheme } from './hooks/useTheme'
import { useAppStore } from './stores/appStore'
import { useAuthStore } from './stores/authStore'
import { useRoute } from './lib/router'
import {
  loadOnboardingCompleted,
  getConfig,
  getHistory,
  getDictionary,
  checkAccessibilityPermission,
  saveOnboardingCompleted,
} from './lib/tauri'
import { initDeepLinkListener } from './lib/deep-link'
import { Capsule } from './components/Capsule'
import { Settings } from './components/Settings'
import { History } from './components/History'
import { MainLayout } from './components/MainLayout'
import { TitleBar } from './components/MainLayout/TitleBar'
import { HomePage } from './components/HomePage'
import { ImageRecognition } from './components/ImageRecognition'
import { UpgradePage } from './components/UpgradePage'
import { AccountPage } from './components/AccountPage'
import { ToastContainer } from './components/Toast'
import { EditorApp } from './components/Editor'

function CapsuleApp() {
  useTauriEvents()
  useTheme()

  const setConfig = useAppStore((s) => s.setConfig)

  useEffect(() => {
    // Load config so DurationTimer gets the correct max_recording_seconds
    getConfig()
      .then((config) => {
        setConfig(config)
        // Restore UI language from config
        if (config.ui_language && config.ui_language !== i18n.language) {
          i18n.changeLanguage(config.ui_language)
          localStorage.setItem('ui_language', config.ui_language)
        }
      })
      .catch((e) => {
        console.error('Failed to load config in capsule:', e)
      })
  }, [setConfig])

  // Window show is handled by useCapsuleResize (setSize → setPosition → show),
  // which works on both Windows and macOS. The previous rAF-based show approach
  // failed on macOS because WKWebView pauses requestAnimationFrame in hidden windows.
  return <Capsule />
}

function MainApp() {
  useTauriEvents()
  useTheme()

  const setOnboardingCompleted = useAppStore((s) => s.setOnboardingCompleted)
  const setConfig = useAppStore((s) => s.setConfig)
  const setSavedConfig = useAppStore((s) => s.setSavedConfig)
  const setHistory = useAppStore((s) => s.setHistory)
  const setDictionary = useAppStore((s) => s.setDictionary)
  const setAccessibilityTrusted = useAppStore((s) => s.setAccessibilityTrusted)
  const [loaded, setLoaded] = useState(false)
  const [loadError, setLoadError] = useState(false)
  const { route } = useRoute()

  useEffect(() => {
    void (async () => {
      try {
        const done = await loadOnboardingCompleted()
        // Load backend defaults before onboarding too, so first-run users see
        // the same CPU/offline configuration the Rust pipeline will use.
        const [config, history, dictionary] = await Promise.all([
          getConfig(),
          getHistory(200, 0),
          getDictionary(),
        ])
        setConfig(config)
        setSavedConfig(config)
        setHistory(history)
        setDictionary(dictionary)
        if (navigator.platform.toUpperCase().indexOf('MAC') >= 0) {
          checkAccessibilityPermission().then((trusted) => {
            setAccessibilityTrusted(trusted)
          })
        }
        if (config.ui_language && config.ui_language !== i18n.language) {
          i18n.changeLanguage(config.ui_language)
          localStorage.setItem('ui_language', config.ui_language)
        }

        // The portable build ships with a complete, usable offline default.
        // First launch therefore opens straight to Home instead of blocking dictation
        // behind a multi-step setup wizard.
        if (!done) {
          try {
            await saveOnboardingCompleted()
          } catch (error) {
            console.warn('Unable to persist first-launch completion:', error)
          }
        }
        setOnboardingCompleted(true)
      } catch (e) {
        console.error('Failed to load initial data:', e)
        setLoadError(true)
      } finally {
        setLoaded(true)
      }
    })()

    // Initialize auth session (non-blocking)
    useAuthStore.getState().initialize()

    // Initialize deep-link listener
    initDeepLinkListener()
  }, [
    setOnboardingCompleted,
    setConfig,
    setSavedConfig,
    setHistory,
    setDictionary,
    setAccessibilityTrusted,
  ])

  const user = useAuthStore((s) => s.user)

  // Periodically refresh subscription status + refresh on window focus (throttled)
  useEffect(() => {
    if (!loaded || !user) return

    let lastRefresh = 0
    const throttledRefresh = () => {
      const now = Date.now()
      const { checkoutPending } = useAuthStore.getState()
      // Skip throttle if user just came back from checkout
      if (!checkoutPending && now - lastRefresh < 30_000) return
      lastRefresh = now
      useAuthStore.getState().refreshSubscription()
    }

    const interval = setInterval(
      () => {
        lastRefresh = Date.now()
        useAuthStore.getState().refreshSubscription()
      },
      5 * 60 * 1000,
    )

    window.addEventListener('focus', throttledRefresh)

    return () => {
      clearInterval(interval)
      window.removeEventListener('focus', throttledRefresh)
    }
  }, [loaded, user])

  if (!loaded || loadError)
    return (
      <div className="brand-shell flex flex-col h-screen">
        <TitleBar />
        <div className="flex flex-1 flex-col items-center justify-center gap-3">
          <span className={loadError ? 'text-error text-sm' : 'text-text-secondary text-sm'}>
            {loadError ? '应用数据加载失败，请重试。' : '正在准备语音输入…'}
          </span>
          {loadError && (
            <button
              onClick={() => window.location.reload()}
              className="px-4 py-2 bg-accent text-white rounded-[10px] text-sm border-none cursor-pointer hover:bg-accent-hover transition-colors"
            >
              重新加载
            </button>
          )}
        </div>
      </div>
    )
  return (
    <MainLayout>
      {route === 'home' && <HomePage />}
      {route === 'image' && <ImageRecognition />}
      {route === 'settings' && <Settings />}
      {route === 'history' && <History />}
      {route === 'upgrade' && <UpgradePage />}
      {route === 'account' && <AccountPage />}
      <ToastContainer />
    </MainLayout>
  )
}

function App() {
  // Capsule window loads with #capsule hash — detect synchronously, no race condition
  if (window.location.hash === '#capsule') return <CapsuleApp />
  if (window.location.hash === '#editor') return <EditorApp />
  return <MainApp />
}

export default App
