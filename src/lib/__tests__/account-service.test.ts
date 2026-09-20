import { afterEach, describe, expect, it, vi } from 'vitest'
import { API_BASE_URL } from '../constants'
import {
  createAccountOrder,
  getAccountCapabilities,
  getAccountOrder,
  trustedCashierUrl,
  trustedOAuthUrl,
} from '../account-service'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
  localStorage.clear()
})

describe('account service contract', () => {
  it('does not connect to a placeholder server in the unconfigured portable build', async () => {
    vi.stubEnv('VITE_API_BASE_URL', '')
    const fetcher = vi.fn()
    vi.stubGlobal('fetch', fetcher)
    expect((await getAccountCapabilities()).ready).toBe(false)
    expect(fetcher).not.toHaveBeenCalled()
  })

  it('filters unsupported capabilities and invalid prices', async () => {
    vi.stubEnv('VITE_API_BASE_URL', API_BASE_URL)
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          ready: true,
          sms: true,
          oauth: ['wechat', 'unknown'],
          payments: ['alipay', 'card'],
          products: [
            { id: 'good', name: 'Support', priceFen: 990, currency: 'CNY' },
            { id: 'bad', name: 'Bad', priceFen: -1, currency: 'CNY' },
          ],
        }),
      }),
    )
    const result = await getAccountCapabilities()
    expect(result.oauth).toEqual(['wechat'])
    expect(result.payments).toEqual(['alipay'])
    expect(result.products.map((p) => p.id)).toEqual(['good'])
  })

  it('sends a stable idempotency key and product ID, not a client price', async () => {
    const fetcher = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ id: 'order-1' }) })
    vi.stubGlobal('fetch', fetcher)
    localStorage.setItem('session_token', 'test-session')
    await createAccountOrder('support-year', 'wechat', 'stable-key')
    const options = fetcher.mock.calls[0][1]
    expect(JSON.parse(options.body)).toEqual({
      productId: 'support-year',
      method: 'wechat',
      idempotencyKey: 'stable-key',
    })
    expect(options.headers.Authorization).toBe('Bearer test-session')
  })

  it('rejects another order returned from the server', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({ ok: true, json: async () => ({ id: 'different' }) }),
    )
    await expect(getAccountOrder('mine')).rejects.toThrow('订单响应不匹配')
  })

  it('rejects untrusted cashiers and spoofed OAuth hosts', () => {
    expect(trustedCashierUrl(`${API_BASE_URL}/cashier/1`)).toContain('/cashier/1')
    expect(() => trustedCashierUrl('https://evil.example/pay')).toThrow()
    expect(() => trustedCashierUrl('javascript:alert(1)')).toThrow()
    expect(trustedOAuthUrl('https://open.weixin.qq.com/connect/qrconnect', 'wechat')).toContain(
      'weixin.qq.com',
    )
    expect(() => trustedOAuthUrl('https://open.weixin.qq.com.evil.example/', 'wechat')).toThrow()
    expect(() => trustedOAuthUrl('https://accounts.google.com/', 'wechat')).toThrow()
    expect(() => trustedOAuthUrl('https://user:pass@accounts.google.com/', 'google')).toThrow()
  })
})
