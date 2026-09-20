import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { CloudAccountPage as AccountPage } from '../CloudAccountPage'
import {
  UNAVAILABLE_ACCOUNT,
  createAccountOrder,
  getAccountCapabilities,
  getAccountOrder,
  sendSmsCode,
  startOAuth,
  type AccountCapabilities,
  type AccountOrder,
} from '../../../lib/account-service'
import { API_BASE_URL } from '../../../lib/constants'

const auth = vi.hoisted(() => ({
  user: null as { id: string; email: string; name: string } | null,
  plan: 'free',
  subscriptionEnd: null,
  sttSecondsLimit: 0,
  sttSecondsUsed: 0,
  error: null,
  signOut: vi.fn().mockResolvedValue(undefined),
  refreshSubscription: vi.fn().mockResolvedValue(undefined),
  handleDeepLinkToken: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../../stores/authStore', () => ({
  useAuthStore: Object.assign(() => auth, {
    getState: () => auth,
    setState: vi.fn(),
  }),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }))
vi.mock('../../../lib/deep-link', () => ({
  clearOAuthState: vi.fn(),
  generateOAuthRequest: vi.fn(),
}))
vi.mock('../../../lib/account-service', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/account-service')>()
  return {
    ...actual,
    getAccountCapabilities: vi.fn(),
    createAccountOrder: vi.fn(),
    getAccountOrder: vi.fn(),
    sendSmsCode: vi.fn(),
    verifySmsCode: vi.fn(),
    startOAuth: vi.fn(),
  }
})

const product = {
  id: 'cloud-15-minutes',
  name: '云端体验 15 分钟',
  description: '一次性识别额度',
  priceFen: 100,
  currency: 'CNY' as const,
}

const available: AccountCapabilities = {
  ready: true,
  sms: false,
  oauth: [],
  payments: ['wechat'],
  products: [product],
}

const pendingOrder: AccountOrder = {
  id: 'order-1',
  productName: product.name,
  status: 'pending',
  amountFen: product.priceFen,
  currency: 'CNY',
  checkoutUrl: `${API_BASE_URL}/cashier/order-1`,
}

function signIn() {
  auth.user = { id: 'account-1', email: 'member@example.com', name: '测试用户' }
}

describe('Legacy cloud account (not the active account route)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    auth.user = null
    auth.plan = 'free'
    auth.error = null
    vi.mocked(getAccountCapabilities).mockResolvedValue(UNAVAILABLE_ACCOUNT)
  })

  afterEach(cleanup)

  it('keeps offline use free and disables unconfigured login even after consent', async () => {
    render(<AccountPage />)
    expect(await screen.findByText(/账号服务尚未开通或暂不可达/)).toBeInTheDocument()
    expect(screen.getByText(/打开就能说，离线永久免费/)).toBeInTheDocument()
    expect(
      screen.getByText(/热词、纠错、历史和本地润色均不需要付费或关注公众号/),
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('checkbox'))
    expect(screen.getByLabelText('手机号')).toBeDisabled()
    expect(screen.getByRole('button', { name: '获取验证码' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '验证并登录 / 注册' })).toBeDisabled()
    for (const name of ['微信', 'QQ', '支付宝', 'Google']) {
      expect(screen.getByRole('button', { name: `${name} 暂未开通` })).toBeDisabled()
    }
    expect(screen.getByText(/暂无可购买商品/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '前往收银台' })).not.toBeInTheDocument()
    expect(sendSmsCode).not.toHaveBeenCalled()
    expect(startOAuth).not.toHaveBeenCalled()
    expect(createAccountOrder).not.toHaveBeenCalled()
  })

  it('does not enable purchasing when a product exists but payment is not configured', async () => {
    signIn()
    vi.mocked(getAccountCapabilities).mockResolvedValue({ ...available, payments: [] })
    render(<AccountPage />)

    expect(await screen.findByText(`${product.name} · ¥1.00`)).toBeInTheDocument()
    expect(screen.getByRole('combobox')).toBeDisabled()
    const buy = screen.getByRole('button', { name: '前往收银台' })
    expect(buy).toBeDisabled()
    fireEvent.click(buy)
    expect(createAccountOrder).not.toHaveBeenCalled()
    expect(openUrl).not.toHaveBeenCalled()
  })

  it('rejects a changed order amount before opening a cashier and allows product refresh', async () => {
    signIn()
    vi.mocked(getAccountCapabilities).mockResolvedValue(available)
    vi.mocked(createAccountOrder).mockResolvedValue({ ...pendingOrder, amountFen: 200 })
    render(<AccountPage />)

    fireEvent.click(await screen.findByRole('button', { name: '前往收银台' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('订单金额发生变化')
    expect(createAccountOrder).toHaveBeenCalledWith(product.id, 'wechat', expect.any(String))
    expect(openUrl).not.toHaveBeenCalled()
    expect(auth.refreshSubscription).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: '重新打开收银台' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '刷新商品与支付方式' }))
    await waitFor(() => expect(getAccountCapabilities).toHaveBeenCalledTimes(2))
  })

  it('starts a new purchase after the server returns an already closed order', async () => {
    signIn()
    vi.mocked(getAccountCapabilities).mockResolvedValue(available)
    vi.mocked(createAccountOrder)
      .mockResolvedValueOnce({ ...pendingOrder, status: 'closed' })
      .mockResolvedValueOnce({ ...pendingOrder, id: 'order-2' })
    render(<AccountPage />)

    fireEvent.click(await screen.findByRole('button', { name: '前往收银台' }))
    expect(await screen.findByText('当前订单：已关闭')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: '前往收银台' })).toBeEnabled())
    expect(openUrl).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '前往收银台' }))
    await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1))
    const calls = vi.mocked(createAccountOrder).mock.calls
    expect(calls).toHaveLength(2)
    expect(calls[1][2]).not.toBe(calls[0][2])
    expect(screen.getByText('当前订单：待付款')).toBeInTheDocument()
  })

  it('starts a new purchase after querying a pending order that has closed', async () => {
    signIn()
    vi.mocked(getAccountCapabilities).mockResolvedValue(available)
    vi.mocked(createAccountOrder).mockResolvedValue(pendingOrder)
    vi.mocked(getAccountOrder).mockResolvedValue({ ...pendingOrder, status: 'closed' })
    render(<AccountPage />)

    fireEvent.click(await screen.findByRole('button', { name: '前往收银台' }))
    await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByRole('button', { name: '查询支付结果' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: '查询支付结果' }))
    expect(await screen.findByText('当前订单：已关闭')).toBeInTheDocument()
    expect(getAccountOrder).toHaveBeenCalledWith(pendingOrder.id)
    expect(auth.refreshSubscription).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.getByRole('button', { name: '前往收银台' })).toBeEnabled())

    fireEvent.click(screen.getByRole('button', { name: '前往收银台' }))
    await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(2))
    const calls = vi.mocked(createAccountOrder).mock.calls
    expect(calls[1][2]).not.toBe(calls[0][2])
  })
})
