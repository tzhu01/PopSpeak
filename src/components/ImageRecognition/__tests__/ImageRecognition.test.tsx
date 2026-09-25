import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ImageRecognition } from '../index'
import { open } from '@tauri-apps/plugin-dialog'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { recognizeImage } from '../../../lib/tauri'

type DropEvent = { payload: { type: 'enter' | 'leave' | 'drop'; paths: string[] } }
let onDrop: ((event: DropEvent) => void) | undefined

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn() }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onCloseRequested: vi.fn(() => Promise.resolve(vi.fn())),
    onDragDropEvent: vi.fn((handler: (event: DropEvent) => void) => {
      onDrop = handler
      return Promise.resolve(vi.fn())
    }),
  }),
}))
vi.mock('../../../lib/tauri', () => ({ recognizeImage: vi.fn() }))

beforeEach(() => {
  vi.clearAllMocks()
  onDrop = undefined
  vi.mocked(open).mockResolvedValue(null)
  vi.mocked(writeText).mockResolvedValue(undefined)
})

afterEach(cleanup)

describe('image recognition', () => {
  it('recognizes a selected image, allows editing, and copies the edited text', async () => {
    vi.mocked(open).mockResolvedValue('C:\\pictures\\receipt.png')
    vi.mocked(recognizeImage).mockResolvedValue('原始文字')
    render(<ImageRecognition />)

    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.chooseImage' }))
    await waitFor(() => expect(recognizeImage).toHaveBeenCalledWith('C:\\pictures\\receipt.png'))
    const result = await screen.findByRole('textbox', { name: 'imageRecognition.editResult' })
    await waitFor(() => expect(result).toHaveValue('原始文字'))
    expect(screen.getByText('receipt.png')).toBeInTheDocument()

    fireEvent.change(result, { target: { value: '修改后的文字' } })
    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.copy' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('修改后的文字'))
    expect(screen.getByRole('button', { name: 'imageRecognition.copied' })).toBeInTheDocument()
  })

  it('accepts a dropped TIFF image and rejects unsupported files', async () => {
    vi.mocked(recognizeImage).mockResolvedValue('拖入图片的文字')
    render(<ImageRecognition />)

    await waitFor(() => expect(onDrop).toBeDefined())
    act(() => onDrop?.({ payload: { type: 'drop', paths: ['D:\\scan\\page.tiff'] } }))
    await waitFor(() => expect(recognizeImage).toHaveBeenCalledWith('D:\\scan\\page.tiff'))
    act(() => onDrop?.({ payload: { type: 'drop', paths: ['D:\\scan\\page.gif'] } }))
    expect(screen.getByRole('alert')).toHaveTextContent('imageRecognition.unsupportedFormat')
    expect(recognizeImage).toHaveBeenCalledTimes(1)
  })

  it('keeps the previous result if recognition fails', async () => {
    vi.mocked(open).mockResolvedValue('C:\\pictures\\first.jpg')
    vi.mocked(recognizeImage)
      .mockResolvedValueOnce('第一份文字')
      .mockRejectedValueOnce(new Error('无法读取图片'))
    render(<ImageRecognition />)

    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.chooseImage' }))
    const result = await screen.findByRole('textbox', { name: 'imageRecognition.editResult' })
    await waitFor(() => expect(result).toHaveValue('第一份文字'))
    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.retry' }))
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('无法读取图片'))
    expect(result).toHaveValue('第一份文字')
  })

  it('does not mark newly edited text as copied while an older clipboard write is pending', async () => {
    vi.mocked(open).mockResolvedValue('C:\\pictures\\first.png')
    vi.mocked(recognizeImage).mockResolvedValue('原始文字')
    let finishCopy: (() => void) | undefined
    vi.mocked(writeText).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finishCopy = resolve
        }),
    )
    render(<ImageRecognition />)

    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.chooseImage' }))
    const result = await screen.findByRole('textbox', { name: 'imageRecognition.editResult' })
    await waitFor(() => expect(result).toHaveValue('原始文字'))
    fireEvent.change(result, { target: { value: '第一次修改' } })
    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.copy' }))
    fireEvent.change(result, { target: { value: '第二次修改' } })
    await act(async () => finishCopy?.())

    expect(writeText).toHaveBeenCalledWith('第一次修改')
    expect(screen.getByRole('button', { name: 'imageRecognition.copy' })).toBeInTheDocument()
    expect(result).toHaveValue('第二次修改')
  })

  it('asks before replacing an edited result', async () => {
    vi.mocked(open).mockResolvedValue('C:\\pictures\\first.bmp')
    vi.mocked(recognizeImage).mockResolvedValue('原始文字')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    render(<ImageRecognition />)

    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.chooseImage' }))
    const result = await screen.findByRole('textbox', { name: 'imageRecognition.editResult' })
    await waitFor(() => expect(result).toHaveValue('原始文字'))
    fireEvent.change(result, { target: { value: '我改过的文字' } })
    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.retry' }))

    expect(confirm).toHaveBeenCalled()
    expect(recognizeImage).toHaveBeenCalledTimes(1)
    expect(result).toHaveValue('我改过的文字')
    confirm.mockRestore()
  })

  it('keeps the page open when leaving with uncopied edits is cancelled', async () => {
    window.history.replaceState(null, '', '#/image')
    vi.mocked(open).mockResolvedValue('C:\\pictures\\first.png')
    vi.mocked(recognizeImage).mockResolvedValue('原始文字')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    render(<ImageRecognition />)

    fireEvent.click(screen.getByRole('button', { name: 'imageRecognition.chooseImage' }))
    const result = await screen.findByRole('textbox', { name: 'imageRecognition.editResult' })
    await waitFor(() => expect(result).toHaveValue('原始文字'))
    fireEvent.change(result, { target: { value: '未复制的修改' } })

    act(() => {
      window.history.replaceState(null, '', '#/history')
      window.dispatchEvent(new HashChangeEvent('hashchange'))
    })
    expect(confirm).toHaveBeenCalledWith('imageRecognition.confirmLeave')
    expect(window.location.hash).toBe('#/image')
    expect(result).toHaveValue('未复制的修改')
    confirm.mockRestore()
    window.history.replaceState(null, '', '#/')
  })
})
