import { useEffect } from 'react'
import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export interface ActivationStatus {
  installation_id: string
  configured: boolean
  activated: boolean
  license_id: string | null
  trial_recordings_used: number
  trial_recordings_limit: number
  trial_recordings_remaining: number
  trial_milliseconds_used: number
  trial_milliseconds_limit: number
  trial_milliseconds_remaining: number
  trial_exhausted: boolean
  recording_in_progress: boolean
}

interface ActivationState {
  status: ActivationStatus | null
  loading: boolean
  error: string | null
  refresh: () => Promise<void>
  activate: (code: string) => Promise<ActivationStatus>
}

// Only native verification can grant access. No localStorage flag or browser
// redirect is treated as activation evidence.
export function parseActivationStatus(value: unknown): ActivationStatus {
  if (!value || typeof value !== 'object') throw new Error('激活状态响应无效，请重试')
  const record = value as Record<string, unknown>
  const counts = [
    'trial_recordings_used',
    'trial_recordings_limit',
    'trial_recordings_remaining',
    'trial_milliseconds_used',
    'trial_milliseconds_limit',
    'trial_milliseconds_remaining',
  ]
  const flags = ['configured', 'activated', 'trial_exhausted', 'recording_in_progress']
  if (
    typeof record.installation_id !== 'string' ||
    !record.installation_id.trim() ||
    !(record.license_id === null || typeof record.license_id === 'string') ||
    counts.some((key) => !Number.isSafeInteger(record[key]) || Number(record[key]) < 0) ||
    flags.some((key) => typeof record[key] !== 'boolean') ||
    (record.activated && (!record.configured || !record.license_id))
  )
    throw new Error('激活状态响应无效，请重试')
  return record as unknown as ActivationStatus
}

let revision = 0
let refreshQueued = false
function acceptStatus(value: unknown) {
  const status = parseActivationStatus(value)
  revision += 1
  useActivationStore.setState({ status, error: null })
  return status
}

export const useActivationStore = create<ActivationState>((set, get) => ({
  status: null,
  loading: false,
  error: null,
  refresh: async () => {
    if (get().loading) {
      refreshQueued = true
      return
    }
    const atRevision = revision
    set({ loading: true, error: null })
    try {
      const result = await invoke<unknown>('get_activation_status')
      if (revision === atRevision) acceptStatus(result)
    } catch {
      if (revision === atRevision)
        set({ status: null, error: '无法读取本机激活状态，请重试。暂不显示任何已解锁权益。' })
    } finally {
      set({ loading: false })
      if (refreshQueued) {
        refreshQueued = false
        void get().refresh()
      }
    }
  },
  activate: async (code) => {
    if (!code.trim()) throw new Error('请先粘贴激活码')
    try {
      const result = await invoke<unknown>('activate_license', { code: code.trim() })
      const status = parseActivationStatus(result)
      if (!status.activated) throw new Error('激活未完成，请检查激活码是否属于这台电脑')
      return acceptStatus(status)
    } catch (error) {
      // Do not echo the entire credential if the native error contains it.
      const message = String(error)
        .replace(/^Error:\s*/i, '')
        .split(code.trim())
        .join('[激活码]')
      throw new Error(message || '激活失败，请检查激活码或联系发码人员')
    }
  },
}))

export function useActivationLifecycle() {
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<unknown>('activation:updated', (event) => {
      try {
        acceptStatus(event.payload)
      } catch {
        revision += 1
        useActivationStore.setState({ status: null, error: '激活状态更新无效，请重新检查。' })
      }
    })
      .then((stop) => {
        if (disposed) stop()
        else {
          unlisten = stop
          void useActivationStore.getState().refresh()
        }
      })
      .catch(() => {
        if (!disposed)
          useActivationStore.setState({
            status: null,
            error: '无法订阅激活状态更新，请点击重新检查。',
          })
      })
    const refresh = () => {
      void useActivationStore.getState().refresh()
    }
    window.addEventListener('focus', refresh)
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('focus', refresh)
    }
  }, [])
}

export interface PublicAccountEntry {
  name: string
  url: string | null
  qrUrl: string | null
}

export function publicAccountUrl(raw: string | undefined): string | null {
  if (!raw?.trim()) return null
  try {
    const url = new URL(raw)
    const host = url.hostname.toLowerCase()
    const path = decodeURIComponent(url.pathname).toLowerCase()
    if (
      url.protocol !== 'https:' ||
      url.username ||
      url.password ||
      url.hash ||
      host === 'localhost' ||
      host.endsWith('.localhost') ||
      host.endsWith('.local') ||
      !host.includes('.') ||
      /^\d+(?:\.\d+){3}$/.test(host) ||
      host.includes(':') ||
      /(?:^|\/)(?:cgi-bin|wxamp|login|admin|console|signin|oauth)(?:\/|$)/.test(path)
    )
      return null
    for (const key of url.searchParams.keys()) {
      if (/token|cookie|session|secret|password|auth|ticket/i.test(key)) return null
    }
    // Only public article/profile routes are allowed on the management domain.
    if (host === 'mp.weixin.qq.com' && path !== '/s' && !path.startsWith('/s/')) return null
    return url.href
  } catch {
    return null
  }
}

export function getPublicAccountEntry(): PublicAccountEntry {
  return {
    name: (import.meta.env.VITE_PUBLIC_ACCOUNT_NAME as string | undefined)?.trim() || '',
    url: publicAccountUrl(import.meta.env.VITE_PUBLIC_ACCOUNT_URL),
    qrUrl: publicAccountUrl(import.meta.env.VITE_PUBLIC_ACCOUNT_QR_URL),
  }
}
