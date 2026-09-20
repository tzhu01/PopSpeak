import { useEffect, useRef, useState } from 'react'
import {
  CheckCircle2,
  ClipboardCopy,
  ExternalLink,
  KeyRound,
  Loader2,
  MessageCircle,
  RefreshCw,
  ShieldCheck,
} from 'lucide-react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getPublicAccountEntry, useActivationStore } from '../../lib/activation'
import { ActivationNotice } from '../ActivationNotice'
import { RewardsCard } from '../RewardsCard'

const panel = 'rounded-[20px] border border-border bg-bg-elevated/75 p-5'
const heading =
  'mb-4 flex items-center gap-2 rounded-xl border border-border bg-bg-secondary px-4 py-3 text-[19px] font-semibold'
const button =
  'flex items-center justify-center gap-2 rounded-xl border border-border px-4 py-2.5 text-[14px] font-medium disabled:cursor-not-allowed disabled:opacity-45'

export function ActivationPage() {
  const { status, loading, refresh, activate } = useActivationStore()
  const publicAccount = getPublicAccountEntry()
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [qrFailed, setQrFailed] = useState(false)
  const pending = useRef(false)
  useEffect(() => {
    void refresh()
  }, [refresh])

  const redeem = async () => {
    if (pending.current || !code.trim()) return
    pending.current = true
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await activate(code)
      setCode('')
      setMessage('激活成功，授权已保存在本机，重启后仍然有效。')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : '激活未完成，请核对激活码后重试。')
    } finally {
      pending.current = false
      setBusy(false)
    }
  }

  return (
    <div className="mx-auto w-full max-w-[1040px] space-y-5 p-6 text-text-primary">
      <header className={panel}>
        <p className="brand-kicker mb-2">ACTIVATE · LOCAL ACCESS</p>
        <h1 className="brand-display text-[28px] font-semibold">公众号激活</h1>
        <p className="mt-3 text-[15px] leading-7 text-text-secondary">
          默认离线识别可直接试用。关注公众号领取本机激活码，解除离线试用限制，解锁精确识别、多语种、热词、纠错和后处理。
        </p>
        <p className="mt-2 flex items-center gap-2 text-[13px] text-text-secondary">
          <ShieldCheck size={16} className="text-success" />{' '}
          不需要注册账号或提供社交账号密码；本地激活不包含云端 API 额度。
        </p>
      </header>
      <ActivationNotice />
      <RewardsCard />
      {status && !status.activated && !status.recording_in_progress && (
        <section className={panel} aria-label="试用用量">
          <h2 className={heading}>默认离线 · 剩余试用</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <Usage
              label="录音次数"
              used={status.trial_recordings_used}
              limit={status.trial_recordings_limit}
              unit="次"
            />
            <Usage
              label="累计录音时长"
              used={status.trial_milliseconds_used / 1000}
              limit={status.trial_milliseconds_limit / 1000}
              unit="秒"
            />
          </div>
          <p className="mt-3 text-[13px] text-text-secondary">
            累计次数或累计时长任一用完即结束试用；关闭或重启软件不会恢复额度。历史记录仍可查看、编辑和删除。
          </p>
        </section>
      )}
      {status?.activated ? (
        <section className={`${panel} border-success/25`}>
          <h2 className={heading}>
            <CheckCircle2 className="text-success" size={21} /> 本机已激活
          </h2>
          <p className="text-[15px] leading-7">
            默认离线、精确离线、多语种离线、热词、纠错、文字润色已解除激活限制。
          </p>
          <p className="mt-2 text-[13px] leading-6 text-text-secondary">
            仍需安装所选识别组件，并保存相关功能设置。超长录音仍受设备资源与安全上限约束；云端服务按厂商
            API 规则和价格使用。
          </p>
          <p className="mt-3 break-all text-[12px] text-text-tertiary">
            授权编号：{status.license_id}
          </p>
        </section>
      ) : (
        <div className="grid items-start gap-5 xl:grid-cols-2">
          <section className={panel}>
            <h2 className={heading}>
              <MessageCircle size={21} /> 1 · 关注公众号
            </h2>
            {publicAccount.name && (
              <p className="text-[20px] font-semibold">{publicAccount.name}</p>
            )}
            {publicAccount.qrUrl && !qrFailed && (
              <img
                src={publicAccount.qrUrl}
                alt="公众号公开二维码"
                referrerPolicy="no-referrer"
                onError={() => setQrFailed(true)}
                className="mx-auto my-4 h-[180px] w-[180px] rounded-xl border border-border bg-white object-contain p-2"
              />
            )}
            {publicAccount.url && (
              <button
                className={`${button} mt-3 w-full bg-accent text-white`}
                onClick={() =>
                  void openUrl(publicAccount.url!).catch(() =>
                    setError('无法打开公众号入口，请使用微信搜索公众号名称。'),
                  )
                }
              >
                <ExternalLink size={16} /> 打开公众号公开入口
              </button>
            )}
            {!publicAccount.url && (!publicAccount.qrUrl || qrFailed) && (
              <div className="mt-3 rounded-xl border border-warning/30 bg-warning/5 p-3 text-[14px] leading-6">
                公众号公开入口待配置。请向运营方索取公众号名称或公开二维码；后台管理链接无法用于用户关注。
              </div>
            )}
            <p className="mt-4 text-[14px] leading-7 text-text-secondary">
              在公众号中按运营方提示领取激活码，并提供下方“本机安装码”。激活码绑定本机，打开链接或扫码本身不会自动激活。
            </p>
            <h2 className={`${heading} mt-5`}>
              <ClipboardCopy size={21} /> 2 · 发送本机安装码
            </h2>
            <p className="mb-3 text-[13px] leading-6 text-text-secondary">
              复制安装码发送给公众号运营方，收到为这台电脑签发的完整激活码后，再完成第 3 步。
            </p>
            <label className="mt-4 block text-[14px] font-semibold" htmlFor="installation-id">
              本机安装码
            </label>
            <input
              id="installation-id"
              readOnly
              value={status?.installation_id ?? ''}
              placeholder="等待本机状态读取"
              className="mt-2 w-full rounded-xl border border-border bg-bg-secondary px-3 py-3 font-mono text-[13px] outline-none"
            />
            <button
              className={`${button} mt-3 w-full`}
              disabled={!status?.installation_id}
              onClick={() => {
                if (!status) return
                void writeText(status.installation_id)
                  .then(() => setMessage('本机安装码已复制，请发送给公众号运营方。'))
                  .catch(() => setError('复制失败，请在安装码框中手动选择并复制。'))
              }}
            >
              <ClipboardCopy size={16} /> 复制本机安装码
            </button>
          </section>
          <section className={panel}>
            <h2 className={heading}>
              <KeyRound size={21} /> 3 · 输入并验证激活码
            </h2>
            {status && !status.configured && (
              <p
                role="alert"
                className="mb-3 rounded-xl border border-warning/30 bg-warning/5 p-3 text-[14px] leading-6"
              >
                此版本尚未配置授权验证公钥，请联系运营方提供已配置的版本。不会把任意输入当成激活成功。
              </p>
            )}
            <form
              onSubmit={(event) => {
                event.preventDefault()
                void redeem()
              }}
            >
              <label className="block text-[14px] font-semibold" htmlFor="activation-code">
                本机激活码
              </label>
              <textarea
                id="activation-code"
                value={code}
                onChange={(event) => {
                  setCode(event.target.value)
                  setError('')
                }}
                autoComplete="off"
                spellCheck={false}
                placeholder="粘贴公众号发给你的完整激活码"
                rows={5}
                disabled={busy}
                className="mt-2 w-full resize-y rounded-xl border border-border bg-bg-primary px-3 py-3 text-[13px] leading-6 outline-none focus:border-accent"
              />
              <button
                type="submit"
                disabled={
                  busy ||
                  loading ||
                  !status?.configured ||
                  status.recording_in_progress ||
                  !code.trim()
                }
                className={`${button} mt-4 w-full bg-accent text-white`}
              >
                {busy ? <Loader2 size={16} className="animate-spin" /> : <KeyRound size={16} />}
                {busy ? '正在验证激活码…' : '验证并激活'}
              </button>
            </form>
            <p className="mt-3 text-[12px] leading-6 text-text-secondary">
              激活码由本机原生程序校验并保存，重启后继续有效。不得使用他人电脑的激活码；更换电脑请联系运营方重新发码。
            </p>
            {status?.recording_in_progress && (
              <p className="mt-2 text-[13px] text-warning">请先结束当前录音再激活。</p>
            )}
          </section>
        </div>
      )}
      {error && (
        <p role="alert" className="rounded-xl bg-error/10 p-4 text-[14px] leading-6 text-error">
          {error}
        </p>
      )}
      {message && (
        <p role="status" className="rounded-xl bg-success/10 p-4 text-[14px] text-success">
          {message}
        </p>
      )}
      <button
        className={`${button} ml-auto`}
        disabled={loading || busy}
        onClick={() => void refresh()}
      >
        <RefreshCw size={15} className={loading ? 'animate-spin' : ''} /> 重新检查激活状态
      </button>
      <footer className="px-1 text-[12px] leading-6 text-text-tertiary">
        本软件不读取微信登录
        Cookie，不收集社交账号密码，不根据点击链接推测关注结果。是否发码由运营方审核；云端
        API、服务商账号与本机激活是独立事项。
      </footer>
    </div>
  )
}

function Usage({
  label,
  used,
  limit,
  unit,
}: {
  label: string
  used: number
  limit: number
  unit: string
}) {
  return (
    <div className="rounded-xl border border-border p-4">
      <p className="text-[14px] font-semibold">{label}</p>
      <p className="mt-2 text-[22px] font-semibold">
        {Math.max(0, limit - used).toFixed(unit === '秒' ? 1 : 0)}{' '}
        <span className="text-[13px] font-normal text-text-secondary">{unit}剩余</span>
      </p>
      <progress
        aria-label={label}
        value={Math.min(used, limit)}
        max={Math.max(1, limit)}
        className="mt-3 h-2 w-full accent-accent"
      />
      <p className="mt-1 text-[12px] text-text-tertiary">
        已用 {used.toFixed(unit === '秒' ? 1 : 0)} / {limit} {unit}
      </p>
    </div>
  )
}
