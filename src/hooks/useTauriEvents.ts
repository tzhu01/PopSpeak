import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useAppStore } from '../stores/appStore'
import type { AppConfig, PipelineState } from '../stores/appStore'
import { getHistory } from '../lib/tauri'
import { toast } from '../components/Toast'

interface PipelineStateEvent {
  session_id: number
  revision: number
  state: PipelineState
}

interface TranscriptEvent {
  session_id: number
  revision: number
  text: string
}

interface PipelineMessageEvent {
  session_id: number
  revision: number
  message: string
}

function validSessionId(sessionId: number) {
  return Number.isSafeInteger(sessionId) && sessionId >= 0
}

function validTranscriptEvent(payload: TranscriptEvent) {
  return (
    validSessionId(payload.session_id) &&
    Number.isSafeInteger(payload.revision) &&
    payload.revision >= 0 &&
    typeof payload.text === 'string'
  )
}

function validPipelineStateEvent(payload: PipelineStateEvent) {
  return (
    validSessionId(payload.session_id) &&
    Number.isSafeInteger(payload.revision) &&
    payload.revision >= 0 &&
    ['idle', 'recording', 'transcribing', 'polishing', 'outputting'].includes(payload.state)
  )
}

function validPipelineMessageEvent(payload: PipelineMessageEvent) {
  return (
    validSessionId(payload.session_id) &&
    Number.isSafeInteger(payload.revision) &&
    payload.revision >= 0 &&
    typeof payload.message === 'string'
  )
}

