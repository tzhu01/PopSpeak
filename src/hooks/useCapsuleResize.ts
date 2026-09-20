import { useEffect, useRef, useState } from 'react'
import { useAppStore } from '../stores/appStore'
import { placeCapsule, capsuleContentSize, CAPSULE_INSET } from '../lib/capsuleGeometry'

export function useCapsuleResize(popupOpen = false) {
  const state = useAppStore((s) => s.pipelineState)
  const hasError = useAppStore((s) => s.pipelineError !== null)
  const menu = useAppStore((s) => s.contextMenuOpen)
  const enabled = useAppStore((s) => s.config.capsule_enabled)
  const topmost = useAppStore((s) => s.config.capsule_always_on_top)
  const autoHide = useAppStore((s) => s.config.capsule_auto_hide)
  const initialized = useRef(false)
  const queue = useRef(Promise.resolve())
  const revision = useRef(0)
  const [viewport, setViewport] = useState(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
    scale: window.devicePixelRatio || 1,
  }))
  const [readyLayout, setReadyLayout] = useState<{
    key: string
    width: number
    height: number
  } | null>(null)
  const content = capsuleContentSize(state, hasError, menu, popupOpen)
  const width = content.width + CAPSULE_INSET * 2
  const height = content.height + CAPSULE_INSET * 2
  const layoutKey = `${width}:${height}:${viewport.scale}`

  useEffect(() => {
    const updateViewport = () =>
      setViewport({
        width: window.innerWidth,
        height: window.innerHeight,
        scale: window.devicePixelRatio || 1,
      })
    window.addEventListener('resize', updateViewport)
    return () => window.removeEventListener('resize', updateViewport)
  }, [])

  useEffect(() => {
    const request = ++revision.current
    let cancelled = false
    // Serialize resize/show operations: a stale show must never undo Close.
    queue.current = queue.current
      .catch(() => {})
      .then(async () => {
        if (cancelled || request !== revision.current) return
        const { getCurrentWindow, PhysicalSize, PhysicalPosition, currentMonitor } =
          await import('@tauri-apps/api/window')
        const win = getCurrentWindow()
        if (!enabled) {
          await win.hide()
          return
        }
        const monitor = await currentMonitor()
        // CSS pixels follow the WebView's actual devicePixelRatio, including
        // Windows display scaling/zoom, not a stale monitor snapshot after drag.
        const scale = viewport.scale
        const size = {
          width: Math.ceil(width * scale),
          height: Math.ceil(height * scale),
        }
        let appliedSize = size
        await win.setAlwaysOnTop(topmost)
        if (monitor) {
          const pos = initialized.current ? await win.outerPosition() : null
          const oldSize = pos ? await win.outerSize() : null
          const area = monitor.workArea
          const rect = placeCapsule(
            {
              x: area.position.x,
              y: area.position.y,
              width: area.size.width,
              height: area.size.height,
            },
            size,
            pos && oldSize
              ? { x: pos.x, y: pos.y, width: oldSize.width, height: oldSize.height }
              : undefined,
            20 * scale,
          )
          await win.setSize(new PhysicalSize(rect.width, rect.height))
          await win.setPosition(new PhysicalPosition(rect.x, rect.y))
          appliedSize = rect
          initialized.current = true
        } else {
          await win.setSize(new PhysicalSize(size.width, size.height))
        }
        if (cancelled || request !== revision.current) return
        // A small work area can clamp the native size. Compare with that actual
        // CSS viewport, not the larger requested size that can never arrive.
        setReadyLayout({
          key: layoutKey,
          width: appliedSize.width / scale,
          height: appliedSize.height / scale,
        })
        if (!autoHide || state !== 'idle' || hasError || menu) await win.show()
        else await win.hide()
        if (menu) useAppStore.getState().setContextMenuReady(true)
      })
      .catch((error) => console.error('Unable to update floating recorder:', error))
    return () => {
      cancelled = true
    }
  }, [
    state,
    hasError,
    menu,
    enabled,
    topmost,
    autoHide,
    popupOpen,
    viewport.scale,
    width,
    height,
    layoutKey,
  ])

  // setSize resolves before the WebView necessarily receives WM_SIZE. Never
  // paint wide old/new content into an undersized native viewport during swaps.
  return (
    readyLayout?.key === layoutKey &&
    Math.abs(viewport.width - readyLayout.width) <= 1 &&
    Math.abs(viewport.height - readyLayout.height) <= 1
  )
}
