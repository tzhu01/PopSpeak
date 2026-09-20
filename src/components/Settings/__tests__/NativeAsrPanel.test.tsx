import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { listen, type EventCallback } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '../../../stores/appStore'
import * as api from '../../../lib/tauri'
import type { NativeAsrModelInfo, NativeAsrPaths, NativeAsrProgress } from '../../../lib/tauri'
import { NativeAsrPanel } from '../NativeAsrPanel'

vi.mock('../../../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../lib/tauri')>()
  return Object.fromEntries(
    Object.entries(actual).map(([key, value]) => [
      key,
      typeof value === 'function' ? vi.fn() : value,
    ]),
  )
})
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const first = 'qwen3-asr-1.7b'
const second = 'parakeet-unified-en-0.6b'
const catalog: NativeAsrModelInfo[] = [
  {
    id: first,
    name: 'Qwen3-ASR 1.7B',
    file_name: 'qwen.gguf',
    bytes: 100,
    sha256: 'abc',
    revision: 'fixed-revision',
    license: 'Apache-2.0',
    upstream_url: 'https://example.org/model',
    sources: [
      { name: '国内主源', url: 'https://example.org/main' },
      { name: '备用源', url: 'https://example.org/backup' },
    ],
  },
  {
    id: second,
    name: 'Parakeet Unified EN 0.6B',
    file_name: 'parakeet.gguf',
    bytes: 100,
    sha256: 'def',
    revision: 'second-revision',
    license: 'CC-BY-4.0',
    upstream_url: 'https://example.org/second',
    sources: [],
  },
]
function paths(id = first, dir = '', ready = false): NativeAsrPaths {
  return {
    model_id: id,
    model_dir: dir ? `${dir}\\${id}` : `D:\\PopSpeak\\models\\native-asr\\${id}`,
    model_path: 'model.gguf',
    display_dir: `.\\models\\native-asr\\${id}`,
    source: 'package-relative',
    is_custom: !!dir,
    ready,
    verified: ready,
    installed_bytes: ready ? 100 : 0,
    expected_bytes: 100,
    model_version: 'fixed-revision',
    update_available: false,
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}
let callbacks: EventCallback<NativeAsrProgress>[]
let unlisten: ReturnType<typeof vi.fn>
function progress(overrides: Partial<NativeAsrProgress> = {}) {
  const payload: NativeAsrProgress = {
    model_id: first,
    current: 25,
    total: 100,
    percent: 25,
    status: 'downloading',
    message: '正在下载',
    speed_bytes_per_sec: 2097152,
    attempt: 2,
    source: '国内主源',
    ...overrides,
  }
  act(() =>
    callbacks.forEach((callback) =>
      callback({ event: 'native-asr:download-progress', id: 1, payload }),
    ),
  )
}
function changeModel(id = second, dir = '') {
  act(() =>
    useAppStore.getState().updateConfig({
      native_asr: { ...useAppStore.getState().config.native_asr, model_id: id, model_dir: dir },
    }),
  )
}

beforeEach(() => {
  vi.resetAllMocks()
  useAppStore.setState(useAppStore.getInitialState())
  callbacks = []
  unlisten = vi.fn()
  vi.mocked(listen).mockImplementation(async (_event, callback) => {
    callbacks.push(callback as EventCallback<NativeAsrProgress>)
    return unlisten
  })
  vi.mocked(api.getNativeAsrPaths).mockImplementation(async (id, dir) => paths(id, dir))
  vi.mocked(api.getNativeAsrCatalog).mockResolvedValue(catalog)
  vi.mocked(api.downloadNativeAsr).mockImplementation(async (id, dir) => paths(id, dir, true))
  vi.mocked(api.deleteNativeAsr).mockImplementation(async (id, dir) => paths(id, dir))
  vi.mocked(api.cancelNativeAsr).mockResolvedValue(undefined)
})
afterEach(cleanup)

describe('native ASR model management', () => {
  it('loads install state and renders named source objects without remote assets', async () => {
    const onReady = vi.fn()
    const { container } = render(<NativeAsrPanel onReadyChange={onReady} />)
    await screen.findByRole('heading', { name: 'Qwen3-ASR 1.7B' })
    expect(screen.getByText('尚未安装')).toBeInTheDocument()
    expect(screen.getByText('国内主源：https://example.org/main')).toBeInTheDocument()
    expect(screen.getByText(/fixed-revision.*Apache-2.0/)).toBeInTheDocument()
    expect(container.querySelectorAll('img')).toHaveLength(0)
    expect(onReady).toHaveBeenLastCalledWith(false)
  })

  it('downloads with progress, speed and retry source then reports verified installation', async () => {
    const download = deferred<NativeAsrPaths>()
    vi.mocked(api.downloadNativeAsr).mockReturnValueOnce(download.promise)
    const onReady = vi.fn()
    render(<NativeAsrPanel onReadyChange={onReady} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    expect(api.downloadNativeAsr).toHaveBeenCalledExactlyOnceWith(first, '')
    progress({ status: 'retrying', message: '重试备用源', source: '备用源' })
    expect(screen.getByRole('progressbar')).toHaveAttribute('value', '25')
    expect(screen.getByText(/重试备用源 · 25.0% · 2.00 MB\/s · 尝试 2/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '自定义目录' })).toBeDisabled()
    await act(async () => download.resolve(paths(first, '', true)))
    expect(screen.getByText('已安装 · SHA-256 已验证')).toBeInTheDocument()
    expect(screen.getByText(/模型已校验并安装/)).toBeInTheDocument()
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
    expect(onReady).toHaveBeenLastCalledWith(true)
  })

  it('clears an active progress event on download failure so retry is enabled', async () => {
    const download = deferred<NativeAsrPaths>()
    vi.mocked(api.downloadNativeAsr).mockReturnValueOnce(download.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    progress()
    await act(async () => download.reject(new Error('网络连接失败')))
    expect(screen.getByRole('alert')).toHaveTextContent('网络连接失败')
    const retry = screen.getByRole('button', { name: '重新尝试下载' })
    expect(retry).toBeEnabled()
    fireEvent.click(retry)
    await screen.findByText('已安装 · SHA-256 已验证')
    expect(api.downloadNativeAsr).toHaveBeenCalledTimes(2)
  })

  it('requests cancellation without reporting success before the running download stops', async () => {
    const download = deferred<NativeAsrPaths>()
    vi.mocked(api.downloadNativeAsr).mockReturnValueOnce(download.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    fireEvent.click(screen.getByRole('button', { name: '取消下载' }))
    await screen.findByText(/已请求取消下载/)
    expect(api.cancelNativeAsr).toHaveBeenCalledExactlyOnceWith(first)
    expect(screen.getByRole('button', { name: '下载模型' })).toBeDisabled()
    await act(async () => download.reject('下载已取消'))
    expect(screen.getByRole('button', { name: '重新尝试下载' })).toBeEnabled()
  })

  it('reports cancellation failures and discards a late cancellation error after switching models', async () => {
    vi.mocked(api.cancelNativeAsr).mockRejectedValueOnce('无法取消，请重试')
    const cancellation = deferred<void>()
    vi.mocked(api.cancelNativeAsr).mockReturnValueOnce(cancellation.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    progress({ status: 'connecting' })
    expect(screen.getByRole('button', { name: '自定义目录' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: '取消下载' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('无法取消，请重试')
    fireEvent.click(screen.getByRole('button', { name: '取消下载' }))
    changeModel()
    await act(async () => cancellation.reject('obsolete cancel failure'))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('refreshes actual install state on completed events from a background download', async () => {
    const onReady = vi.fn()
    render(<NativeAsrPanel onReadyChange={onReady} />)
    await screen.findByText('尚未安装')
    vi.mocked(api.getNativeAsrPaths).mockResolvedValueOnce(paths(first, '', true))
    progress({ status: 'completed', percent: 100, message: '模型安装完成' })
    await screen.findByText('已安装 · SHA-256 已验证')
    expect(onReady).toHaveBeenLastCalledWith(true)
    expect(screen.getByRole('button', { name: '检查并修复模型' })).toBeEnabled()
  })

  it('requires confirmation to delete only the selected model and updates installation state', async () => {
    vi.mocked(api.getNativeAsrPaths).mockResolvedValue(paths(first, '', true))
    const onReady = vi.fn()
    render(<NativeAsrPanel onReadyChange={onReady} />)
    await screen.findByText('已安装 · SHA-256 已验证')
    fireEvent.click(screen.getByRole('button', { name: '删除模型' }))
    expect(api.deleteNativeAsr).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '保留' }))
    expect(screen.queryByRole('button', { name: '确认删除' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '删除模型' }))
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    await screen.findByText('尚未安装')
    expect(api.deleteNativeAsr).toHaveBeenCalledExactlyOnceWith(first, '')
    expect(onReady).toHaveBeenLastCalledWith(false)
    expect(screen.getByText(/其他模型和录音历史未受影响/)).toBeInTheDocument()
  })

  it('preserves installed state if deletion fails and does not offer download cancellation', async () => {
    vi.mocked(api.getNativeAsrPaths).mockResolvedValue(paths(first, '', true))
    const deletion = deferred<NativeAsrPaths>()
    vi.mocked(api.deleteNativeAsr).mockReturnValueOnce(deletion.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('已安装 · SHA-256 已验证')
    fireEvent.click(screen.getByRole('button', { name: '删除模型' }))
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    expect(screen.queryByRole('button', { name: '取消下载' })).not.toBeInTheDocument()
    await act(async () => deletion.reject('文件占用，请停止识别后重试'))
    expect(screen.getByRole('alert')).toHaveTextContent('文件占用')
    expect(screen.getByText('已安装 · SHA-256 已验证')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '删除模型' })).toBeEnabled()
  })

  it('selects a custom root without losing newer CPU settings, and restores the relative default', async () => {
    const selection = deferred<string | null>()
    vi.mocked(open).mockReturnValueOnce(selection.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '自定义目录' }))
    fireEvent.change(screen.getByLabelText('原生语音 CPU 线程数'), { target: { value: '8' } })
    await act(async () => selection.resolve('E:\\Speech Models'))
    expect(useAppStore.getState().config.native_asr).toEqual({
      model_id: first,
      model_dir: 'E:\\Speech Models',
      num_threads: 8,
    })
    await waitFor(() =>
      expect(api.getNativeAsrPaths).toHaveBeenLastCalledWith(first, 'E:\\Speech Models'),
    )
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    await screen.findByText('已安装 · SHA-256 已验证')
    expect(api.downloadNativeAsr).toHaveBeenCalledWith(first, 'E:\\Speech Models')
    fireEvent.click(screen.getByRole('button', { name: '恢复软件旁 Model 目录' }))
    await waitFor(() => expect(api.getNativeAsrPaths).toHaveBeenLastCalledWith(first, ''))
    expect(useAppStore.getState().config.native_asr.model_dir).toBe('')
  })

  it('handles directory cancellation and errors without changing configuration', async () => {
    vi.mocked(open).mockResolvedValueOnce(null).mockRejectedValueOnce('目录选择失败')
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '自定义目录' }))
    await act(async () => {})
    expect(useAppStore.getState().config.native_asr.model_dir).toBe('')
    fireEvent.click(screen.getByRole('button', { name: '自定义目录' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('目录选择失败')
  })

  it('does not apply an old directory dialog result after switching models', async () => {
    const selection = deferred<string | null>()
    vi.mocked(open).mockReturnValueOnce(selection.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '自定义目录' }))
    changeModel()
    await act(async () => selection.resolve('E:\\Wrong model'))
    expect(useAppStore.getState().config.native_asr).toMatchObject({
      model_id: second,
      model_dir: '',
    })
  })

  it('drops obsolete path, catalog, event and download results after a model switch', async () => {
    const lookup = deferred<NativeAsrPaths>()
    const metadata = deferred<NativeAsrModelInfo[]>()
    const download = deferred<NativeAsrPaths>()
    vi.mocked(api.getNativeAsrPaths).mockReturnValueOnce(lookup.promise)
    vi.mocked(api.getNativeAsrCatalog).mockReturnValueOnce(metadata.promise)
    vi.mocked(api.downloadNativeAsr).mockReturnValueOnce(download.promise)
    const onReady = vi.fn()
    render(<NativeAsrPanel onReadyChange={onReady} />)
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    changeModel()
    await screen.findByRole('heading', { name: 'Parakeet Unified EN 0.6B' })
    expect(screen.getByRole('button', { name: '下载模型' })).toBeEnabled()
    onReady.mockClear()
    progress()
    await act(async () => {
      lookup.resolve(paths(first, '', true))
      metadata.resolve(catalog)
      download.resolve(paths(first, '', true))
    })
    expect(screen.getByRole('heading', { name: 'Parakeet Unified EN 0.6B' })).toBeInTheDocument()
    expect(screen.getByText('尚未安装')).toBeInTheDocument()
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
    expect(onReady).not.toHaveBeenCalled()
  })

  it('does not allow a slow pre-download lookup to overwrite a completed install', async () => {
    const lookup = deferred<NativeAsrPaths>()
    vi.mocked(api.getNativeAsrPaths).mockReturnValueOnce(lookup.promise)
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    await screen.findByText('已安装 · SHA-256 已验证')
    await act(async () => lookup.resolve(paths()))
    expect(screen.getByText('已安装 · SHA-256 已验证')).toBeInTheDocument()
  })

  it('ignores late failures and dialog writes after unmount and unregisters event listeners', async () => {
    const download = deferred<NativeAsrPaths>()
    const selection = deferred<string | null>()
    vi.mocked(api.downloadNativeAsr).mockReturnValueOnce(download.promise)
    vi.mocked(open).mockReturnValueOnce(selection.promise)
    const onReady = vi.fn()
    const { unmount } = render(<NativeAsrPanel onReadyChange={onReady} />)
    await screen.findByText('尚未安装')
    fireEvent.click(screen.getByRole('button', { name: '自定义目录' }))
    fireEvent.click(screen.getByRole('button', { name: '下载模型' }))
    onReady.mockClear()
    unmount()
    await act(async () => {
      download.reject('late failure')
      selection.resolve('E:\\Obsolete')
    })
    expect(onReady).not.toHaveBeenCalled()
    expect(useAppStore.getState().config.native_asr.model_dir).toBe('')
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('unsubscribes even when the event registration resolves after unmount', async () => {
    const subscription = deferred<() => void>()
    vi.mocked(listen).mockReturnValueOnce(subscription.promise)
    const { unmount } = render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    unmount()
    await act(async () => subscription.resolve(unlisten))
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it.each(['paths', 'catalog', 'subscription'])(
    'reports %s Promise failures without an unhandled rejection',
    async (failure) => {
      if (failure === 'paths')
        vi.mocked(api.getNativeAsrPaths).mockRejectedValueOnce('path failure')
      if (failure === 'catalog')
        vi.mocked(api.getNativeAsrCatalog).mockRejectedValueOnce('catalog failure')
      if (failure === 'subscription')
        vi.mocked(listen).mockRejectedValueOnce('subscription failure')
      render(<NativeAsrPanel onReadyChange={vi.fn()} />)
      expect(await screen.findByRole('alert')).toHaveTextContent(
        `${failure === 'paths' ? 'path' : failure} failure`,
      )
      expect(screen.getByRole('button', { name: '重新尝试下载' })).toBeEnabled()
    },
  )

  it('offers update when the fixed catalog revision differs from the installed revision', async () => {
    vi.mocked(api.getNativeAsrPaths).mockResolvedValue({
      ...paths(first, '', true),
      update_available: true,
    })
    render(<NativeAsrPanel onReadyChange={vi.fn()} />)
    expect(await screen.findByRole('button', { name: '更新模型' })).toBeEnabled()
    expect(screen.getByText(/有版本更新/)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '更新模型' }))
    await screen.findByRole('button', { name: '检查并修复模型' })
  })

  it('converts only the empty model root to null for native API calls', async () => {
    const actual = await vi.importActual<typeof import('../../../lib/tauri')>('../../../lib/tauri')
    await actual.getNativeAsrPaths(first, '')
    expect(invoke).toHaveBeenLastCalledWith('get_native_asr_paths', {
      modelId: first,
      customDir: null,
    })
    await actual.downloadNativeAsr(first, 'E:\\语音模型')
    expect(invoke).toHaveBeenLastCalledWith('download_native_asr_model', {
      modelId: first,
      customDir: 'E:\\语音模型',
    })
    await actual.deleteNativeAsr(second, '')
    expect(invoke).toHaveBeenLastCalledWith('delete_native_asr_model', {
      modelId: second,
      customDir: null,
    })
  })
})
