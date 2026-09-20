import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '../../../stores/appStore'
import { Capsule } from '..'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('../../../hooks/useCapsuleResize', () => ({ useCapsuleResize: vi.fn() }))

describe('Capsule recording cancellation', () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState())
    useAppStore.setState({ pipelineState: 'recording' })
    vi.mocked(invoke).mockClear()
  })

  afterEach(() => cleanup())

  it('aborts without first submitting audio through the capsule pointer-up handler', () => {
    render(<Capsule />)

    const cancel = screen.getByRole('button', { name: '取消录音' })
    // Use MouseEvent for the pointer events so jsdom supplies button=0, as
    // browsers do; a generic Event would accidentally skip the parent handler.
    fireEvent(cancel, new MouseEvent('pointerdown', { bubbles: true, button: 0 }))
    fireEvent(cancel, new MouseEvent('pointerup', { bubbles: true, button: 0 }))
    fireEvent.click(cancel)

    expect(invoke).toHaveBeenCalledExactlyOnceWith('abort_recording')
    expect(invoke).not.toHaveBeenCalledWith('stop_recording')
  })
})
