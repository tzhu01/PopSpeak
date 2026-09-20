interface BrandMarkProps {
  size?: number
  className?: string
  monochrome?: boolean
}

/** PopSpeak's speech-bubble waveform. Kept in sync with app-icon.svg. */
export function BrandMark({ size = 36, className = '', monochrome = false }: BrandMarkProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <rect
        x="2"
        y="2"
        width="60"
        height="60"
        rx="17"
        fill={monochrome ? 'currentColor' : '#E64B3C'}
      />
      <path
        d="M15 19.5C15 15.9 17.9 13 21.5 13h21C46.1 13 49 15.9 49 19.5v22c0 3.6-2.9 6.5-6.5 6.5H29L20 54v-6.5c-2.9-.7-5-3.3-5-6.5V19.5Z"
        stroke="#FFF9F1"
        strokeWidth="3.3"
        strokeLinejoin="round"
      />
      <path
        d="M20 32h5l4-8 5 16 4-11 2.5 3H44"
        stroke="#FFF9F1"
        strokeWidth="3.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}
