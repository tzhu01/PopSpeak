import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { openUrl } from '@tauri-apps/plugin-opener'
import { AccountPage } from '../index'
import { useActivationStore, type ActivationStatus } from '../../../lib/activation'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(vi.fn()) }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../../../lib/router', () => ({ useRoute: () => ({ navigate: vi.fn() }) }))
const trial: ActivationStatus = {
  installation_id: 'installation-123',
  configured: true,
  activated: false,
  license_id: null,
  trial_recordings_used: 5,
  trial_recordings_limit: 200,
  trial_recordings_remaining: 195,
  trial_milliseconds_used: 40000,
  trial_milliseconds_limit: 1200000,
  trial_milliseconds_remaining: 1160000,
  trial_exhausted: false,
  recording_in_progress: false,
}
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubEnv('VITE_PUBLIC_ACCOUNT_NAME', '')
  vi.stubEnv('VITE_PUBLIC_ACCOUNT_URL', '')
  vi.stubEnv('VITE_PUBLIC_ACCOUNT_QR_URL', '')
  useActivationStore.setState({ status: null, error: null, loading: false })
  vi.mocked(invoke).mockResolvedValue(trial)
})
afterEach(() => {
  cleanup()
  vi.unstubAllEnvs()
})

describe('active account route is native activation', () => {
  it('shows actual remaining trial and an honest missing public-entry notice without fake login', async () => {
    render(<AccountPage />)
    expect(await screen.findByDisplayValue('installation-123')).toBeInTheDocument()
    expect(screen.getByText(/剩余 195 次 \/ 1160.0 秒/)).toBeInTheDocument()
    expect(screen.getByText(/公众号公开入口待配置/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '1 · 关注公众号' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '2 · 发送本机安装码' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '3 · 输入并验证激活码' })).toBeInTheDocument()
    expect(screen.queryByRole('img', { name: '公众号公开二维码' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /登录|支付|收银台/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '验证并激活' })).toBeDisabled()
  })
  it('copies installation ID only on explicit request', async () => {
    render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    expect(writeText).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '复制本机安装码' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('installation-123'))
  })
  it('activates only after native verification and removes the code input after success', async () => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === 'activate_license'
        ? { ...trial, activated: true, license_id: 'verified-license' }
        : trial,
    )
    render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    fireEvent.change(screen.getByLabelText('本机激活码'), {
      target: { value: 'signed-license-code' },
    })
    fireEvent.click(screen.getByRole('button', { name: '验证并激活' }))
    expect(await screen.findByText('本机已激活')).toBeInTheDocument()
    expect(invoke).toHaveBeenCalledWith('activate_license', { code: 'signed-license-code' })
    expect(screen.queryByLabelText('本机激活码')).not.toBeInTheDocument()
    expect(screen.getByText(/本地激活不包含云端 API 额度/)).toBeInTheDocument()
  })
  it('shows rejection without removing remaining trial or granting activation', async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'activate_license') throw new Error('激活码不属于这台电脑')
      return trial
    })
    render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    fireEvent.change(screen.getByLabelText('本机激活码'), {
      target: { value: 'wrong-device-code' },
    })
    fireEvent.click(screen.getByRole('button', { name: '验证并激活' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('不属于这台电脑')
    expect(screen.queryByText('本机已激活')).not.toBeInTheDocument()
    expect(useActivationStore.getState().status?.trial_recordings_remaining).toBe(195)
  })
  it('does not treat opening a public entry as following or activation', async () => {
    vi.stubEnv('VITE_PUBLIC_ACCOUNT_URL', 'https://mp.weixin.qq.com/s/publicaccount')
    render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    fireEvent.click(screen.getByRole('button', { name: '打开公众号公开入口' }))
    expect(openUrl).toHaveBeenCalledWith('https://mp.weixin.qq.com/s/publicaccount')
    expect(invoke).not.toHaveBeenCalledWith('activate_license', expect.anything())
    expect(useActivationStore.getState().status?.activated).toBe(false)
  })
  it('fails closed on loading error and offers a retry', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('cannot load ledger'))
    render(<AccountPage />)
    expect(await screen.findByRole('alert')).toHaveTextContent('无法读取本机激活状态')
    fireEvent.change(screen.getByLabelText('本机激活码'), { target: { value: 'some-code' } })
    expect(screen.getByRole('button', { name: '验证并激活' })).toBeDisabled()
    vi.mocked(invoke).mockResolvedValue(trial)
    fireEvent.click(screen.getByRole('button', { name: '重新检查激活状态' }))
    await screen.findByDisplayValue('installation-123')
    expect(screen.getByRole('button', { name: '验证并激活' })).toBeEnabled()
  })
  it('rejects a configured admin URL without embedding it into the page', async () => {
    vi.stubEnv(
      'VITE_PUBLIC_ACCOUNT_URL',
      'https://mp.weixin.qq.com/cgi-bin/home?token=private-token',
    )
    const { container } = render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    expect(screen.queryByRole('button', { name: '打开公众号公开入口' })).not.toBeInTheDocument()
    expect(container.innerHTML).not.toContain('private-token')
  })

  it('does not misreport reserved quota during recording as permanently exhausted', async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...trial,
      recording_in_progress: true,
      trial_exhausted: true,
      trial_milliseconds_used: 1200000,
      trial_milliseconds_remaining: 0,
    })
    render(<AccountPage />)
    await screen.findByDisplayValue('installation-123')
    expect(screen.getByText('正在录音 · 试用额度已预留')).toBeInTheDocument()
    expect(screen.queryByText('免费试用已用完')).not.toBeInTheDocument()
    expect(screen.queryByRole('region', { name: '试用用量' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '验证并激活' })).toBeDisabled()
  })
})
