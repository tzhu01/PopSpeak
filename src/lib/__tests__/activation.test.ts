import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { act, renderHook, waitFor } from '@testing-library/react'
import {
  parseActivationStatus,
  publicAccountUrl,
  getPublicAccountEntry,
  useActivationStore,
  useActivationLifecycle,
  type ActivationStatus,
} from '../activation'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(vi.fn()) }))

const trial: ActivationStatus = {
  installation_id: 'test-installation',
  configured: true,
  activated: false,
  license_id: null,
  trial_recordings_used: 2,
  trial_recordings_limit: 200,
  trial_recordings_remaining: 198,
  trial_milliseconds_used: 10000,
  trial_milliseconds_limit: 1200000,
  trial_milliseconds_remaining: 1190000,
  trial_exhausted: false,
  recording_in_progress: false,
}
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(listen).mockResolvedValue(vi.fn())
  useActivationStore.setState({ status: null, loading: false, error: null })
})
afterEach(() => vi.unstubAllEnvs())

describe('native activation status', () => {
  it('accepts a complete native status and does not infer activated from remaining quota', () => {
    expect(parseActivationStatus(trial)).toEqual(trial)
    expect(parseActivationStatus({ ...trial, trial_exhausted: true }).activated).toBe(false)
  })
  it.each([
    null,
    {},
    { ...trial, activated: 'true' },
    { ...trial, trial_milliseconds_remaining: -1 },
    { ...trial, trial_recordings_limit: NaN },
    { ...trial, activated: true, license_id: null },
    { ...trial, activated: true, configured: false, license_id: 'license' },
  ])('rejects malformed status without unlocking features', (value) => {
    expect(() => parseActivationStatus(value)).toThrow()
  })
  it('loads the native ledger rather than local browser flags', async () => {
    localStorage.setItem('activated', 'true')
    vi.mocked(invoke).mockResolvedValue(trial)
    await useActivationStore.getState().refresh()
    expect(invoke).toHaveBeenCalledWith('get_activation_status')
    expect(useActivationStore.getState().status?.activated).toBe(false)
    localStorage.removeItem('activated')
  })
  it('fails closed if refreshing previously activated status fails', async () => {
    useActivationStore.setState({ status: { ...trial, activated: true, license_id: 'license' } })
    vi.mocked(invoke).mockRejectedValue(new Error('disk unavailable'))
    await useActivationStore.getState().refresh()
    expect(useActivationStore.getState().status).toBeNull()
    expect(useActivationStore.getState().error).toContain('无法读取')
  })
  it('submits a trimmed code and accepts only the native verified result', async () => {
    const activated = { ...trial, activated: true, license_id: 'signed-license' }
    vi.mocked(invoke).mockResolvedValue(activated)
    await expect(useActivationStore.getState().activate(' signed-code ')).resolves.toEqual(
      activated,
    )
    expect(invoke).toHaveBeenCalledWith('activate_license', { code: 'signed-code' })
    expect(useActivationStore.getState().status?.activated).toBe(true)
  })
  it('does not unlock on a successful command whose payload is not activated', async () => {
    vi.mocked(invoke).mockResolvedValue(trial)
    await expect(useActivationStore.getState().activate('bad-code')).rejects.toThrow('激活未完成')
    expect(useActivationStore.getState().status).toBeNull()
  })
  it('does not submit an empty code or echo a complete rejected credential', async () => {
    await expect(useActivationStore.getState().activate(' ')).rejects.toThrow('请先粘贴')
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockRejectedValue(new Error('rejected private-code'))
    await expect(useActivationStore.getState().activate('private-code')).rejects.toThrow(
      'rejected [激活码]',
    )
  })

  it('does not overwrite a newly verified license with an older pending status read', async () => {
    let finishRead: (value: ActivationStatus) => void = () => {}
    const pendingRead = new Promise<ActivationStatus>((resolve) => {
      finishRead = resolve
    })
    const activated = { ...trial, activated: true, license_id: 'newly-verified' }
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'get_activation_status' ? pendingRead : (Promise.resolve(activated) as any),
    )
    const reading = useActivationStore.getState().refresh()
    await useActivationStore.getState().activate('signed-code')
    finishRead(trial)
    await reading
    expect(useActivationStore.getState().status?.license_id).toBe('newly-verified')
  })

  it('establishes event subscription before reading initial status and cleans it up', async () => {
    let finishSubscription: (dispose: () => void) => void = () => {}
    const dispose = vi.fn()
    vi.mocked(listen).mockReturnValue(
      new Promise((resolve) => {
        finishSubscription = resolve
      }),
    )
    vi.mocked(invoke).mockResolvedValue(trial)
    const { unmount } = renderHook(useActivationLifecycle)
    expect(invoke).not.toHaveBeenCalled()
    await act(async () => {
      finishSubscription(dispose)
    })
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_activation_status'))
    unmount()
    expect(dispose).toHaveBeenCalledOnce()
  })
})

describe('public account entry URL boundary', () => {
  it.each([
    'https://mp.weixin.qq.com/cgi-bin/home?t=home/index&token=example',
    'https://mp.weixin.qq.com/wxamp/wacodepage/getcodepage',
    'https://example.com/qr?access_token=secret',
    'https://example.com/login',
    'https://user:password@example.com/qr.png',
    'http://example.com/qr.png',
    'https://127.0.0.1/qr.png',
    'https://localhost/qr',
    'https://192.168.1.1/qr',
    'https://[::1]/qr',
    'https://example.local/qr',
    'https://example.com/qr#token=secret',
    'javascript:alert(1)',
    'file:///tmp/qr.png',
  ])('rejects non-public or credential-bearing URL %s', (url) => {
    expect(publicAccountUrl(url)).toBeNull()
  })
  it('accepts public HTTPS articles and image URLs', () => {
    expect(publicAccountUrl('https://mp.weixin.qq.com/s/publicarticle')).toBe(
      'https://mp.weixin.qq.com/s/publicarticle',
    )
    expect(publicAccountUrl('https://cdn.example.com/account-qr.png')).toBe(
      'https://cdn.example.com/account-qr.png',
    )
  })
  it('does not invent a public account when deployment metadata is missing', () => {
    vi.stubEnv('VITE_PUBLIC_ACCOUNT_NAME', '')
    vi.stubEnv('VITE_PUBLIC_ACCOUNT_URL', '')
    vi.stubEnv('VITE_PUBLIC_ACCOUNT_QR_URL', '')
    expect(getPublicAccountEntry()).toEqual({ name: '', url: null, qrUrl: null })
  })
})
