import { useAppStore } from '../../stores/appStore'

export function DurationTimer() {
  // The capsule owns the clock so collapsing/opening its POP never resets it.
  const seconds = useAppStore((s) => s.recordingDuration)
  return (
    <span className="text-[11px] font-mono text-white/80 tabular-nums">
      {String(Math.floor(seconds / 60)).padStart(2, '0')}:{String(seconds % 60).padStart(2, '0')}
    </span>
  )
}
