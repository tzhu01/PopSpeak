import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CapsuleStageProgress } from '../CapsuleStageProgress'

describe('CapsuleStageProgress', () => {
  it('exposes the current pipeline stage to assistive technology', () => {
    render(<CapsuleStageProgress value={72} label="AI polishing · 2/3" />)

    const progress = screen.getByRole('progressbar', { name: 'AI polishing · 2/3' })
    expect(progress).toHaveAttribute('aria-valuenow', '72')
    expect(progress).toHaveAttribute('aria-valuemin', '0')
    expect(progress).toHaveAttribute('aria-valuemax', '100')
  })

  it('clamps values to the valid progress range', () => {
    const { container, rerender } = render(<CapsuleStageProgress value={120} label="Complete" />)
    expect(within(container).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')

    rerender(<CapsuleStageProgress value={-1} label="Starting" />)
    expect(within(container).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0')
  })
})
