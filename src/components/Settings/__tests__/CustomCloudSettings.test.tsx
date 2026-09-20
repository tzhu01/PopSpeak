import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react'
import { CustomCloudSettings } from '../CustomCloudSettings'
import * as tauri from '../../../lib/tauri'

// UI contract tests only: every network command is mocked. Live account validation
// is a separate release check and must never be implied by these tests passing.
vi.mock('../../../lib/tauri')
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))

const blank = () => ({
  vendor: 'whisper',
  app_id: '',
  api_key: '',
  api_secret: '',
  access_token: '',
  endpoint: '',
  model: '',
  region: '',
})
const mockStore = {
  config: { custom_cloud: blank() },
  updateConfig: vi.fn(),
}
vi.mock('../../../stores/appStore', () => ({
  useAppStore: (selector: any) => selector(mockStore),
}))

describe('CustomCloudSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.config = { custom_cloud: blank() }
  })
  afterEach(cleanup)

  it('offers five vendor-specific protocols plus the generic transcription adapter', () => {
    render(<CustomCloudSettings />)
    const select = screen.getByRole('combobox', { name: '服务商与接口类型' }) as HTMLSelectElement
    expect([...select.options].map((option) => option.value)).toEqual([
      'bytedance',
      'aliyun',
      'tencent',
      'iflytek',
      'baidu',
      'whisper',
    ])
    expect(screen.getByText(/测试成功不代表语音准确率测试通过/)).toBeInTheDocument()
  })

  it.each([
    ['bytedance', ['APP ID', 'Access Token', '语音资源 ID']],
    ['aliyun', ['项目 AppKey', '语音服务 Token', '服务地域']],
    ['tencent', ['APP ID', 'SecretId', 'SecretKey', '识别引擎型号']],
    ['iflytek', ['APPID', 'APIKey', 'APISecret']],
    ['baidu', ['API Key', 'Secret Key', '识别模型编号（dev_pid）']],
    ['whisper', ['音频转写地址', 'API Key', '模型 ID']],
  ])('shows only the required credential shape for %s', (vendor, labels) => {
    mockStore.config.custom_cloud.vendor = vendor as string
    render(<CustomCloudSettings />)
    for (const label of labels) expect(screen.getByLabelText(label)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '测试云端接口' })).toBeDisabled()
    expect(screen.getByText(/明文写入本机 settings.json/)).toBeInTheDocument()
  })

  it('clears credentials and endpoint when switching vendors', () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      api_key: 'private-key',
      api_secret: 'private-secret',
      access_token: 'private-token',
      endpoint: 'https://old.example/v1',
      model: 'old-model',
    }
    render(<CustomCloudSettings />)
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'bytedance' } })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      custom_cloud: { ...blank(), vendor: 'bytedance', model: 'volc.seedasr.sauc.duration' },
    })
    expect(tauri.benchSttConnection).not.toHaveBeenCalled()
  })

  it('masks secrets and updates only the custom cloud object', () => {
    mockStore.config.custom_cloud.vendor = 'iflytek'
    render(<CustomCloudSettings />)
    const secret = screen.getByLabelText('APISecret') as HTMLInputElement
    expect(secret.type).toBe('password')
    fireEvent.change(secret, { target: { value: 'new-secret' } })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      custom_cloud: { ...blank(), vendor: 'iflytek', api_secret: 'new-secret' },
    })
  })

  it('passes custom credentials separately from the legacy provider credentials', async () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      vendor: 'tencent',
      app_id: '1234',
      api_key: 'secret-id',
      api_secret: 'secret-key',
      model: '16k_zh',
    }
    vi.mocked(tauri.benchSttConnection).mockResolvedValue(321)
    render(<CustomCloudSettings />)
    fireEvent.click(screen.getByRole('button', { name: '测试云端接口' }))
    await waitFor(() =>
      expect(tauri.benchSttConnection).toHaveBeenCalledWith(
        '',
        'custom-whisper',
        '',
        '',
        '',
        '',
        mockStore.config.custom_cloud,
      ),
    )
    expect(await screen.findByRole('status')).toHaveTextContent('接口验证成功 · 321 ms')
  })

  it('does not present server rejection as success or echo credentials in errors', async () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      vendor: 'aliyun',
      app_id: 'app-key',
      access_token: 'private-access-token',
    }
    vi.mocked(tauri.benchSttConnection).mockRejectedValue(
      new Error('Token private-access-token 已过期'),
    )
    render(<CustomCloudSettings />)
    fireEvent.click(screen.getByRole('button', { name: '测试云端接口' }))
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('已过期')
    expect(alert).not.toHaveTextContent('private-access-token')
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })

  it('requires a transcription URL and model for the generic adapter', () => {
    mockStore.config.custom_cloud.api_key = 'api-key'
    const { rerender } = render(<CustomCloudSettings />)
    expect(screen.getByRole('button', { name: '测试云端接口' })).toBeDisabled()
    mockStore.config.custom_cloud.endpoint = 'https://example.com/v1/audio/transcriptions'
    mockStore.config.custom_cloud.model = 'transcribe-model'
    rerender(<CustomCloudSettings />)
    expect(screen.getByRole('button', { name: '测试云端接口' })).toBeEnabled()
  })

  it('supports an already saved ByteDance API Key without requiring an APP ID', async () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      vendor: 'bytedance',
      api_key: 'voice-api-key',
      model: 'volc.seedasr.sauc.duration',
    }
    vi.mocked(tauri.benchSttConnection).mockResolvedValue(123)
    render(<CustomCloudSettings />)
    expect(screen.getByRole('combobox', { name: '认证方式' })).toHaveValue('alternative')
    expect(screen.queryByLabelText('APP ID')).not.toBeInTheDocument()
    expect(screen.getByLabelText('语音 API Key')).toHaveValue('voice-api-key')
    fireEvent.click(screen.getByRole('button', { name: '测试云端接口' }))
    expect(await screen.findByRole('status')).toHaveTextContent('接口验证成功')
  })

  it('supports Baidu Access Token authentication and explicitly describes its non-streaming limit', () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      vendor: 'baidu',
      access_token: 'saved-access-token',
      model: '1537',
    }
    render(<CustomCloudSettings />)
    expect(screen.getByLabelText('Access Token')).toHaveValue('saved-access-token')
    expect(screen.queryByLabelText('Secret Key')).not.toBeInTheDocument()
    expect(screen.getByText(/单段最多 60 秒；不是实时字幕接口/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '测试云端接口' })).toBeEnabled()
  })

  it('clears the previous credentials when the authentication method changes', () => {
    mockStore.config.custom_cloud = {
      ...blank(),
      vendor: 'bytedance',
      app_id: 'old-app',
      access_token: 'old-token',
      model: 'volc.seedasr.sauc.duration',
    }
    render(<CustomCloudSettings />)
    fireEvent.change(screen.getByRole('combobox', { name: '认证方式' }), {
      target: { value: 'alternative' },
    })
    expect(mockStore.updateConfig).toHaveBeenCalledWith({
      custom_cloud: { ...blank(), vendor: 'bytedance', model: 'volc.seedasr.sauc.duration' },
    })
    expect(screen.getByLabelText('语音 API Key')).toBeInTheDocument()
  })
})
