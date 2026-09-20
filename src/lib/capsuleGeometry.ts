export interface ScreenRect {
  x: number
  y: number
  width: number
  height: number
}

export const CAPSULE_INSET = 12

export function capsuleContentSize(
  state: string,
  hasError: boolean,
  menu: boolean,
  popup: boolean,
) {
  if (menu) return { width: 264, height: 324 }
  if (popup) return { width: 320, height: 248 }
  // Include the one-pixel native surface border in the idle diameter.
  return { width: hasError || state !== 'idle' ? 264 : 38, height: 38 }
}

/** Physical pixels, including negative monitor origins and taskbar work areas. */
export function placeCapsule(
  area: ScreenRect,
  size: { width: number; height: number },
  previous?: ScreenRect,
  margin = 20,
): ScreenRect {
  const width = Math.min(size.width, area.width)
  const height = Math.min(size.height, area.height)
  const right = previous ? previous.x + previous.width : area.x + area.width - margin
  const bottom = previous ? previous.y + previous.height : area.y + area.height - margin
  return {
    x: Math.round(Math.max(area.x, Math.min(right - width, area.x + area.width - width))),
    y: Math.round(Math.max(area.y, Math.min(bottom - height, area.y + area.height - height))),
    width: Math.round(width),
    height: Math.round(height),
  }
}
