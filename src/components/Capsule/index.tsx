import { useRef, useCallback, useEffect, useState } from 'react'
import { ChevronUp } from 'lucide-react'
import { motion } from 'framer-motion'
import { useAppStore } from '../../stores/appStore'
import { useRecording } from '../../hooks/useRecording'
import { useCapsuleResize } from '../../hooks/useCapsuleResize'
import { CapsuleIdle } from './CapsuleIdle'
import { CapsuleRecording } from './CapsuleRecording'
import { CapsuleProcessing } from './CapsuleProcessing'
import { CapsulePolishing } from './CapsulePolishing'
import { CapsuleComplete } from './CapsuleComplete'
import { CapsuleError } from './CapsuleError'
import { CapsuleContextMenu } from './CapsuleContextMenu'
import { CapsulePop } from './CapsulePop'
import { capsuleContentSize, CAPSULE_INSET } from '../../lib/capsuleGeometry'

const DRAG_THRESHOLD = 5

function getCapsuleState(pipelineState: string, hasError: boolean) {
  if (hasError) return 'error'
  return pipelineState
}

export function Capsule() {
  const pipelineState = useAppStore((s) => s.pipelineState)
  const pipelineError = useAppStore((s) => s.pipelineError)
  const contextMenuOpen = useAppStore((s) => s.contextMenuOpen)
  const setContextMenuOpen = useAppStore((s) => s.setContextMenuOpen)
  const contextMenuReady = useAppStore((s) => s.contextMenuReady)
  const setContextMenuReady = useAppStore((s) => s.setContextMenuReady)
  const previewEnabled = useAppStore((s) => s.config.capsule_preview_enabled)
  const [collapsed, setCollapsed] = useState(false)
  const { startRecording, stopRecording, isRecording, isProcessing } = useRecording()

  const dragStart = useRef<{ x: number; y: number } | null>(null)
  const isDragging = useRef(false)

  const popupOpen =
    previewEnabled &&
    !collapsed &&
    pipelineState !== 'idle' &&
    pipelineError === null &&
    !contextMenuOpen
  const layoutReady = useCapsuleResize(popupOpen) !== false
  const compactSize = capsuleContentSize(pipelineState, pipelineError !== null, false, false)

  useEffect(() => {
    if (pipelineState !== 'recording') return
    setCollapsed(false)
    const started = Date.now()
    const tick = () =>
      useAppStore.getState().setRecordingDuration(Math.floor((Date.now() - started) / 1000))
    tick()
    const timer = setInterval(tick, 250)
    return () => clearInterval(timer)
  }, [pipelineState])

  const hasError = pipelineError !== null
  const capsuleState = getCapsuleState(pipelineState, hasError)

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return
    if ((e.target as HTMLElement).closest('button, input, textarea, a')) return
    dragStart.current = { x: e.clientX, y: e.clientY }
    isDragging.current = false
  }, [])

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragStart.current || isDragging.current) return
    const dx = e.clientX - dragStart.current.x
    const dy = e.clientY - dragStart.current.y
    if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) {
      isDragging.current = true
      dragStart.current = null
      import('@tauri-apps/api/window')
        .then(({ getCurrentWindow }) => {
          getCurrentWindow()
            .startDragging()
            .catch(() => {})
        })
        .catch(() => {})
    }
  }, [])

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return
      if ((e.target as HTMLElement).closest('button, input, textarea, a')) return
      if (isDragging.current) {
        isDragging.current = false
        dragStart.current = null
        return
      }
      dragStart.current = null

      if (isRecording) {
        void stopRecording().catch(console.error)
      } else if (!isProcessing && !hasError && pipelineState === 'idle') {
        void startRecording().catch(console.error)
      }
    },
    [isRecording, isProcessing, hasError, pipelineState, startRecording, stopRecording],
  )

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault()
    if (!contextMenuOpen) {
      setContextMenuOpen(true)
    }
  }

  const handleCloseMenu = () => {
    setContextMenuReady(false)
    setContextMenuOpen(false)
  }

  return (
    <div
      className="w-full h-full relative"
      style={{ background: 'transparent', visibility: layoutReady ? 'visible' : 'hidden' }}
      onContextMenu={handleContextMenu}
    >
      {popupOpen ? (
        <div className="absolute bottom-3 right-3">
          <CapsulePop
            onCollapse={() => setCollapsed(true)}
            onStop={() => {
              void stopRecording().catch(console.error)
            }}
          />
        </div>
      ) : (
        <>
          {/* Persistent, compact recorder surface. */}
          <motion.div
            style={{
              position: 'absolute',
              right: CAPSULE_INSET,
              bottom: CAPSULE_INSET,
              width: compactSize.width,
              maxWidth: 'calc(100vw - 24px)',
              height: compactSize.height,
              boxSizing: 'border-box',
            }}
            className={`signal-capsule absolute bottom-3 right-3 rounded-full pointer-events-auto shrink-0 ${
              capsuleState === 'error'
                ? 'jelly-capsule-error'
                : capsuleState === 'idle'
                  ? 'jelly-capsule text-white'
                  : 'jelly-capsule-active text-white'
            }`}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
          >
            <div
              className="flex h-full w-full min-w-0 items-center overflow-hidden"
              key={capsuleState}
            >
              {capsuleState === 'idle' && <CapsuleIdle />}
              {previewEnabled && capsuleState !== 'idle' && capsuleState !== 'error' && (
                <button
                  className="ml-2 rounded-full p-1 hover:bg-white/15"
                  aria-label="展开预览"
                  onClick={() => setCollapsed(false)}
                >
                  <ChevronUp size={14} />
                </button>
              )}
              <div className="min-w-0 flex-1">
                {capsuleState === 'recording' && <CapsuleRecording />}
                {capsuleState === 'transcribing' && <CapsuleProcessing />}
                {capsuleState === 'polishing' && <CapsulePolishing />}
                {capsuleState === 'outputting' && <CapsuleComplete />}
                {capsuleState === 'error' && <CapsuleError />}
              </div>
            </div>
          </motion.div>
        </>
      )}

      {/* Open upward, remaining inside the monitor work area. */}
      {contextMenuOpen && contextMenuReady && (
        <div
          className="absolute bottom-[60px] right-3"
          style={{
            maxWidth: 'calc(100vw - 24px)',
            maxHeight: 'calc(100vh - 72px)',
            overflowY: 'auto',
          }}
        >
          <CapsuleContextMenu onClose={handleCloseMenu} />
        </div>
      )}
    </div>
  )
}
