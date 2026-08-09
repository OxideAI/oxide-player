import { useEffect } from 'react'
import styles from './ShortcutToast.module.css'

interface ShortcutToastProps {
  text: string | null
  onClear: () => void
}

export function ShortcutToast({ text, onClear }: ShortcutToastProps) {
  useEffect(() => {
    if (!text) return
    const t = setTimeout(onClear, 4000)
    return () => clearTimeout(t)
  }, [text, onClear])
  if (!text) return null
  return (
    <div className={styles.toast} role="status" aria-live="polite">
      {text}
    </div>
  )
}
