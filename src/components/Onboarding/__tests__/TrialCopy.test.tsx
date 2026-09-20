import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { ModeSelectStep } from '../ModeSelectStep'
import { QuickTestStep } from '../QuickTestStep'
import { getLocalLlmPaths, getSenseVoicePaths, listAudioInputDevices } from '../../../lib/tauri'
import { useAppStore } from '../../../stores/appStore'
import { APP_VERSION } from '../../../lib/constants'
import packageInfo from '../../../../package.json'
import zh from '../../../i18n/locales/zh.json'
import en from '../../../i18n/locales/en.json'

vi.mock('../../../lib/tauri')
beforeEach(() => {
  vi.clearAllMocks()
  useAppStore.setState(useAppStore.getInitialState())
  vi.mocked(listAudioInputDevices).mockResolvedValue(['Default microphone'])
  vi.mocked(getSenseVoicePaths).mockResolvedValue({ ready: true } as any)
  vi.mocked(getLocalLlmPaths).mockResolvedValue({ default_model_ready: true } as any)
})
afterEach(cleanup)

describe('public trial and activation copy', () => {
  it('keeps the public app version aligned with the actual package', () => {
    expect(APP_VERSION).toBe(`v${packageInfo.version}`)
  })
  it('describes onboarding as a bounded trial, not unlimited free offline use', () => {
    const { container } = render(<ModeSelectStep />)
    expect(screen.getByText(/累计 200 次或 20 分钟任一用完后需激活/)).toBeInTheDocument()
    expect(screen.getByText(/热词和纠错需激活/)).toBeInTheDocument()
    expect(container.textContent).not.toMatch(/永久免费|无限免费|不限制免费/)
  })
  it('does not call manual text input a successful real speech-recognition test', async () => {
    render(<QuickTestStep />)
    await waitFor(() => expect(screen.getAllByText('已就绪')).toHaveLength(3))
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '这是手动输入的文字' } })
    expect(screen.getByText('输入框已有文本，请核对识别结果')).toBeInTheDocument()
    expect(screen.queryByText('真实链路测试成功')).not.toBeInTheDocument()
    expect(screen.getByText(/组件就绪不代表已激活/)).toBeInTheDocument()
  })
  it('describes the offline-first product honestly and removes unsupported cloud quota promises', () => {
    for (const locale of [zh, en]) {
      expect(locale.settings.aboutDescription).toMatch(/Windows/)
      expect(locale.settings.aboutDescription).toMatch(/CPU/)
      expect(locale.settings.aboutDescription).not.toMatch(
        /绝对私密|绝对安全|absolute(?:ly)? private|absolute(?:ly)? secure/i,
      )
      const hints = [
        locale.settings.sttSignInHint,
        locale.settings.sttUpgradeHint,
        locale.settings.sttProActive,
        locale.settings.llmSignInHint,
        locale.settings.llmUpgradeHint,
        locale.settings.llmProActive,
      ].join(' ')
      expect(hints).not.toMatch(/10小时|10h\/month|500万|5M tokens|毫秒级|millisecond/)
    }
  })
})
