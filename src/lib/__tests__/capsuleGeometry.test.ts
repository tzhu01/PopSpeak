import { describe, expect, it } from 'vitest'
import { placeCapsule } from '../capsuleGeometry'

describe('capsule screen geometry (physical pixels)', () => {
  it('defaults to the bottom-right of the taskbar-excluding work area', () => {
    expect(
      placeCapsule({ x: 0, y: 0, width: 1920, height: 1040 }, { width: 60, height: 60 }),
    ).toEqual({ x: 1840, y: 960, width: 60, height: 60 })
  })

  it('supports monitors left of and above the primary monitor', () => {
    expect(
      placeCapsule({ x: -1920, y: -1080, width: 1920, height: 1040 }, { width: 60, height: 60 }),
    ).toEqual({ x: -80, y: -120, width: 60, height: 60 })
  })

  it('keeps the bottom-right anchor when a POP expands and contracts', () => {
    const area = { x: -1920, y: 0, width: 1920, height: 1040 }
    const idle = placeCapsule(area, { width: 60, height: 60 })
    const pop = placeCapsule(area, { width: 344, height: 272 }, idle)
    expect(pop.x + pop.width).toBe(idle.x + idle.width)
    expect(pop.y + pop.height).toBe(idle.y + idle.height)
    expect(placeCapsule(area, { width: 60, height: 60 }, pop)).toEqual(idle)
  })

  it('clamps a stale monitor position and an oversized popup inside the work area', () => {
    expect(
      placeCapsule(
        { x: 0, y: 30, width: 300, height: 220 },
        { width: 344, height: 272 },
        { x: -1000, y: -2000, width: 60, height: 60 },
      ),
    ).toEqual({ x: 0, y: 30, width: 300, height: 220 })
  })

  it('uses physical sizes and margins exactly once at 150% DPI', () => {
    const scale = 1.5
    expect(
      placeCapsule(
        { x: 0, y: 0, width: 2880, height: 1560 },
        { width: (320 + 24) * scale, height: (248 + 24) * scale },
        undefined,
        20 * scale,
      ),
    ).toEqual({ x: 2334, y: 1122, width: 516, height: 408 })
  })
})