export function useTauriEvents() {
  const {
    setAudioVolume,
    setTargetApp,
    setPipelineError,
    setAccessibilityTrusted,
    setHistory,
    patchHistoryEntry,
    removeHistoryEntry,
    applyBackendConfig,
  } = useAppStore()

  useEffect(() => {
    let cancelled = false
    const unlisteners: Array<() => void> = []

    function addListener<T>(event: string, handler: (payload: T) => void) {
      listen<T>(event, (e) => handler(e.payload))
        .then((unlisten) => {
          if (cancelled) {
            unlisten()
          } else {
            unlisteners.push(unlisten)
          }
        })
        .catch((err) => {
          console.error(`Failed to register listener for "${event}":`, err)
        })
    }

    addListener<number>('audio:volume', setAudioVolume)
    addListener<TranscriptEvent>('stt:preview', (payload) => {
      if (!validTranscriptEvent(payload)) return
      const state = useAppStore.getState()
      if (
        state.activeSessionId !== payload.session_id ||
        state.pipelineState !== 'recording' ||
        state.authoritativeFinalSeen ||
        payload.revision <= state.lastPreviewRevision
      ) {
        return
      }
      useAppStore.setState({
        previewTranscript: payload.text,
        lastPreviewRevision: payload.revision,
      })
    })
    addListener<TranscriptEvent>('stt:partial', (payload) => {
      if (!validTranscriptEvent(payload)) return
      const state = useAppStore.getState()
      if (
        state.activeSessionId !== payload.session_id ||
        state.pipelineState === 'idle' ||
        state.lastResolvedRevision >= 0 ||
        payload.revision <= Math.max(state.lastPartialRevision, state.lastFinalRevision)
      ) {
        return
      }
      useAppStore.setState({
        partialTranscript: payload.text,
        lastPartialRevision: payload.revision,
      })
    })
    addListener<TranscriptEvent>('stt:final', (payload) => {
      if (!validTranscriptEvent(payload)) return
      const state = useAppStore.getState()
      if (
        state.activeSessionId !== payload.session_id ||
        state.pipelineState === 'idle' ||
        state.lastResolvedRevision >= 0 ||
        payload.revision <= Math.max(state.lastFinalRevision, state.lastPartialRevision)
      ) {
        return
      }
      useAppStore.setState({
        previewTranscript: '',
        partialTranscript: '',
        finalTranscript: payload.text,
        lastFinalRevision: payload.revision,
        authoritativeFinalSeen: true,
      })
    })
    addListener<TranscriptEvent>('llm:chunk', (payload) => {
      if (!validTranscriptEvent(payload)) return
      const state = useAppStore.getState()
      if (
        state.activeSessionId !== payload.session_id ||
        state.pipelineState === 'idle' ||
        state.lastResolvedRevision >= 0 ||
        payload.revision <= state.lastLlmRevision
      ) {
        return
      }
      useAppStore.setState({
        polishedText: state.polishedText + payload.text,
        lastLlmRevision: payload.revision,
      })
    })
    addListener<TranscriptEvent>('pipeline:resolved', (payload) => {
      if (!validTranscriptEvent(payload)) return
      const state = useAppStore.getState()
      if (
        state.activeSessionId !== payload.session_id ||
        state.pipelineState === 'idle' ||
        payload.revision <= state.lastResolvedRevision
      ) {
        return
      }
      useAppStore.setState({
        previewTranscript: '',
        partialTranscript: '',
        resolvedTranscript: payload.text,
        lastResolvedRevision: payload.revision,
        authoritativeFinalSeen: true,
      })
    })
    addListener<PipelineStateEvent>('pipeline:state', (payload) => {
      if (!validPipelineStateEvent(payload)) return
      const current = useAppStore.getState()

      if (payload.state === 'recording') {
        // A recording event establishes the current session. Session IDs are
        // monotonic, so a delayed start from an older run cannot take over.
        if (current.activeSessionId !== null && payload.session_id <= current.activeSessionId) {
          return
        }
        useAppStore.setState({
          pipelineState: 'recording',
          activeSessionId: payload.session_id,
          lastPipelineStateRevision: payload.revision,
          lastPipelineMessageRevision: -1,
          audioVolume: 0,
          previewTranscript: '',
          partialTranscript: '',
          finalTranscript: '',
          polishedText: '',
          resolvedTranscript: '',
          lastPreviewRevision: -1,
          lastPartialRevision: -1,
          lastFinalRevision: -1,
          lastLlmRevision: -1,
          lastResolvedRevision: -1,
          authoritativeFinalSeen: false,
          recordingDuration: 0,
        })
        // Clear any previous error when starting a new pipeline run
        setPipelineError(null)
        return
      }

      if (
        current.activeSessionId !== payload.session_id ||
        payload.revision <= current.lastPipelineStateRevision
      ) {
        return
      }
      useAppStore.setState({
        pipelineState: payload.state,
        lastPipelineStateRevision: payload.revision,
      })
      if (payload.state === 'idle') {
        // Don't clear pipelineError here — CapsuleError auto-resets after 2.5s.
        // Clearing here would swallow errors from failed start() calls that
        // transition Recording → Idle in rapid succession.
        getHistory(200, 0)
          .then(setHistory)
          .catch((err) => {
            console.error('Failed to refresh history:', err)
          })
      }
    })
    addListener<string>('pipeline:target_app', setTargetApp)
    addListener<string | PipelineMessageEvent>('pipeline:error', (payload) => {
      let error: string
      if (typeof payload === 'string') {
        error = payload
      } else {
        if (!payload || !validPipelineMessageEvent(payload)) return
        const current = useAppStore.getState()
        if (
          current.activeSessionId !== payload.session_id ||
          payload.revision <= current.lastPipelineMessageRevision
        ) {
          return
        }
        useAppStore.setState({ lastPipelineMessageRevision: payload.revision })
        error = payload.message
      }
      setPipelineError(error)
      if (error === 'ACCESSIBILITY_REQUIRED') {
        setAccessibilityTrusted(false)
      }
    })
    addListener<string>('pipeline:output_fallback', (message) => toast(message))
    addListener<string | PipelineMessageEvent>('pipeline:notice', (payload) => {
      let message: string
      if (typeof payload === 'string') {
        message = payload
      } else {
        if (!payload || !validPipelineMessageEvent(payload)) return
        const current = useAppStore.getState()
        if (
          current.activeSessionId !== payload.session_id ||
          payload.revision <= current.lastPipelineMessageRevision
        ) {
          return
        }
        useAppStore.setState({ lastPipelineMessageRevision: payload.revision })
        message = payload.message
      }
      toast(message)
    })
    addListener<{ id: number; polished_text: string }>('history:updated', (p) => {
      patchHistoryEntry(p.id, p.polished_text)
    })
    addListener<AppConfig>('config:updated', applyBackendConfig)
    addListener<{ id: number }>('history:deleted', (p) => removeHistoryEntry(p.id))

    addListener<void>('tray:settings', () => {
      window.location.hash = '#/settings'
    })
    addListener<void>('tray:history', () => {
      window.location.hash = '#/history'
    })
    addListener<string>('navigate', (hash) => {
      window.location.hash = hash
    })
    addListener<void>('tray:about', () => {
      window.location.hash = '#/settings'
    })

    return () => {
      cancelled = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [
    setAudioVolume,
    setTargetApp,
    setPipelineError,
    setAccessibilityTrusted,
    setHistory,
    patchHistoryEntry,
    removeHistoryEntry,
    applyBackendConfig,
  ])
}
