import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { Download, FolderOpen, Pause, RefreshCw, Trash2 } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import {
  cancelNativeAsr,
  deleteNativeAsr,
  downloadNativeAsr,
  getNativeAsrCatalog,
  getNativeAsrPaths,
  type NativeAsrModelInfo,
  type NativeAsrPaths,
  type NativeAsrProgress,
} from '../../lib/tauri'

export function NativeAsrPanel({ onReadyChange }: { onReadyChange: (ready: boolean) => void }) {
  const options = useAppStore((s) => s.config.native_asr)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const [paths, setPaths] = useState<NativeAsrPaths | null>(null)
  const [info, setInfo] = useState<NativeAsrModelInfo | null>(null)
  const [progress, setProgress] = useState<NativeAsrProgress | null>(null)
  const [busy, setBusy] = useState(false)
  const [action, setAction] = useState<'download' | 'delete' | null>(null)
  const [cancelling, setCancelling] = useState(false)
  const [error, setError] = useState('')
  const [hint, setHint] = useState('')
  const [confirmDelete, setConfirmDelete] = useState(false)
  const readyCallback = useRef(onReadyChange)
  readyCallback.current = onReadyChange
  const generation = useRef(0)
  const operation = useRef(false)
  const pathsRequest = useRef(0)
  const latestOptions = useRef(options)
  latestOptions.current = options
  useEffect(() => {
    const current = ++generation.current
    const request = ++pathsRequest.current
    operation.current = false
    setPaths(null)
    setInfo(null)
    setBusy(false)
    setAction(null)
    setCancelling(false)
    setError('')
    setProgress(null)
    setHint('')
    setConfirmDelete(false)
    readyCallback.current(false)
    void getNativeAsrPaths(options.model_id, options.model_dir)
      .then((value) => {
        if (generation.current !== current || pathsRequest.current !== request) return
        setPaths(value)
        readyCallback.current(value.ready)
      })
      .catch((reason) => {
        if (generation.current === current && pathsRequest.current === request)
          setError(String(reason))
      })
    void getNativeAsrCatalog()
      .then((catalog) => {
        if (generation.current === current)
          setInfo(catalog.find((model) => model.id === options.model_id) ?? null)
      })
      .catch((reason) => {
        if (generation.current === current) setError(String(reason))
      })
    const subscription = listen<NativeAsrProgress>(
      'native-asr:download-progress',
      ({ payload }) => {
        if (generation.current !== current || payload.model_id !== options.model_id) return
        setProgress(payload)
        if (payload.status === 'error') setError(payload.message)
        if (payload.status === 'cancelled') setHint(payload.message)
        // A download can continue after navigating away. Its old component no longer
        // owns the result, so the replacement panel rechecks the actual installation.
        if (['completed', 'done', 'ready', 'deleted'].includes(payload.status)) {
          const refresh = ++pathsRequest.current
          void getNativeAsrPaths(options.model_id, options.model_dir)
            .then((value) => {
              if (generation.current !== current || pathsRequest.current !== refresh) return
              setPaths(value)
              readyCallback.current(value.ready)
            })
            .catch((reason) => {
              if (generation.current === current && pathsRequest.current === refresh)
                setError(String(reason))
            })
        }
      },
    ).catch((reason) => {
      if (generation.current === current) setError(`下载进度暂不可用：${String(reason)}`)
      return null
    })
    return () => {
      generation.current = current + 1
      void subscription.then((unlisten) => unlisten?.()).catch(() => {})
    }
  }, [options.model_id, options.model_dir])

  async function run(action: 'download' | 'delete') {
    if (operation.current) return
    const current = generation.current
    operation.current = true
    pathsRequest.current++
    setBusy(true)
    setAction(action)
    setError('')
    setHint('')
    setProgress(null)
    setConfirmDelete(false)
    try {
      const result = await (action === 'download'
        ? downloadNativeAsr(options.model_id, options.model_dir)
        : deleteNativeAsr(options.model_id, options.model_dir))
      if (generation.current !== current) return
      pathsRequest.current++
      setPaths(result)
      readyCallback.current(result.ready)
      setProgress(null)
      setHint(
        action === 'download'
          ? '模型已校验并安装。点击页面底部“保存并应用”，下一段录音使用此模型；首次加载请稍候。'
          : '此模型已删除，其他模型和录音历史未受影响。',
      )
    } catch (reason) {
      if (generation.current === current) {
        setError(String(reason))
        setHint('')
        setProgress(null)
      }
    } finally {
      if (generation.current === current) {
        operation.current = false
        setBusy(false)
        setAction(null)
        setCancelling(false)
      }
    }
  }
  async function chooseDirectory() {
    const current = generation.current
    try {
      const folder = await open({ directory: true, multiple: false, title: '选择模型保存根目录' })
      if (generation.current === current && typeof folder === 'string') {
        updateConfig({ native_asr: { ...latestOptions.current, model_dir: folder } })
      }
    } catch (reason) {
      if (generation.current === current) setError(String(reason))
    }
  }
  async function cancelDownload() {
    if (cancelling) return
    const current = generation.current
    setCancelling(true)
    try {
      await cancelNativeAsr(options.model_id)
      if (generation.current === current) setHint('已请求取消下载，正在等待当前任务停止。')
    } catch (reason) {
      if (generation.current === current) setError(String(reason))
    } finally {
      if (generation.current === current) setCancelling(false)
    }
  }
  const button =
    'inline-flex items-center gap-2 rounded-[9px] border border-border px-3 py-2 text-[12px] disabled:opacity-50'
  const active =
    busy ||
    Boolean(
      progress &&
      ['starting', 'connecting', 'downloading', 'retrying', 'verifying', 'installing'].includes(
        progress.status,
      ),
    )
  const percent =
    progress && Number.isFinite(progress.percent) ? Math.max(0, Math.min(100, progress.percent)) : 0
  return (
    <section className="space-y-4" aria-label="原生语音模型管理">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className="font-semibold text-text-primary">{info?.name ?? options.model_id}</h4>
        <span className="text-[12px] text-text-secondary">
          {paths ? (paths.ready ? '已安装' : '尚未安装') : '正在检查安装状态'}
          {paths?.verified ? ' · SHA-256 已验证' : ''}
          {paths?.update_available ? ' · 有版本更新' : ''}
        </span>
      </div>
      <p className="text-[12px] leading-6 text-text-secondary">
        无需 Python 或
        GPU。模型按需下载，不增加初始软件包体积；本后端使用整段识别，暂不支持录音预览与识别阶段热词。
      </p>
      <div className="rounded-[10px] border border-border bg-bg-secondary p-3">
        <p className="text-[12px] font-medium">保存目录</p>
        <p className="mt-1 break-all text-[12px] text-text-secondary">
          {paths?.model_dir || options.model_dir || '.\\models\\native-asr\\' + options.model_id}
        </p>
        <div className="mt-3 flex flex-wrap gap-2">
          <button className={button} disabled={active} onClick={() => void chooseDirectory()}>
            <FolderOpen size={14} />
            自定义目录
          </button>
          {options.model_dir && (
            <button
              className={button}
              disabled={active}
              onClick={() => updateConfig({ native_asr: { ...options, model_dir: '' } })}
            >
              恢复软件旁 Model 目录
            </button>
          )}
        </div>
      </div>
      {progress && (
        <div role="status" className="space-y-2 text-[12px] text-text-secondary">
          <progress
            aria-label="模型下载进度"
            className="h-2 w-full accent-accent"
            max={100}
            value={percent}
          />
          <p>
            {progress.message} · {percent.toFixed(1)}% ·{' '}
            {(progress.speed_bytes_per_sec / 1048576).toFixed(2)} MB/s · 尝试 {progress.attempt}
          </p>
          <p className="break-all">{progress.source}</p>
        </div>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          className={button + ' bg-accent text-white'}
          disabled={active}
          onClick={() => void run('download')}
        >
          {paths?.ready ? <RefreshCw size={14} /> : <Download size={14} />}
          {error
            ? '重新尝试下载'
            : paths?.update_available
              ? '更新模型'
              : paths?.ready
                ? '检查并修复模型'
                : '下载模型'}
        </button>
        {active && action !== 'delete' && (
          <button className={button} disabled={cancelling} onClick={() => void cancelDownload()}>
            <Pause size={14} />
            {cancelling ? '正在取消' : '取消下载'}
          </button>
        )}
        {paths?.ready && (
          <button className={button} disabled={active} onClick={() => setConfirmDelete(true)}>
            <Trash2 size={14} />
            删除模型
          </button>
        )}
      </div>
      {confirmDelete && (
        <div role="alert" className="rounded-[9px] border border-border p-3 text-[12px]">
          仅删除上面目录中的此模型，使用时需重新下载。
          <div className="mt-2 flex gap-2">
            <button className={button} onClick={() => void run('delete')}>
              确认删除
            </button>
            <button className={button} onClick={() => setConfirmDelete(false)}>
              保留
            </button>
          </div>
        </div>
      )}
      {error && (
        <p role="alert" className="break-words text-[12px] text-red-600">
          {error}
        </p>
      )}
      {hint && (
        <p role="status" className="text-[12px] text-success">
          {hint}
        </p>
      )}
      <details className="text-[12px] text-text-secondary">
        <summary className="cursor-pointer">版本、下载备份与 CPU 设置</summary>
        <label className="mt-3 flex items-center gap-3">
          CPU 线程数
          <input
            aria-label="原生语音 CPU 线程数"
            type="number"
            min={1}
            max={16}
            value={options.num_threads}
            onChange={(event) =>
              updateConfig({
                native_asr: {
                  ...options,
                  num_threads: Math.max(
                    1,
                    Math.min(16, Math.round(Number(event.target.value)) || 1),
                  ),
                },
              })
            }
            className="w-16 rounded border border-border bg-bg-secondary p-1"
          />
        </label>
        <p className="mt-3 break-all">
          版本：{info?.revision} · 许可：{info?.license}
        </p>
        <p className="mt-2 leading-5">
          固定版本和 SHA-256
          校验；主源失败自动切换备用。升级软件的模型目录清单后，可检查并修复至清单固定版本。速度会随网络变化。
        </p>
        <ol className="mt-2 list-inside list-decimal space-y-1">
          {info?.sources.map((source) => (
            <li key={source.url} className="break-all">
              {source.name}：{source.url}
            </li>
          ))}
        </ol>
      </details>
    </section>
  )
}
