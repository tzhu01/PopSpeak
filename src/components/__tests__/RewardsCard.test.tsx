import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { RewardsCard } from '../RewardsCard'
import type { RewardSummary } from '../../lib/tauri'

const { getSummary, subscribe } = vi.hoisted(() => ({
  getSummary: vi.fn(),
  subscribe: vi.fn(),
}))
vi.mock('../../lib/tauri', () => ({ getRewardSummary: getSummary }))
vi.mock('@tauri-apps/api/event', () => ({ listen: subscribe }))

const summary: RewardSummary = {
  total_points: 12,
  today_points: 3,
  daily_limit: 100,
  minimum_duration_ms: 2000,
  local_day: '2026-09-18',
  redemption_available: false,
  recent_activity: [{ history_id: 3, points: 1, credited_at: '2026-09-18T10:00:00+08:00' }],
}
let onUpdate: (() => void) | undefined

beforeEach(() => {
  vi.clearAllMocks()
  onUpdate = undefined
  getSummary.mockResolvedValue(summary)
  subscribe.mockImplementation(async (_event: string, handler: () => void) => {
    onUpdate = handler
    return vi.fn()
  })
})
afterEach(cleanup)

describe('local experience rewards', () => {
  it('shows native points and keeps redemption clearly unavailable', async () => {
    render(<RewardsCard />)
    expect(await screen.findByText('12')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '礼品兑换 · 待上线' })).toBeDisabled()
    expect(screen.getByText(/后续兑换需服务端核验/)).toBeInTheDocument()
    expect(getSummary).toHaveBeenCalledTimes(1)
  })

  it('refreshes after the native successful-history event and window focus', async () => {
    render(<RewardsCard />)
    await screen.findByText('12')
    getSummary.mockResolvedValue({ ...summary, total_points: 13, today_points: 4 })
    onUpdate?.()
    expect(await screen.findByText('13')).toBeInTheDocument()
    getSummary.mockResolvedValue({ ...summary, total_points: 14, today_points: 5 })
    fireEvent.focus(window)
    expect(await screen.findByText('14')).toBeInTheDocument()
    expect(subscribe).toHaveBeenCalledWith('rewards:updated', expect.any(Function))
  })

  it('does not invent a zero balance when native storage cannot be read, and supports retry', async () => {
    getSummary.mockRejectedValueOnce(new Error('database unavailable'))
    render(<RewardsCard />)
    expect(await screen.findByText(/积分读取失败/)).toBeInTheDocument()
    expect(screen.queryByText('0')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '重试' }))
    expect(await screen.findByText('12')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByText(/积分读取失败/)).not.toBeInTheDocument())
  })
})
