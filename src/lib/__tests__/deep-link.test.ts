import { webcrypto } from 'node:crypto'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
const mocks = vi.hoisted(() => ({
  exchange: vi.fn(),
  accept: vi.fn(),
  refresh: vi.fn(),
  setState: vi.fn(),
}))
vi.mock('@tauri-apps/plugin-deep-link', () => ({ onOpenUrl: vi.fn() }))
vi.mock('../account-service', () => ({ exchangeOAuthCode: mocks.exchange }))
vi.mock('../../stores/authStore', () => ({
  useAuthStore: {
    getState: () => ({ handleDeepLinkToken: mocks.accept, refreshSubscription: mocks.refresh }),
    setState: mocks.setState,
  },
}))
import { clearOAuthState, generateOAuthRequest, handleDeepLinkUrl } from '../deep-link'

beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('crypto', webcrypto)
  mocks.exchange.mockResolvedValue({ token: 'validated-session-token' })
})
afterEach(() => {
  clearOAuthState()
  vi.unstubAllGlobals()
})

it('exchanges a one-time code with the matching PKCE verifier exactly once', async () => {
  const { state, codeChallenge } = await generateOAuthRequest()
  expect(codeChallenge).toMatch(/^[A-Za-z0-9_-]{43}$/)
  await handleDeepLinkUrl(`popspeak://auth/callback?code=one-time-code&state=${state}`)
  expect(mocks.exchange).toHaveBeenCalledTimes(1)
  const verifier = mocks.exchange.mock.calls[0][2]
  expect(verifier).toMatch(/^[A-Za-z0-9_-]{43}$/)
  const digest = await webcrypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))
  expect(Buffer.from(digest).toString('base64url')).toBe(codeChallenge)
  expect(mocks.accept).toHaveBeenCalledWith('validated-session-token')
  await handleDeepLinkUrl(`popspeak://auth/callback?code=one-time-code&state=${state}`)
  expect(mocks.exchange).toHaveBeenCalledTimes(1)
})

it('ignores an incorrect state without cancelling the legitimate pending login', async () => {
  const { state } = await generateOAuthRequest()
  await handleDeepLinkUrl('popspeak://auth/callback?code=injected-code&state=wrong')
  expect(mocks.exchange).not.toHaveBeenCalled()
  await handleDeepLinkUrl(`popspeak://auth/callback?code=valid-code-123&state=${state}`)
  expect(mocks.accept).toHaveBeenCalledTimes(1)
})

it('does not accept access tokens embedded in a URL', async () => {
  const { state } = await generateOAuthRequest()
  await handleDeepLinkUrl(`popspeak://auth/callback?token=exposed-token&state=${state}`)
  expect(mocks.exchange).not.toHaveBeenCalled()
  expect(mocks.accept).not.toHaveBeenCalled()
})

it('payment deep links only query the server and do not set membership', async () => {
  await handleDeepLinkUrl('popspeak://checkout/success?plan=pro')
  expect(mocks.refresh).toHaveBeenCalledTimes(1)
  expect(mocks.setState).not.toHaveBeenCalled()
  expect(mocks.accept).not.toHaveBeenCalled()
})
