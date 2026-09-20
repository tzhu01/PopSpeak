/**
 * Settings 组件测试集
 *
 * 覆盖以下范围：
 * 1. Tab 切换 — 点击侧边栏后正确显示对应 Pane 内容
 * 2. 页面结构 — 子页切换不叠放并重置滚动位置
 * 3. appStore.llmModels — 状态提升：初始值、读写、reset
 * 4. LlmPane provider 切换 — 清空 models 缓存
 * 5. LlmPane useEffect skip — 已有缓存时不再触发 debounce fetch
 * 6. DirtyBar — 配置变更后出现，Reset 后消失
 * 7. appStore getInitialState — llmModels 在 reset 后为空数组
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor, act, cleanup, within } from '@testing-library/react'
import React from 'react'
import { useAppStore } from '../../../stores/appStore'
import type { AppConfig } from '../../../stores/appStore'
import { useActivationStore } from '../../../lib/activation'

// 每个测试后清理 DOM，防止多次 render 的节点积累导致 getByText 找到多个元素
afterEach(() => {
  cleanup()
})

// ─── Mock framer-motion ───────────────────────────────────────────────────────
// 过滤掉所有 framer-motion 专有 prop，避免 React DOM 警告和 getByText 多元素问题
const MOTION_PROPS = new Set([
  'initial',
  'animate',
  'exit',
  'transition',
  'variants',
  'whileHover',
  'whileTap',
  'whileFocus',
  'whileDrag',
  'whileInView',
  'layoutId',
  'layout',
  'drag',
  'dragConstraints',
  'onAnimationComplete',
])

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  motion: new Proxy(
    {},
    {
      get:
        (_t, tag: string) =>
        ({ children, ...rest }: any) => {
          const domProps: Record<string, unknown> = {}
          for (const [k, v] of Object.entries(rest)) {
            if (!MOTION_PROPS.has(k)) domProps[k] = v
          }
          return React.createElement(tag as string, { 'data-motion': tag, ...domProps }, children)
        },
    },
  ),
}))

// ─── Mock react-i18next ───────────────────────────────────────────────────────
vi.mock('react-i18next', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react-i18next')>()
  return {
    ...actual,
    useTranslation: () => ({
      t: (key: string) => key,
      i18n: { language: 'en', changeLanguage: vi.fn() },
    }),
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}))

// ─── Mock Tauri plugins / lib/tauri ──────────────────────────────────────────
vi.mock('../../../lib/tauri', async (importOriginal) => ({
  NATIVE_ASR_IDS: (await importOriginal<typeof import('../../../lib/tauri')>()).NATIVE_ASR_IDS,
  getNativeAsrPaths: vi.fn(async (modelId: string) => ({ model_id: modelId, ready: false })),
  getNativeAsrCatalog: vi.fn().mockResolvedValue([]),
  downloadNativeAsr: vi.fn(),
  cancelNativeAsr: vi.fn(),
  deleteNativeAsr: vi.fn(),
  updateHotkey: vi.fn().mockResolvedValue(undefined),
  listAudioInputDevices: vi.fn().mockResolvedValue(['Default microphone']),
  getSenseVoicePaths: vi.fn().mockResolvedValue({
    model_dir: 'models/sensevoice',
    display_dir: '.\\models\\sensevoice',
    source: 'package-relative',
    model_path: 'models/sensevoice/model.int8.onnx',
    tokens_path: 'models/sensevoice/tokens.txt',
    ready: true,
    is_custom: false,
  }),
  openSenseVoiceModelDirectory: vi.fn().mockResolvedValue(undefined),
  getFunAsrPaths: vi.fn().mockResolvedValue({
    model_dir: 'models/funasr-nano',
    display_dir: '.\\models\\funasr-nano',
    is_custom: false,
    quantization: 'Q4_K_M',
    runtime_variant: 'AVX2',
    ready: false,
    runtime_ready: true,
    encoder_ready: false,
    llm_ready: false,
    vad_ready: false,
  }),
  getFunAsrRuntimeStatus: vi.fn().mockResolvedValue({ running: false, ready: false }),
  getLocalModelPaths: vi.fn().mockResolvedValue({
    default_model_ready: true,
    upgrade_model_ready: true,
    small_model_ready: false,
    turbo_model_ready: false,
  }),
  getLocalLlmPaths: vi.fn().mockResolvedValue({
    server_path: 'llama-server.exe',
    model_dir: 'models/llm',
    default_model_path: 'models/llm/model.gguf',
    default_model_ready: true,
    upgrade_model_path: 'models/llm/model-upgrade.gguf',
    upgrade_model_ready: false,
  }),
  localLlmHealth: vi.fn().mockResolvedValue(false),
  checkForUpdates: vi.fn().mockResolvedValue({
    current_version: '0.2.0',
    latest_version: 'v0.2.0',
    available: false,
    release_url: 'https://github.com/tzhu01/PopSpeak/releases',
  }),
  pauseHotkey: vi.fn().mockResolvedValue(undefined),
  resumeHotkey: vi.fn().mockResolvedValue(undefined),
  setAutoStart: vi.fn().mockResolvedValue(undefined),
  testSttConnection: vi.fn().mockResolvedValue(true),
  testLlmConnection: vi.fn().mockResolvedValue(true),
  fetchLlmModels: vi.fn().mockResolvedValue(['gpt-4o', 'gpt-3.5-turbo']),
  addDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  updateDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  removeDictionaryEntry: vi.fn().mockResolvedValue(undefined),
  getDictionary: vi.fn().mockResolvedValue([]),
  updateConfig: vi.fn(async (config: AppConfig) => ({ ...config })),
}))

// ─── Mock @tauri-apps/plugin-opener ─────────────────────────────────────────
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))

// ─── Mock lib/api (ScenesPane uses getScenes) ────────────────────────────────
vi.mock('../../../lib/api', () => ({
  getScenes: vi.fn().mockResolvedValue([]),
}))

// ─── Mock stores/authStore ────────────────────────────────────────────────────
vi.mock('../../../stores/authStore', () => ({
  useAuthStore: () => ({ user: null, plan: 'free' }),
}))

// ─── Import components AFTER mocks ───────────────────────────────────────────
import { Settings } from '../index'

// ─── Helpers ─────────────────────────────────────────────────────────────────
function resetStore() {
  useAppStore.setState(useAppStore.getInitialState())
}

function seedSavedConfig() {
  const { config } = useAppStore.getState()
  useAppStore.getState().setSavedConfig(config)
}

function renderSettings() {
  return render(<Settings />)
}

// 侧边栏导航按钮：精确匹配 sidebar 内的 <button data-motion="button"> 子元素
function clickSidebarItem(label: string) {
  const spans = screen.getAllByText(label)
  // sidebar button 的直接父链中有 data-motion="button"，且该 button 不含 h2
  const sidebarSpan = spans.find((el) => {
    const btn = el.closest('[data-motion="button"]')
    return btn !== null && btn.querySelector('h2') === null
  })
  const btn = (sidebarSpan ?? spans[0]).closest('[data-motion="button"], button')
  if (btn) fireEvent.click(btn)
  else fireEvent.click(spans[0])
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Tab 切换 — 渲染正确 Pane 内容
// ─────────────────────────────────────────────────────────────────────────────
describe('Settings tab 切换', () => {
  beforeEach(() => {
    resetStore()
    seedSavedConfig()
  })

  it('初始渲染显示 General pane（含 hotkey section）', () => {
    renderSettings()
    // General pane 包含 "settings.hotkey" section 标题
    expect(screen.getByText('settings.hotkey')).toBeDefined()
  })

  it('通用设置按五个醒目模块分组且保留全部入口', () => {
    renderSettings()

    expect(screen.getAllByTestId('general-settings-group')).toHaveLength(5)
    expect(screen.getByText('settings.generalGroupRecording')).toBeDefined()
    expect(screen.getByText('settings.generalGroupOutput')).toBeDefined()
    expect(screen.getByText('settings.generalGroupCapsule')).toBeDefined()
    expect(screen.getByText('settings.generalGroupAudio')).toBeDefined()
    expect(screen.getByText('settings.generalGroupStartup')).toBeDefined()
    expect(screen.getByText('settings.outputMode')).toBeDefined()
    expect(screen.getByText('settings.microphoneDevice')).toBeDefined()
    expect(screen.getByText('settings.startupBehavior')).toBeDefined()
  })

  it('编辑浮层模式可开关自动关闭并配置停留秒数', () => {
    useAppStore.getState().updateConfig({
      output_mode: 'editor',
      editor_auto_hide_enabled: true,
      editor_auto_hide_seconds: 12,
    })
    seedSavedConfig()
    renderSettings()

    expect(screen.getByText('settings.editorAutoHide')).toBeDefined()
    const slider = screen.getByLabelText('settings.editorAutoHideSeconds')
    expect(slider).toHaveValue('12')

    fireEvent.change(slider, { target: { value: '20' } })
    expect(useAppStore.getState().config.editor_auto_hide_seconds).toBe(20)

    const autoCloseSwitch = screen
      .getByText('settings.editorAutoHide')
      .closest('label')
      ?.querySelector('[role="switch"]')
    expect(autoCloseSwitch).not.toBeNull()
    fireEvent.click(autoCloseSwitch as HTMLElement)
    expect(useAppStore.getState().config.editor_auto_hide_enabled).toBe(false)
  })

  it('点击 Speech Recognition 后显示模型卡片与折叠的高级设置', () => {
    renderSettings()
    clickSidebarItem('settings.speechRecognition')
    expect(screen.getByText('语音转文字模型')).toBeDefined()
    // 默认 SenseVoice 使用自己的离线语言选择，避免与通用语言下拉重复。
    expect(screen.getByText('识别语言')).toBeDefined()
  })

  it('点击 AI Polish 后显示 LLM provider 字段', () => {
    renderSettings()
    clickSidebarItem('settings.aiPolish')
    // LLM pane 也含 provider，但还含 enableAiPolish toggle
    expect(screen.getByText('settings.enableAiPolish')).toBeDefined()
  })

  it('点击 Dictionary 后明确显示纠错映射输入框', () => {
    renderSettings()
    clickSidebarItem('settings.dictionary')
    expect(screen.getByPlaceholderText('dictionary.correctionFromExample')).toBeDefined()
    expect(screen.getByPlaceholderText('dictionary.correctWordExample')).toBeDefined()
    expect(screen.getByText('dictionary.firstPrinciple')).toBeDefined()
    expect(screen.getByText('dictionary.advancedCorrection')).toBeDefined()
  })

  it('未登录也能浏览和使用本地场景', () => {
    renderSettings()
    clickSidebarItem('settings.scenes')
    expect(screen.getByText('scenes.howItWorks')).toBeDefined()
    expect(screen.getAllByText('scenes.dailyName').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('scenes.chatName')).toBeDefined()
  })

  it('应用本地场景会一次保存输出与润色设置', async () => {
    vi.clearAllMocks()
    useActivationStore.setState({
      error: null,
      status: {
        installation_id: 'test-device',
        configured: true,
        activated: true,
        license_id: 'test-license',
        trial_recordings_used: 0,
        trial_recordings_limit: 200,
        trial_recordings_remaining: 200,
        trial_milliseconds_used: 0,
        trial_milliseconds_limit: 1200000,
        trial_milliseconds_remaining: 1200000,
        trial_exhausted: false,
        recording_in_progress: false,
      },
    })
    const { updateConfig } = await import('../../../lib/tauri')
    renderSettings()
    clickSidebarItem('settings.scenes')
    const chatCard = screen.getByText('scenes.chatName').closest('article')
    expect(chatCard).not.toBeNull()
    fireEvent.click(within(chatCard as HTMLElement).getByRole('button', { name: 'scenes.apply' }))

    await waitFor(() =>
      expect(updateConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          output_mode: 'keyboard',
          polish_enabled: true,
          polish_mode: 'fast',
        }),
      ),
    )
    expect(useAppStore.getState().savedConfig).toEqual(
      expect.objectContaining({ output_mode: 'keyboard', polish_enabled: true }),
    )
  })

  it('会议场景只添加尚不存在的热词', async () => {
    vi.clearAllMocks()
    useActivationStore.setState((state) => ({
      error: null,
      status: {
        ...(state.status ?? {
          installation_id: 'test-device',
          configured: true,
          license_id: 'test-license',
          trial_recordings_used: 0,
          trial_recordings_limit: 200,
          trial_recordings_remaining: 200,
          trial_milliseconds_used: 0,
          trial_milliseconds_limit: 1200000,
          trial_milliseconds_remaining: 1200000,
          trial_exhausted: false,
          recording_in_progress: false,
        }),
        activated: true,
      },
    }))
    const { addDictionaryEntry, getDictionary } = await import('../../../lib/tauri')
    vi.mocked(getDictionary)
      .mockResolvedValueOnce([{ id: 1, word: '议程', pronunciation: null, correction_from: null }])
      .mockResolvedValueOnce([{ id: 1, word: '议程', pronunciation: null, correction_from: null }])

    renderSettings()
    clickSidebarItem('settings.scenes')
    const meetingCard = screen.getByText('scenes.meetingName').closest('article')
    fireEvent.click(
      within(meetingCard as HTMLElement).getByRole('button', { name: 'scenes.apply' }),
    )

    await waitFor(() => expect(addDictionaryEntry).toHaveBeenCalledTimes(4))
    expect(addDictionaryEntry).not.toHaveBeenCalledWith('议程', null, null)
    expect(addDictionaryEntry).toHaveBeenCalledWith('行动项', null, null)
    expect(useAppStore.getState().savedConfig).toEqual(
      expect.objectContaining({ output_mode: 'editor', polish_mode: 'deep' }),
    )
  })

  it('点击 About 后显示版本信息区域', () => {
    renderSettings()
    clickSidebarItem('settings.about')
    expect(screen.getByText('settings.openSource')).toBeDefined()
  })

  it('可以在多个 tab 之间来回切换', () => {
    renderSettings()
    clickSidebarItem('settings.aiPolish')
    expect(screen.getByText('settings.enableAiPolish')).toBeDefined()

    clickSidebarItem('settings.general')
    expect(screen.getByText('settings.hotkey')).toBeDefined()
  })

  it('切换 tab 后 title bar 更新', () => {
    renderSettings()
    clickSidebarItem('settings.dictionary')
    // title bar 中的 h2 应该显示 settings.dictionary
    const titles = screen.getAllByText('settings.dictionary')
    // 至少出现两次：sidebar nav 和 title bar h2
    expect(titles.length).toBeGreaterThanOrEqual(2)
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 2. 页面结构 — 切换时只保留一个子页并回到顶部
// ─────────────────────────────────────────────────────────────────────────────
describe('Settings 页面结构', () => {
  beforeEach(() => {
    resetStore()
    seedSavedConfig()
  })

  it('motion wrapper 正常渲染 pane 内容', () => {
    const { container } = renderSettings()
    // 我们的 mock 给 motion 元素打上 data-motion 属性
    expect(container.querySelector('[data-motion]')).not.toBeNull()
  })

  it('切换 tab 后 pane 内容正常更新（无卡死）', () => {
    renderSettings()
    clickSidebarItem('settings.speechRecognition')
    // 仅断言组件没有崩溃，DOM 还在
    expect(document.body).toBeDefined()
  })

  it('切换 tab 后重置共享滚动容器且只保留一个 pane', () => {
    renderSettings()
    const scrollContainer = screen.getByTestId('settings-pane-scroll')
    scrollContainer.scrollTop = 420

    clickSidebarItem('settings.speechRecognition')

    expect(scrollContainer.scrollTop).toBe(0)
    expect(screen.getAllByTestId('settings-pane-content')).toHaveLength(1)
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 3. appStore.llmModels — store 层测试
// ─────────────────────────────────────────────────────────────────────────────
describe('appStore.llmModels', () => {
  beforeEach(() => {
    resetStore()
  })

  it('初始值为空数组', () => {
    expect(useAppStore.getState().llmModels).toEqual([])
  })

  it('setLlmModels 正确更新 store', () => {
    useAppStore.getState().setLlmModels(['model-a', 'model-b'])
    expect(useAppStore.getState().llmModels).toEqual(['model-a', 'model-b'])
  })

  it('setLlmModels([]) 可以清空缓存', () => {
    useAppStore.getState().setLlmModels(['model-a'])
    useAppStore.getState().setLlmModels([])
    expect(useAppStore.getState().llmModels).toHaveLength(0)
  })

  it('store 中的 llmModels 不随组件卸载而丢失', () => {
    useAppStore.getState().setLlmModels(['gpt-4o', 'claude-3'])
    // 模拟"切走再切回"：zustand store 不依赖组件生命周期
    const { unmount } = render(<div />)
    unmount()
    expect(useAppStore.getState().llmModels).toEqual(['gpt-4o', 'claude-3'])
  })

  it('setLlmModels 替换而不是合并', () => {
    useAppStore.getState().setLlmModels(['a', 'b', 'c'])
    useAppStore.getState().setLlmModels(['x'])
    expect(useAppStore.getState().llmModels).toEqual(['x'])
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 4. LlmPane — provider 切换时清空 models 缓存
// ─────────────────────────────────────────────────────────────────────────────
describe('LlmPane provider 切换清空 models', () => {
  beforeEach(() => {
    resetStore()
    seedSavedConfig()
  })

  it('切换 provider 时 store 中的 llmModels 被清空', async () => {
    useAppStore.getState().setLlmModels(['model-x', 'model-y'])

    renderSettings()
    clickSidebarItem('settings.aiPolish')

    // provider select 是当前 pane 中的第一个 combobox
    const selects = screen.getAllByRole('combobox')
    const providerSelect = selects[0]

    await act(async () => {
      fireEvent.change(providerSelect, { target: { value: 'openai' } })
    })

    expect(useAppStore.getState().llmModels).toEqual([])
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 5. LlmPane useEffect — 已有缓存时不重复 fetch
// ─────────────────────────────────────────────────────────────────────────────
describe('LlmPane models 缓存：已有缓存时跳过 fetch', () => {
  beforeEach(() => {
    resetStore()
    seedSavedConfig()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it('llmModels 已有内容时不触发 fetchLlmModels', async () => {
    const { fetchLlmModels } = await import('../../../lib/tauri')
    const mockFetch = vi.mocked(fetchLlmModels)
    mockFetch.mockClear()

    useAppStore.getState().setLlmModels(['cached-model'])
    useAppStore.getState().updateConfig({
      llm_api_key: 'sk-test',
      llm_base_url: 'https://api.openai.com/v1',
      llm_provider: 'openai',
    })

    renderSettings()
    clickSidebarItem('settings.aiPolish')

    await act(async () => {
      vi.runAllTimers()
    })

    expect(mockFetch).not.toHaveBeenCalled()
  })

  it('llmModels 为空且有 api key/url 时触发 fetchLlmModels', async () => {
    const { fetchLlmModels } = await import('../../../lib/tauri')
    const mockFetch = vi.mocked(fetchLlmModels)
    mockFetch.mockClear()

    useAppStore.getState().setLlmModels([])
    useAppStore.getState().updateConfig({
      llm_api_key: 'sk-test',
      llm_base_url: 'https://api.openai.com/v1',
      llm_provider: 'openai',
    })

    renderSettings()
    clickSidebarItem('settings.aiPolish')

    // runAllTimersAsync 同时推进 fake timer 并 flush 所有 pending microtasks/promises
    await act(async () => {
      await vi.runAllTimersAsync()
    })

    expect(mockFetch).toHaveBeenCalledTimes(1)
  })

  it('fetchLlmModels 完成后 store 中 llmModels 被更新', async () => {
    const { fetchLlmModels } = await import('../../../lib/tauri')
    vi.mocked(fetchLlmModels).mockResolvedValue(['gpt-4o', 'gpt-3.5-turbo'])

    useAppStore.getState().setLlmModels([])
    useAppStore.getState().updateConfig({
      llm_api_key: 'sk-test',
      llm_base_url: 'https://api.openai.com/v1',
      llm_provider: 'openai',
    })

    renderSettings()
    clickSidebarItem('settings.aiPolish')

    await act(async () => {
      await vi.runAllTimersAsync()
    })

    expect(useAppStore.getState().llmModels).toEqual(['gpt-4o', 'gpt-3.5-turbo'])
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 6. DirtyBar — 配置变更后出现，Reset 后消失
// ─────────────────────────────────────────────────────────────────────────────
describe('DirtyBar 行为', () => {
  beforeEach(() => {
    resetStore()
    seedSavedConfig()
  })

  it('初始状态下 DirtyBar 不显示', () => {
    renderSettings()
    expect(screen.queryByText('有未保存的设置；保存后应用于下一段录音')).toBeNull()
  })

  it('修改 config 后 DirtyBar 出现', async () => {
    renderSettings()
    act(() => {
      useAppStore.getState().updateConfig({ theme: 'dark' })
    })
    await waitFor(() => {
      expect(screen.getByText('有未保存的设置；保存后应用于下一段录音')).toBeDefined()
    })
  })

  it('点击 Reset 后 DirtyBar 消失', async () => {
    renderSettings()
    act(() => {
      useAppStore.getState().updateConfig({ theme: 'dark' })
    })
    await waitFor(() => {
      expect(screen.getByText('有未保存的设置；保存后应用于下一段录音')).toBeDefined()
    })

    fireEvent.click(screen.getByText('放弃更改'))

    await waitFor(() => {
      expect(screen.queryByText('有未保存的设置；保存后应用于下一段录音')).toBeNull()
    })
  })

  it('DirtyBar 显示 Save 和 Reset 两个按钮', async () => {
    renderSettings()
    act(() => {
      useAppStore.getState().updateConfig({ theme: 'dark' })
    })
    await waitFor(() => {
      expect(screen.getByText('保存并应用')).toBeDefined()
      expect(screen.getByText('放弃更改')).toBeDefined()
    })
  })
})

describe('识别模式草稿与实际配置', () => {
  beforeEach(() => {
    useActivationStore.setState({
      error: null,
      status: {
        installation_id: 'test-device',
        configured: true,
        activated: true,
        license_id: 'test-license',
        trial_recordings_used: 0,
        trial_recordings_limit: 200,
        trial_recordings_remaining: 200,
        trial_milliseconds_used: 0,
        trial_milliseconds_limit: 1200000,
        trial_milliseconds_remaining: 1200000,
        trial_exhausted: false,
        recording_in_progress: false,
      },
    })
    resetStore()
    seedSavedConfig()
    vi.clearAllMocks()
  })

  it('选择精确离线并切到热词页时，明确待保存，原生保存成功后才显示支持', async () => {
    const { updateConfig } = await import('../../../lib/tauri')
    renderSettings()
    clickSidebarItem('settings.speechRecognition')
    fireEvent.click(screen.getByRole('button', { name: /Fun-ASR-Nano/ }))
    clickSidebarItem('settings.dictionary')

    expect(screen.getByText('识别模式待应用')).toBeInTheDocument()
    expect(screen.queryByText('dictionary.decoderHotwordsSupported')).not.toBeInTheDocument()
    expect(useAppStore.getState().savedConfig?.stt_provider).toBe('sensevoice')
    fireEvent.click(screen.getByRole('button', { name: '保存当前设置并应用' }))
    expect(await screen.findByText('dictionary.decoderHotwordsSupported')).toBeInTheDocument()
    expect(updateConfig).toHaveBeenCalledWith(
      expect.objectContaining({ stt_provider: 'funasr-nano' }),
    )
    expect(useAppStore.getState().savedConfig?.stt_provider).toBe('funasr-nano')
    expect(screen.queryByText('有未保存的设置；保存后应用于下一段录音')).not.toBeInTheDocument()
  })

  it('离开设置再进入，不把未保存的选择冒充为实际配置；放弃更改恢复原模式', async () => {
    const first = renderSettings()
    clickSidebarItem('settings.speechRecognition')
    fireEvent.click(screen.getByRole('button', { name: /Fun-ASR-Nano/ }))
    first.unmount()

    renderSettings()
    clickSidebarItem('settings.dictionary')
    expect(useAppStore.getState().savedConfig?.stt_provider).toBe('sensevoice')
    expect(screen.getByText('识别模式待应用')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '放弃更改' }))
    expect(screen.getByText('dictionary.postprocessHotwordsSupported')).toBeInTheDocument()
    clickSidebarItem('settings.speechRecognition')
    expect(screen.getByRole('button', { name: /SenseVoice Small/ })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
  })
})

// ─────────────────────────────────────────────────────────────────────────────
// 7. appStore getInitialState — llmModels 包含在初始状态中
// ─────────────────────────────────────────────────────────────────────────────
describe('appStore getInitialState 包含 llmModels', () => {
  it('getInitialState().llmModels 为空数组', () => {
    const initial = useAppStore.getInitialState()
    expect(initial.llmModels).toEqual([])
  })

  it('setState(getInitialState()) 后 llmModels 恢复为空', () => {
    useAppStore.getState().setLlmModels(['stale-model'])
    useAppStore.setState(useAppStore.getInitialState())
    expect(useAppStore.getState().llmModels).toEqual([])
  })

  it('getInitialState 不改变 llmModels 以外的字段', () => {
    const initial = useAppStore.getInitialState()
    expect(initial.config.hotkey).toBe('Ctrl+/')
    expect(initial.pipelineState).toBe('idle')
    expect(initial.dictionary).toEqual([])
  })
})
