export type LayoutMode = 'auto' | 'single-line' | 'multi-line'

export interface EditorPreferences {
  autoCopy: boolean
  copyOnClose: boolean
  highlightOnCopy: boolean
  alwaysOnTop: boolean
  layoutMode: LayoutMode
  removeSpaces: boolean
  removePunctuation: boolean
}

const PREFERENCES_KEY = 'popspeak.editor.preferences.v1'
const DEFAULT_PREFERENCES: EditorPreferences = {
  autoCopy: false,
  copyOnClose: false,
  highlightOnCopy: false,
  alwaysOnTop: true,
  layoutMode: 'auto',
  removeSpaces: false,
  removePunctuation: false,
}

export function loadEditorPreferences(): EditorPreferences {
  try {
    const stored = JSON.parse(
      localStorage.getItem(PREFERENCES_KEY) ?? '{}',
    ) as Partial<EditorPreferences>
    return {
      ...DEFAULT_PREFERENCES,
      ...stored,
      layoutMode: ['auto', 'single-line', 'multi-line'].includes(stored.layoutMode ?? '')
        ? (stored.layoutMode as LayoutMode)
        : 'auto',
    }
  } catch {
    return { ...DEFAULT_PREFERENCES }
  }
}

export function saveEditorPreferences(preferences: EditorPreferences) {
  try {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify(preferences))
  } catch (error) {
    console.warn('Unable to persist editor preferences:', error)
  }
}

export function formatEditorText(value: string, preferences: EditorPreferences) {
  let result = value.replace(/\r\n/g, '\n').trim()
  if (preferences.layoutMode === 'single-line') {
    result = result.replace(/\s*\n+\s*/g, ' ')
  } else if (preferences.layoutMode === 'multi-line') {
    result = result
      .replace(/\s*\n+\s*/g, '')
      .replace(/([。！？!?；;])\s*/g, '$1\n')
      .replace(/\n+$/g, '')
  } else {
    result = result
      .split('\n')
      .map((line) => line.trim())
      .join('\n')
      .replace(/\n{3,}/g, '\n\n')
  }
  if (preferences.removeSpaces) result = result.replace(/[\t \u3000]+/g, '')
  if (preferences.removePunctuation) {
    result = result.replace(/[，。！？；：、,.!?;:"“”‘’（）()[\]{}《》<>…—-]/g, '')
  }
  return result
}
