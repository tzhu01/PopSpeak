import { Maximize2, Minus, X } from 'lucide-react'

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow()[action]()
}

/** A compact, app-coloured replacement for the mismatched native title bar. */
export function TitleBar() {
  return (
    <header
      className="app-titlebar flex h-9 shrink-0 items-center border-b border-border"
      data-tauri-drag-region
    >
      <div className="flex min-w-0 flex-1 items-center gap-2 pl-4" data-tauri-drag-region>
        <span className="app-titlebar-dot" data-tauri-drag-region />
        <span
          className="truncate text-[11px] font-semibold tracking-[0.04em] text-text-secondary"
          data-tauri-drag-region
        >
          PopSpeak
        </span>
      </div>
      <div className="flex h-full items-stretch">
        <button
          type="button"
          aria-label="最小化"
          title="最小化"
          onClick={() => void windowAction('minimize')}
          className="app-window-control"
        >
          <Minus size={14} />
        </button>
        <button
          type="button"
          aria-label="最大化或还原"
          title="最大化或还原"
          onClick={() => void windowAction('toggleMaximize')}
          className="app-window-control"
        >
          <Maximize2 size={12} />
        </button>
        <button
          type="button"
          aria-label="关闭窗口"
          title="关闭窗口"
          onClick={() => void windowAction('close')}
          className="app-window-control app-window-close"
        >
          <X size={15} />
        </button>
      </div>
    </header>
  )
}
