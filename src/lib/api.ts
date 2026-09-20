import { API_BASE_URL } from './constants'

const DEFAULT_TIMEOUT_MS = 30_000

function authHeaders(): Record<string, string> {
  const token = localStorage.getItem('session_token')
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function request<T>(
  path: string,
  options?: RequestInit & { timeoutMs?: number },
): Promise<T> {
  const { timeoutMs = DEFAULT_TIMEOUT_MS, ...fetchOptions } = options ?? {}
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)

  try {
    const res = await fetch(`${API_BASE_URL}${path}`, {
      ...fetchOptions,
      credentials: 'include',
      signal: controller.signal,
      headers: {
        'Content-Type': 'application/json',
        ...authHeaders(),
        ...fetchOptions?.headers,
      },
    })

    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw new ApiError(res.status, body.error ?? res.statusText)
    }

    return res.json()
  } finally {
    clearTimeout(timer)
  }
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

// Subscription
export interface SubscriptionStatus {
  plan: 'free' | 'pro'
  subscriptionEnd: string | null
  sttSecondsUsed: number
  sttSecondsLimit: number
  llmTokensUsed: number
  llmTokensLimit: number
}

export function getSubscriptionStatus(): Promise<SubscriptionStatus> {
  return request('/api/subscription/status')
}

// Checkout
export interface CheckoutResponse {
  url: string
}

export function createCheckout(origin: 'desktop' | 'web' = 'desktop'): Promise<CheckoutResponse> {
  return request('/api/checkout/create', {
    method: 'POST',
    body: JSON.stringify({ origin }),
  })
}

// Proxy STT
export async function proxyStt(audioBlob: Blob, language: string): Promise<{ text: string }> {
  const formData = new FormData()
  formData.append('audio', audioBlob)
  formData.append('language', language)

  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), 60_000)

  try {
    const res = await fetch(`${API_BASE_URL}/api/proxy/stt`, {
      method: 'POST',
      credentials: 'include',
      signal: controller.signal,
      headers: authHeaders(),
      body: formData,
    })

    if (!res.ok) {
      const body = await res.json().catch(() => ({ error: res.statusText }))
      throw new ApiError(res.status, body.error ?? res.statusText)
    }

    return res.json()
  } finally {
    clearTimeout(timer)
  }
}

// Proxy LLM
export function proxyLlm(
  messages: Array<{ role: string; content: string }>,
): Promise<{ text: string }> {
  return request('/api/proxy/llm', {
    method: 'POST',
    body: JSON.stringify({ messages }),
  })
}

// Backup
export function uploadBackup(data: {
  history?: unknown
  dictionary?: unknown
  settings?: unknown
}): Promise<{ success: boolean }> {
  return request('/api/backup/upload', {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

export function downloadBackup(): Promise<{
  history?: unknown
  dictionary?: unknown
  settings?: unknown
}> {
  return request('/api/backup/download')
}

// Scenes
export interface ScenePack {
  id: string
  name: string
  description: string
  category: string
  promptTemplate: string
  dictionaryTerms: Array<{ word: string; pronunciation?: string }>
  isPro: boolean
}

export function getScenes(): Promise<ScenePack[]> {
  return request('/api/scenes')
}

// Subscription portal
export function createPortalSession(): Promise<{ url: string }> {
  return request('/api/subscription/portal', { method: 'POST' })
}
