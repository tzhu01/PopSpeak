import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Settings, History, LogOut, CircleUser, AppWindow, EyeOff } from 'lucide-react'
import { useAppStore, type AppConfig } from '../../stores/appStore'

interface Props {
  onClose: () => void
}

export function CapsuleContextMenu({ onClose }: Props) {
  const [error, setError] = useState('')
  const [closing, setClosing] = useState(false)
  const openMainWindow = async (hash: string) => {
    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
      const { emitTo } = await import('@tauri-apps/api/event')
      const mainWin = await WebviewWindow.getByLabel('main')
      if (mainWin) {
        await mainWin.show()
        await mainWin.unminimize()
        await mainWin.setFocus()
        await emitTo('main', 'navigate', hash)
      }
    } catch {
      /* ignore – window may not exist */
    }
  }

  const items = [
    {
      icon: AppWindow,
      label: '打开主窗口',
      onClick: () => {
        openMainWindow('#/')
        onClose()
      },
    },
    { type: 'separator' as const },
    {
      icon: EyeOff,
      label: '关闭悬浮球',
      onClick: async () => {
        if (closing) return
        setClosing(true)
        try {
          const config = await invoke<AppConfig>('set_capsule_enabled', { enabled: false })
          useAppStore.getState().applyCapsulePreferences(config, { capsule_enabled: false })
          onClose()
        } catch {
          setError('未能保存，请重试')
        } finally {
          setClosing(false)
        }
      },
    },
    {
      icon: Settings,
      label: '设置',
      onClick: () => {
        openMainWindow('#/settings')
        onClose()
      },
    },
    {
      icon: History,
      label: '历史记录',
      onClick: () => {
        openMainWindow('#/history')
        onClose()
      },
    },
    {
      icon: CircleUser,
      label: '账号与激活',
      onClick: () => {
        openMainWindow('#/account')
        onClose()
      },
    },
    { type: 'separator' as const },
    {
      icon: LogOut,
      label: '退出程序',
      onClick: () => {
        import('@tauri-apps/api/core')
          .then(({ invoke }) => invoke('quit_app'))
          .catch((error) => console.error('退出程序失败:', error))
        onClose()
      },
    },
  ]

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        className="signal-menu relative z-50 w-[212px] py-1 rounded-[14px] shadow-float"
        style={{ maxWidth: '100%' }}
        role="menu"
      >
        {error && (
          <p role="alert" className="px-3 py-1 text-xs text-red-300">
            {error}
          </p>
        )}
        {items.map((item, i) => {
          if ('type' in item && item.type === 'separator') {
            return <div key={i} className="my-1 border-t border-border" />
          }
          const {
            icon: Icon,
            label,
            onClick,
          } = item as { icon: typeof Settings; label: string; onClick: () => void }
          return (
            <button
              key={label}
              onClick={onClick}
              role="menuitem"
              className="flex items-center gap-2.5 w-full px-3 py-1.5 text-[13px] text-white/85 hover:text-white hover:bg-white/10 transition-colors bg-transparent border-none cursor-pointer text-left"
            >
              <Icon size={14} />
              {label}
            </button>
          )
        })}
      </div>
    </>
  )
}
