import { ArrowUpRight, CheckCircle2, Loader2, LockKeyhole, RefreshCw } from 'lucide-react'
import { useActivationStore } from '../lib/activation'
import { useRoute } from '../lib/router'

export function ActivationNotice({
  feature,
  compact = false,
}: {
  feature?: string
  compact?: boolean
}) {
  const { status, loading, error, refresh } = useActivationStore()
  const { navigate } = useRoute()
  if (status?.activated && !error) {
    if (feature || compact) return null
    return (
      <div className="flex items-center gap-2 rounded-xl border border-success/20 bg-success/5 p-4 text-[14px] text-success">
        <CheckCircle2 size={17} /> 本机已激活，离线试用限制已解除
      </div>
    )
  }
  const title = error
    ? '激活状态读取失败'
    : !status
      ? '正在读取激活状态'
      : status.recording_in_progress && !feature
        ? '正在录音 · 试用额度已预留'
        : feature
          ? `${feature}需先激活`
          : status.trial_exhausted
            ? '免费试用已用完'
            : '默认离线识别 · 免费试用'
  return (
    <div
      role={error ? 'alert' : 'status'}
      className={`rounded-xl border border-accent/20 bg-accent/5 ${compact ? 'px-4 py-2.5' : 'p-4'} text-text-primary`}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="flex items-center gap-2 text-[15px] font-semibold">
            {!status && !error ? (
              <Loader2 size={16} className="animate-spin" />
            ) : (
              <LockKeyhole size={16} className="text-accent" />
            )}
            {title}
          </p>
          <p className="mt-1 text-[12px] leading-5 text-text-secondary">
            {error ||
              (!status
                ? '以本机验证结果为准，请稍候。'
                : feature
                  ? '激活后才能保存或使用受限功能。已保存的词表和历史仍然保留；未激活时热词、纠错和后处理不会生效。'
                  : status.recording_in_progress
                    ? '当前录音占用的额度仅为预留，结束后按实际采音时长结算。此时显示为 0 不代表试用已经结束。'
                    : `剩余 ${status.trial_recordings_remaining} 次 / ${(status.trial_milliseconds_remaining / 1000).toFixed(1)} 秒，任一额度用完即结束试用。`)}
          </p>
        </div>
        {error ? (
          <button
            disabled={loading}
            onClick={() => void refresh()}
            className="flex shrink-0 items-center gap-1.5 rounded-lg border border-border px-3 py-2 text-[13px]"
          >
            <RefreshCw size={14} />
            重新检查
          </button>
        ) : (
          status && (
            <button
              onClick={() => navigate('account')}
              className="flex shrink-0 items-center gap-1.5 rounded-lg bg-accent px-3 py-2 text-[13px] text-white"
            >
              前往激活
              <ArrowUpRight size={14} />
            </button>
          )
        )}
      </div>
    </div>
  )
}
