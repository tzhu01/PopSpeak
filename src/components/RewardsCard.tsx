import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { Gift, RefreshCw, Sparkles } from 'lucide-react'
import { getRewardSummary, type RewardSummary } from '../lib/tauri'

/** Local-only experience points; no checkout, redemption, or cloud balance is implied. */
export function RewardsCard() {
  const [summary, setSummary] = useState<RewardSummary | null>(null)
  const [error, setError] = useState(false)
  const [loading, setLoading] = useState(false)
  const alive = useRef(false)
  const request = useRef(0)
  const refresh = useCallback(async () => {
    const current = ++request.current
    if (alive.current) setLoading(true)
    try {
      const result = await getRewardSummary()
      if (
        !result ||
        !Number.isFinite(result.total_points) ||
        !Number.isFinite(result.today_points) ||
        !Array.isArray(result.recent_activity)
      ) {
        throw new Error('Invalid local rewards summary')
      }
      if (alive.current && current === request.current) {
        setSummary(result)
        setError(false)
      }
    } catch {
      if (alive.current && current === request.current) setError(true)
    } finally {
      if (alive.current && current === request.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    alive.current = true
    void refresh()
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen('rewards:updated', () => void refresh())
      .then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
      .catch(() => {
        // A normal browser preview has no Tauri event bridge; focus still refreshes.
      })
    const onFocus = () => void refresh()
    window.addEventListener('focus', onFocus)
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh()
    }, 60_000)
    return () => {
      alive.current = false
      disposed = true
      unlisten?.()
      window.removeEventListener('focus', onFocus)
      window.clearInterval(timer)
    }
  }, [refresh])

  return (
    <section
      className="rounded-2xl border border-border bg-bg-primary p-5"
      aria-label="本机体验积分"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-[16px] font-semibold text-text-primary">
          <Sparkles size={18} className="text-accent" /> 本机体验积分
        </div>
        <button
          type="button"
          disabled
          title="公众号礼品兑换尚未上线，未来兑换需要服务端核验"
          className="flex items-center gap-1.5 rounded-lg border border-border px-3 py-2 text-[12px] text-text-secondary opacity-65"
        >
          <Gift size={14} /> 礼品兑换 · 待上线
        </button>
      </div>
      <div className="mt-4 flex items-end gap-7" aria-live="polite">
        <div>
          <span className="text-[30px] font-semibold leading-none tabular-nums text-text-primary">
            {summary?.total_points ?? '—'}
          </span>
          <span className="ml-2 text-[13px] text-text-secondary">累计积分</span>
        </div>
        <div className="pb-0.5 text-[13px] text-text-secondary">
          今日{' '}
          <span className="font-semibold text-text-primary">{summary?.today_points ?? '—'}</span>
          {' / '}
          {summary?.daily_limit ?? 100}
        </div>
      </div>
      {error ? (
        <div className="mt-3 flex items-center gap-3 text-[13px] text-text-secondary" role="status">
          积分读取失败，未影响识别和已有积分。
          <button
            type="button"
            disabled={loading}
            onClick={() => void refresh()}
            className="flex items-center gap-1 text-accent"
          >
            <RefreshCw size={13} className={loading ? 'animate-spin' : ''} /> 重试
          </button>
        </div>
      ) : null}
      <p className="mt-3 text-[13px] leading-6 text-text-secondary">
        每次成功识别有效语音满 {(summary?.minimum_duration_ms ?? 2000) / 1000} 秒，获得 1
        积分；每日最多 {summary?.daily_limit ?? 100} 积分。
      </p>
      <details className="mt-1 text-[12px] leading-6 text-text-secondary">
        <summary className="cursor-pointer select-none">积分规则与最近记录</summary>
        <p className="mt-2">
          从新版开始累计，旧历史不补发；失败、取消和修改历史不计分，删除历史不扣分。
          积分仅保存在本机，不等同于现金、会员权益或已承诺的礼品；公众号兑换待上线，后续兑换需服务端核验。
        </p>
        {summary?.recent_activity.length ? (
          <ul className="mt-2 space-y-1">
            {summary.recent_activity.map((activity) => (
              <li key={activity.history_id} className="flex justify-between gap-3">
                <span>
                  {new Date(activity.credited_at).toLocaleString('zh-CN', { hour12: false })} ·
                  成功识别
                </span>
                <span className="shrink-0 text-success">+{activity.points}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="mt-1">
            {summary ? '暂无积分记录，完成一次有效语音输入即可开始。' : '正在读取本机积分…'}
          </p>
        )}
      </details>
    </section>
  )
}
