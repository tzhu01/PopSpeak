import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '../../stores/appStore'
import { useCapsuleResize } from '../useCapsuleResize'

const native = vi.hoisted(() => ({
  hide: vi.fn(),
  show: vi.fn(),
  setSize: vi.fn(),
  setPosition: vi.fn(),
  setAlwaysOnTop: vi.fn(),
  outerPosition: vi.fn(),
  outerSize: vi.fn(),
  currentMonitor: vi.fn(),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => native,
  currentMonitor: native.currentMonitor,
  PhysicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
  PhysicalPosition: class {
    constructor(
      public x: number,
      public y: number,
    ) {}
  },
}))

beforeEach(() => {
  Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 1 })
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: 1024 })
  Object.defineProperty(window, 'innerHeight', { configurable: true, value: 768 })
  useAppStore.setState(useAppStore.getInitialState())
  for (const mock of Object.values(native)) mock.mockReset().mockResolvedValue(undefined)
  native.outerPosition.mockResolvedValue({ x: -80, y: 960 })
  native.outerSize.mockResolvedValue({ width: 60, height: 60 })
  native.currentMonitor.mockResolvedValue({
    scaleFactor: 1,
    position: { x: -1920, y: 0 },
    size: { width: 1920, height: 1080 },
    workArea: { position: { x: -1920, y: 0 }, size: { width: 1920, height: 1040 } },
  })
})
afterEach(cleanup)

describe('floating recorder native resize', () => {
  it.each([
    {
      menu: true,
      popup: false,
      area: { width: 400, height: 400 },
      css: { width: 288, height: 320 },
    },
    {
      menu: false,
      popup: true,
      area: { width: 300, height: 400 },
      css: { width: 240, height: 272 },
    },
  ])(
    'shows a clamped small-work-area layout at 125% DPI (menu=$menu)',
    async ({ menu, popup, area, css }) => {
      Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 1.25 })
      native.currentMonitor.mockResolvedValue({
        scaleFactor: 1.25,
        workArea: { position: { x: -400, y: -400 }, size: area },
      })
      useAppStore.setState({ contextMenuOpen: menu })
      const hook = renderHook(() => useCapsuleResize(popup))
      await waitFor(() => expect(native.show).toHaveBeenCalledOnce())
      expect(hook.result.current).toBe(false)
      expect(native.setSize).toHaveBeenCalledWith(
        expect.objectContaining({
          width: css.width * 1.25,
          height: css.height * 1.25,
        }),
      )
      act(() => {
        Object.defineProperty(window, 'innerWidth', { configurable: true, value: css.width })
        Object.defineProperty(window, 'innerHeight', { configurable: true, value: css.height })
        window.dispatchEvent(new Event('resize'))
      })
      expect(hook.result.current).toBe(true)
    },
  )

  it('starts at the work-area bottom-right on a negative-origin monitor', async () => {
    renderHook(() => useCapsuleResize(false))
    await waitFor(() => expect(native.show).toHaveBeenCalledOnce())
    expect(native.setSize).toHaveBeenLastCalledWith(
      expect.objectContaining({ width: 62, height: 62 }),
    )
    expect(native.setPosition).toHaveBeenLastCalledWith(expect.objectContaining({ x: -82, y: 958 }))
    expect(native.outerPosition).not.toHaveBeenCalled()
  })

  it('converts POP logical dimensions and margin to physical pixels at 150% DPI', async () => {
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 1.5 })
    native.currentMonitor.mockResolvedValue({
      scaleFactor: 1.5,
      workArea: { position: { x: 0, y: 0 }, size: { width: 2880, height: 1560 } },
    })
    renderHook(() => useCapsuleResize(true))
    await waitFor(() => expect(native.show).toHaveBeenCalledOnce())
    expect(native.setSize).toHaveBeenCalledWith(
      expect.objectContaining({ width: 516, height: 408 }),
    )
    expect(native.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 2334, y: 1122 }))
  })

  it('serializes a close behind an in-flight resize so a stale show cannot reopen it', async () => {
    let finishResize!: () => void
    native.setSize.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finishResize = resolve
        }),
    )
    renderHook(() => useCapsuleResize(true))
    await waitFor(() => expect(native.setSize).toHaveBeenCalledOnce())
    act(() => useAppStore.getState().updateConfig({ capsule_enabled: false }))
    await act(async () => finishResize())
    await waitFor(() => expect(native.hide).toHaveBeenCalled())
    expect(native.show).not.toHaveBeenCalled()
  })

  it('does not show a disabled capsule even while recording', async () => {
    useAppStore.setState((state) => ({
      pipelineState: 'recording',
      config: { ...state.config, capsule_enabled: false },
    }))
    renderHook(() => useCapsuleResize(true))
    await waitFor(() => expect(native.hide).toHaveBeenCalledOnce())
    expect(native.show).not.toHaveBeenCalled()
    expect(native.setSize).not.toHaveBeenCalled()
  })

  it('waits for the WebView viewport before painting an expanded POP', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 62 })
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 62 })
    const hook = renderHook(() => useCapsuleResize(true))
    await waitFor(() => expect(native.show).toHaveBeenCalledOnce())
    expect(hook.result.current).toBe(false)
    act(() => {
      Object.defineProperty(window, 'innerWidth', { configurable: true, value: 344 })
      Object.defineProperty(window, 'innerHeight', { configurable: true, value: 272 })
      window.dispatchEvent(new Event('resize'))
    })
    expect(hook.result.current).toBe(true)
  })

  it('also waits for a shrinking viewport so the idle ball cannot use stale POP coordinates', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 344 })
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 272 })
    const hook = renderHook(({ popup }) => useCapsuleResize(popup), {
      initialProps: { popup: true },
    })
    await waitFor(() => expect(hook.result.current).toBe(true))
    hook.rerender({ popup: false })
    await waitFor(() =>
      expect(native.setSize).toHaveBeenLastCalledWith(
        expect.objectContaining({ width: 62, height: 62 }),
      ),
    )
    expect(hook.result.current).toBe(false)
    act(() => {
      Object.defineProperty(window, 'innerWidth', { configurable: true, value: 62 })
      Object.defineProperty(window, 'innerHeight', { configurable: true, value: 62 })
      window.dispatchEvent(new Event('resize'))
    })
    expect(hook.result.current).toBe(true)
  })

  it('resizes again when WebView DPI changes instead of trusting an old monitor scale', async () => {
    renderHook(() => useCapsuleResize(true))
    await waitFor(() => expect(native.show).toHaveBeenCalledOnce())
    act(() => {
      Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 2 })
      window.dispatchEvent(new Event('resize'))
    })
    await waitFor(() =>
      expect(native.setSize).toHaveBeenLastCalledWith(
        expect.objectContaining({ width: 688, height: 544 }),
      ),
    )
  })
})
