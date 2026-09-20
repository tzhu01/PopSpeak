import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useAuthStore } from '../authStore'

// Mock external dependencies
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('../../lib/auth-client', () => ({
  authClient: {
    getSession: vi.fn(),
    signIn: { email: vi.fn() },
    signUp: { email: vi.fn() },
    signOut: vi.fn(),
  },
}))

vi.mock('../../lib/api', () => ({
  getSubscriptionStatus: vi.fn(),
}))

vi.mock('../../components/Toast', () => ({
  toast: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import { authClient } from '../../lib/auth-client'
import { getSubscriptionStatus } from '../../lib/api'

function getState() {
  return useAuthStore.getState()
}

describe('authStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    // Reset store state
    useAuthStore.setState({
      user: null,
      plan: 'free',
      subscriptionEnd: null,
      sttSecondsUsed: 0,
      sttSecondsLimit: 0,
      llmTokensUsed: 0,
      llmTokensLimit: 0,
      loading: false,
      error: null,
    })

    // Set up mock implementations fresh each test
    vi.mocked(invoke).mockResolvedValue(undefined)
    vi.mocked(authClient.getSession).mockResolvedValue({ data: null } as never)
    vi.mocked(authClient.signOut).mockResolvedValue(undefined as never)
    vi.mocked(getSubscriptionStatus).mockResolvedValue({
      plan: 'pro',
      subscriptionEnd: '2025-12-31',
      sttSecondsUsed: 100,
      sttSecondsLimit: 36000,
      llmTokensUsed: 5000,
      llmTokensLimit: 5000000,
    })
  })

  describe('initial state', () => {
    it('starts with no user and free plan', () => {
      expect(getState().user).toBeNull()
      expect(getState().plan).toBe('free')
      expect(getState().loading).toBe(false)
      expect(getState().error).toBeNull()
    })
  })

  describe('signOut', () => {
    it('clears user and resets to free plan', async () => {
      useAuthStore.setState({
        user: { id: '1', email: 'test@example.com', name: 'Test' },
        plan: 'pro',
        subscriptionEnd: '2025-12-31',
        sttSecondsUsed: 100,
        sttSecondsLimit: 36000,
        llmTokensUsed: 5000,
        llmTokensLimit: 5000000,
      })

      await getState().signOut()

      expect(getState().user).toBeNull()
      expect(getState().plan).toBe('free')
      expect(getState().subscriptionEnd).toBeNull()
      expect(getState().sttSecondsUsed).toBe(0)
      expect(getState().llmTokensUsed).toBe(0)
    })
  })

  describe('refreshSubscription', () => {
    it('updates quota fields from API response', async () => {
      useAuthStore.setState({
        user: { id: '1', email: 'test@example.com', name: 'Test' },
      })

      await getState().refreshSubscription()

      expect(getState().plan).toBe('pro')
      expect(getState().subscriptionEnd).toBe('2025-12-31')
      expect(getState().sttSecondsUsed).toBe(100)
      expect(getState().sttSecondsLimit).toBe(36000)
      expect(getState().llmTokensUsed).toBe(5000)
      expect(getState().llmTokensLimit).toBe(5000000)
    })
  })

  describe('initialize', () => {
    it('sets loading during initialization', async () => {
      localStorage.setItem('session_token', 'existing-session')
      const promise = getState().initialize()
      expect(getState().loading).toBe(true)
      await promise
      expect(getState().loading).toBe(false)
    })

    it('stays null user when no session exists', async () => {
      await getState().initialize()
      expect(getState().user).toBeNull()
      expect(authClient.getSession).not.toHaveBeenCalled()
    })
  })

  it('does not persist a session that fails server validation', async () => {
    await expect(getState().handleDeepLinkToken('invalid-session')).rejects.toThrow()
    expect(localStorage.getItem('session_token')).toBeNull()
    expect(invoke).not.toHaveBeenCalled()
    expect(getState().user).toBeNull()
  })

  it('persists a server-validated session before refreshing entitlements', async () => {
    vi.mocked(authClient.getSession).mockResolvedValue({
      data: { user: { id: 'u1', email: 't@example.com', name: 'User' } },
    } as never)
    await getState().handleDeepLinkToken('validated-session')
    expect(localStorage.getItem('session_token')).toBe('validated-session')
    expect(invoke).toHaveBeenCalledWith('set_session_token', { token: 'validated-session' })
    expect(getState().user?.id).toBe('u1')
  })
})
