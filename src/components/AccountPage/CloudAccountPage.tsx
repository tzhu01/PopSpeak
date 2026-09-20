import { useCallback, useEffect, useRef, useState } from 'react'
import { Cloud, Crown, LogOut, ShieldCheck, UserRound } from 'lucide-react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAuthStore } from '../../stores/authStore'
import { clearOAuthState, generateOAuthRequest } from '../../lib/deep-link'
import {
  UNAVAILABLE_ACCOUNT,
  createAccountOrder,
  getAccountCapabilities,
  getAccountOrder,
  sendSmsCode,
  startOAuth,
  trustedCashierUrl,
  trustedOAuthUrl,
  verifySmsCode,
  type AccountCapabilities,
  type AccountOrder,
  type LoginProvider,
  type PaymentMethod,
} from '../../lib/account-service'

const LOGIN_NAMES: Record<LoginProvider, string> = {
  wechat: '微信',
  qq: 'QQ',
  alipay: '支付宝',
  google: 'Google',
}
const ORDER_NAMES: Record<AccountOrder['status'], string> = {
  pending: '待付款',
  paid: '已支付',
  closed: '已关闭',
  refunding: '退款中',
  refunded: '已退款',
}
const field =
  'w-full rounded-xl border border-border bg-bg-primary px-4 py-3 text-[14px] outline-none focus:border-accent disabled:opacity-50'
const button =
  'rounded-xl border border-border px-4 py-2.5 text-[14px] font-medium hover:bg-accent/5 disabled:cursor-not-allowed disabled:opacity-45'
const section = 'rounded-[20px] border border-border bg-bg-elevated p-5'
const heading =
  'mb-4 flex items-center gap-2 rounded-xl border border-border bg-bg-secondary px-4 py-3 text-[19px] font-semibold'

