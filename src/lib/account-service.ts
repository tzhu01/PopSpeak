import { API_BASE_URL } from './constants'

export type LoginProvider = 'wechat' | 'qq' | 'alipay' | 'google'
export type PaymentMethod = 'wechat' | 'alipay'
export interface AccountProduct {
  id: string
  name: string
  description: string
  priceFen: number
  currency: 'CNY'
}
export interface AccountCapabilities {
  ready: boolean
  sms: boolean
  oauth: LoginProvider[]
  payments: PaymentMethod[]
  products: AccountProduct[]
}
export interface AccountOrder {
  id: string
  status: 'pending' | 'paid' | 'closed' | 'refunding' | 'refunded'
  productName: string
  amountFen: number
  currency: 'CNY'
  checkoutUrl?: string
}
export const UNAVAILABLE_ACCOUNT: AccountCapabilities = {
  ready: false,
  sms: false,
  oauth: [],
  payments: [],
  products: [],
}

async function request<T>(path: string, body?: unknown, authenticated = true): Promise<T> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), 12000)
  try {
    const headers: Record<string, string> = { 'Content-Type': 'application/json' }
    const token = localStorage.getItem('session_token')
    if (authenticated && token) headers.Authorization = `Bearer ${token}`
    const response = await fetch(`${API_BASE_URL}${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
    })
    if (!response.ok) {
      if (response.status === 401) throw new Error('会话已失效，请重新登录')
      if (response.status === 429) throw new Error('操作太频繁，请稍后再试')
      throw new Error(`账号服务暂不可用（${response.status}），不影响离线输入`)
    }
    return (await response.json()) as T
  } catch (error) {
    if (error instanceof Error && error.name === 'AbortError')
      throw new Error('账号服务响应超时，请稍后重试')
    throw error
  } finally {
    clearTimeout(timeout)
  }
}
export async function getAccountCapabilities(): Promise<AccountCapabilities> {
  // A placeholder hostname is not an operator-configured account backend.
  if (!import.meta.env.VITE_API_BASE_URL) return UNAVAILABLE_ACCOUNT
  const data = await request<AccountCapabilities>('/api/desktop/capabilities', undefined, false)
  if (data?.ready !== true) return UNAVAILABLE_ACCOUNT
  return {
    ready: true,
    sms: data.sms === true,
    oauth: Array.isArray(data.oauth)
      ? data.oauth.filter((p) => ['wechat', 'qq', 'alipay', 'google'].includes(p))
      : [],
    payments: Array.isArray(data.payments)
      ? data.payments.filter((p) => ['wechat', 'alipay'].includes(p))
      : [],
    products: Array.isArray(data.products)
      ? data.products.filter(
          (p) =>
            p &&
            typeof p.id === 'string' &&
            typeof p.name === 'string' &&
            p.currency === 'CNY' &&
            Number.isSafeInteger(p.priceFen) &&
            p.priceFen > 0,
        )
      : [],
  }
}
export function sendSmsCode(phone: string) {
  return request<{ challengeId: string; retryAfterSeconds: number }>(
    '/api/desktop/auth/sms/send',
    { phone },
    false,
  )
}
export function verifySmsCode(phone: string, code: string, challengeId: string) {
  return request<{ token: string }>(
    '/api/desktop/auth/sms/verify',
    { phone, code, challengeId },
    false,
  )
}
export function startOAuth(provider: LoginProvider, state: string, codeChallenge: string) {
  return request<{ authorizeUrl: string }>(
    '/api/desktop/auth/oauth/start',
    {
      provider,
      state,
      codeChallenge,
      codeChallengeMethod: 'S256',
      redirectUri: 'popspeak://auth/callback',
    },
    false,
  )
}
export function exchangeOAuthCode(code: string, state: string, codeVerifier: string) {
  return request<{ token: string }>(
    '/api/desktop/auth/exchange',
    { code, state, codeVerifier },
    false,
  )
}
export function createAccountOrder(
  productId: string,
  method: PaymentMethod,
  idempotencyKey: string,
) {
  return request<AccountOrder>('/api/desktop/orders', { productId, method, idempotencyKey })
}
export async function getAccountOrder(id: string) {
  const order = await request<AccountOrder>(`/api/desktop/orders/${encodeURIComponent(id)}`)
  if (order.id !== id) throw new Error('订单响应不匹配，请重新查询')
  return order
}
export function trustedCashierUrl(raw: string): string {
  const url = new URL(raw)
  if (
    url.protocol !== 'https:' ||
    url.origin !== new URL(API_BASE_URL).origin ||
    url.username ||
    url.password
  )
    throw new Error('收银台地址不属于已配置的账号服务器')
  return url.href
}
export function trustedOAuthUrl(raw: string, provider: LoginProvider): string {
  const url = new URL(raw)
  const allowed: Record<LoginProvider, string[]> = {
    google: ['accounts.google.com'],
    qq: ['graph.qq.com'],
    wechat: ['open.weixin.qq.com'],
    alipay: ['openauth.alipay.com', 'auth.alipay.com'],
  }
  if (
    url.protocol !== 'https:' ||
    url.username ||
    url.password ||
    (!allowed[provider].includes(url.hostname) && url.origin !== new URL(API_BASE_URL).origin)
  )
    throw new Error('授权地址不在该登录平台的允许列表中')
  return url.href
}