export function CloudAccountPage() {
  const {
    user,
    plan,
    subscriptionEnd,
    sttSecondsLimit,
    sttSecondsUsed,
    error: authError,
  } = useAuthStore()
  const [capabilities, setCapabilities] = useState<AccountCapabilities>(UNAVAILABLE_ACCOUNT)
  const [checking, setChecking] = useState(true)
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [phone, setPhone] = useState('')
  const [code, setCode] = useState('')
  const [challenge, setChallenge] = useState('')
  const [cooldown, setCooldown] = useState(0)
  const [method, setMethod] = useState<PaymentMethod>('wechat')
  const [order, setOrder] = useState<AccountOrder | null>(null)
  const orderKeys = useRef(new Map<string, string>())
  const currentOrderKey = useRef<string | null>(null)
  const [consent, setConsent] = useState(false)

  const checkService = useCallback(async () => {
    setChecking(true)
    try {
      const result = await getAccountCapabilities()
      setCapabilities(result)
      if (result.payments[0]) setMethod(result.payments[0])
    } catch {
      setCapabilities(UNAVAILABLE_ACCOUNT)
    } finally {
      setChecking(false)
    }
  }, [])
  useEffect(() => {
    void checkService()
  }, [checkService])
  useEffect(() => {
    if (cooldown <= 0) return
    const timer = setTimeout(() => setCooldown((v) => Math.max(0, v - 1)), 1000)
    return () => clearTimeout(timer)
  }, [cooldown])
  useEffect(() => {
    setOrder(null)
    orderKeys.current.clear()
    currentOrderKey.current = null
  }, [user?.id])

  const run = async (action: () => Promise<void>) => {
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError('')
    setMessage('')
    useAuthStore.setState({ error: null })
    try {
      await action()
    } catch (e) {
      setError(e instanceof Error ? e.message : '操作失败，请稍后重试')
    } finally {
      pending.current = false
      setBusy(false)
    }
  }
  const refreshOrder = async () => {
    if (!order) return
    const latest = await getAccountOrder(order.id)
    setOrder(latest)
    if (['paid', 'closed', 'refunded'].includes(latest.status) && currentOrderKey.current) {
      orderKeys.current.delete(currentOrderKey.current)
    }
    if (latest.status === 'paid') {
      await useAuthStore.getState().refreshSubscription()
      setMessage('服务器已确认付款，已请求刷新账号权益。')
    } else {
      setMessage(`服务器返回：${ORDER_NAMES[latest.status] || '待核对'}。`)
    }
  }
  const beginOAuth = (provider: LoginProvider) =>
    run(async () => {
      if (!capabilities.oauth.includes(provider) || !consent) return
      const { state, codeChallenge } = await generateOAuthRequest()
      try {
        const result = await startOAuth(provider, state, codeChallenge)
        await openUrl(trustedOAuthUrl(result.authorizeUrl, provider))
        setMessage('请在系统浏览器完成授权，完成后会返回 PopSpeak。授权链接 5 分钟内有效。')
      } catch (e) {
        clearOAuthState()
        throw e
      }
    })

  return (
    <div className="mx-auto w-full max-w-[1040px] space-y-5 p-6 text-text-primary">
      <header className={section}>
        <p className="brand-kicker mb-2">ACCOUNT · MEMBERSHIP</p>
        <h1 className="brand-display text-[28px] font-semibold">账号与会员</h1>
        <p className="mt-3 text-[15px] leading-relaxed text-text-secondary">
          打开就能说，离线永久免费。仅在需要云端服务时登录。
        </p>
        <div className="mt-4 flex items-center gap-2 text-[13px] text-success">
          <ShieldCheck size={17} />
          热词、纠错、历史和本地润色均不需要付费或关注公众号。
        </div>
      </header>

      {!capabilities.ready && (
        <div
          role="status"
          className="rounded-xl border border-warning/30 bg-warning/5 p-4 text-[14px] leading-relaxed"
        >
          {checking
            ? '正在检查账号服务…'
            : '账号服务尚未开通或暂不可达。登录与支付暂不可用，不影响离线输入。'}
          <button
            className={`${button} ml-3`}
            disabled={checking || busy}
            onClick={() => void checkService()}
          >
            重新检查
          </button>
        </div>
      )}
      {(error || authError) && (
        <p role="alert" className="rounded-xl bg-error/10 p-4 text-[14px] text-error">
          {error || authError}
        </p>
      )}
      {message && (
        <p role="status" className="rounded-xl bg-success/10 p-4 text-[14px] text-success">
          {message}
        </p>
      )}

      <section className={section}>
        <h2 className={heading}>
          <UserRound size={20} />
          {user ? '我的账号' : '登录与注册'}
        </h2>
        {user ? (
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <p className="text-[17px] font-semibold">{user.name || 'PopSpeak 用户'}</p>
              <p className="mt-1 text-[13px] text-text-secondary">
                {user.email || '已验证账号'} · {plan === 'pro' ? '支持会员' : '免费账号'}
              </p>
            </div>
            <button
              className={`${button} flex items-center gap-2`}
              disabled={busy}
              onClick={() => void run(() => useAuthStore.getState().signOut())}
            >
              <LogOut size={16} />
              退出登录
            </button>
          </div>
        ) : (
          <>
            <p className="mb-3 text-[13px] text-text-secondary">
              首次验证会创建账号。不想登录可直接返回首页继续使用。
            </p>
            <label className="mb-4 flex items-start gap-2 text-[13px] leading-relaxed">
              <input
                type="checkbox"
                checked={consent}
                onChange={(e) => setConsent(e.target.checked)}
                className="mt-1"
              />
              我同意将手机号或所选平台的必要账号信息发送给运营方，用于登录及账号服务。
            </label>
            <fieldset disabled={busy || !capabilities.sms || !consent} className="space-y-3">
              <label className="block text-[14px] font-medium">
                手机号（中国大陆）
                <input
                  aria-label="手机号"
                  type="tel"
                  autoComplete="tel"
                  placeholder="输入 11 位手机号"
                  value={phone}
                  onChange={(e) => {
                    setPhone(e.target.value)
                    setChallenge('')
                    setCode('')
                  }}
                  className={`${field} mt-2`}
                />
              </label>
              <div className="flex gap-3">
                <input
                  aria-label="短信验证码"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  maxLength={6}
                  placeholder="短信验证码"
                  value={code}
                  onChange={(e) => setCode(e.target.value.replace(/\D/g, ''))}
                  className={field}
                />
                <button
                  className={`${button} shrink-0`}
                  disabled={!/^1[3-9]\d{9}$/.test(phone) || cooldown > 0}
                  onClick={() =>
                    void run(async () => {
                      const result = await sendSmsCode(`+86${phone}`)
                      if (!result.challengeId) throw new Error('短信服务未返回有效验证请求')
                      setChallenge(result.challengeId)
                      setCooldown(
                        Math.max(30, Math.min(300, Number(result.retryAfterSeconds) || 60)),
                      )
                      setMessage('验证码已发送，请查收手机短信。')
                    })
                  }
                >
                  {cooldown > 0 ? `${cooldown}s 后重发` : '获取验证码'}
                </button>
              </div>
              <button
                className={`${button} w-full bg-accent text-white`}
                disabled={!challenge || !/^\d{6}$/.test(code)}
                onClick={() =>
                  void run(async () => {
                    const result = await verifySmsCode(`+86${phone}`, code, challenge)
                    if (!result.token) throw new Error('登录服务未返回有效会话')
                    await useAuthStore.getState().handleDeepLinkToken(result.token)
                    setCode('')
                    setChallenge('')
                  })
                }
              >
                验证并登录 / 注册
              </button>
            </fieldset>
            {!capabilities.sms && (
              <p className="mt-2 text-[12px] text-text-tertiary">手机号验证尚未开通</p>
            )}
            <div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-4">
              {(Object.keys(LOGIN_NAMES) as LoginProvider[]).map((provider) => (
                <button
                  key={provider}
                  className={button}
                  disabled={busy || !consent || !capabilities.oauth.includes(provider)}
                  onClick={() => void beginOAuth(provider)}
                >
                  {LOGIN_NAMES[provider]}
                  <span className="mt-1 block text-[11px] font-normal">
                    {capabilities.oauth.includes(provider) ? '授权登录' : '暂未开通'}
                  </span>
                </button>
              ))}
            </div>
            <p className="mt-3 text-[12px] leading-relaxed text-text-tertiary">
              使用系统浏览器授权，不索取社交账号密码。Google
              需要相应网络可达；微信登录不等于关注公众号。
            </p>
          </>
        )}
      </section>

      <section className={section}>
        <h2 className={heading}>
          <Crown size={20} />
          会员与云端额度
        </h2>
        {capabilities.ready && (
          <button
            className={`${button} mb-4`}
            disabled={busy || checking}
            onClick={() => void checkService()}
          >
            刷新商品与支付方式
          </button>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="rounded-xl border border-border p-4">
            <p className="text-[14px] font-semibold">支持会员</p>
            <p className="mt-2 text-[20px] font-semibold">
              {user && plan === 'pro' ? '已开通' : '离线功能永久免费'}
            </p>
            <p className="mt-2 text-[13px] text-text-secondary">
              {subscriptionEnd
                ? `有效期至 ${subscriptionEnd.slice(0, 10)}`
                : '建议 9.9 元 / 年，权益上线后再开放购买；不含无限云识别。'}
            </p>
          </div>
          <div className="rounded-xl border border-border p-4">
            <p className="flex items-center gap-2 text-[14px] font-semibold">
              <Cloud size={16} />
              云端用量
            </p>
            <p className="mt-2 text-[20px] font-semibold">
              {user
                ? `${Math.max(0, Math.floor((sttSecondsLimit - sttSecondsUsed) / 60))} 分钟`
                : '登录后查询'}
            </p>
            <p className="mt-2 text-[13px] text-text-secondary">
              单独按量购买。额度不足或云端不可用，不影响本地输入。
            </p>
          </div>
        </div>
        {capabilities.products.length > 0 ? (
          <>
            <label className="mt-5 block text-[14px] font-medium">
              支付方式
              <select
                className={`${field} mt-2`}
                value={method}
                onChange={(e) => setMethod(e.target.value as PaymentMethod)}
                disabled={busy || !capabilities.payments.length}
              >
                {capabilities.payments.map((p) => (
                  <option key={p} value={p}>
                    {p === 'wechat' ? '微信支付' : '支付宝'}
                  </option>
                ))}
              </select>
            </label>
            <div className="mt-3 space-y-3">
              {capabilities.products.map((product) => (
                <div
                  key={product.id}
                  className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border p-4"
                >
                  <div>
                    <p className="text-[16px] font-semibold">
                      {product.name} · ¥{(product.priceFen / 100).toFixed(2)}
                    </p>
                    <p className="mt-1 text-[13px] text-text-secondary">{product.description}</p>
                  </div>
                  <button
                    className={`${button} bg-accent text-white`}
                    disabled={busy || !user || !capabilities.payments.includes(method)}
                    onClick={() =>
                      void run(async () => {
                        const key = `${product.id}:${method}`
                        if (!orderKeys.current.has(key))
                          orderKeys.current.set(key, crypto.randomUUID())
                        const created = await createAccountOrder(
                          product.id,
                          method,
                          orderKeys.current.get(key)!,
                        )
                        if (
                          !created.id ||
                          created.currency !== 'CNY' ||
                          !Number.isSafeInteger(created.amountFen) ||
                          created.amountFen !== product.priceFen
                        )
                          throw new Error('订单金额发生变化，请刷新商品后重新确认')
                        setOrder(created)
                        currentOrderKey.current = key
                        if (['paid', 'closed', 'refunded'].includes(created.status)) {
                          orderKeys.current.delete(key)
                          setMessage('该订单已结束；可查询状态，或重新发起购买。')
                          return
                        }
                        if (created.checkoutUrl)
                          await openUrl(trustedCashierUrl(created.checkoutUrl))
                        setMessage(
                          '订单已创建。请在收银台核对金额并付款，完成后点击“查询支付结果”。',
                        )
                      })
                    }
                  >
                    {user ? '前往收银台' : '登录后购买'}
                  </button>
                </div>
              ))}
            </div>
          </>
        ) : (
          <p className="mt-4 rounded-xl border border-dashed border-border p-4 text-[14px] text-text-secondary">
            暂无可购买商品。微信支付、支付宝将在商户及服务端配置完成后开放，不会提前收款。
          </p>
        )}
        {order && (
          <div className="mt-4 rounded-xl border border-accent/30 bg-accent/5 p-4 text-[14px]">
            <p className="font-semibold">当前订单：{ORDER_NAMES[order.status] || '待核对'}</p>
            <p className="mt-2 break-all">
              {order.productName} · ¥{(order.amountFen / 100).toFixed(2)} · {order.id}
            </p>
            <div className="mt-3 flex flex-wrap gap-3">
              <button className={button} disabled={busy} onClick={() => void run(refreshOrder)}>
                查询支付结果
              </button>
              {order.checkoutUrl && order.status === 'pending' && (
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await openUrl(trustedCashierUrl(order.checkoutUrl!))
                    })
                  }
                >
                  重新打开收银台
                </button>
              )}
            </div>
            <p className="mt-2 text-[12px] text-text-secondary">
              会员及额度以服务器验签、查单后的结果为准。关闭收银台不代表支付失败。
            </p>
          </div>
        )}
      </section>
      <footer className="px-2 text-[12px] leading-relaxed text-text-tertiary">
        公众号用于自愿获取教程、更新和客服，不以关注或分享解锁离线功能。退出登录不会删除本机历史、词典或识别配置。
        {busy ? ' 正在处理…' : ''}
      </footer>
    </div>
  )
}
