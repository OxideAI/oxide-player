import { useEffect, useRef } from 'react'
import { BINDINGS } from './shortcuts'
import styles from './ShortcutHelp.module.css'

interface ShortcutHelpProps {
  open: boolean
  onClose: () => void
}

export function ShortcutHelp({ open, onClose }: ShortcutHelpProps) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    const t = setTimeout(() => ref.current?.focus(), 0)
    window.addEventListener('keydown', onKey)
    return () => {
      clearTimeout(t)
      window.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  if (!open) return null

  return (
    <div className={styles.scrim} onClick={onClose}>
      <div
        ref={ref}
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.head}>
          <h2 className={styles.title}>Keyboard shortcuts</h2>
          <button className={styles.close} onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>
        <ul className={styles.list}>
          {BINDINGS.map((b) => (
            <li key={b.id} className={styles.row}>
              <kbd className={styles.key}>{b.label}</kbd>
              <span className={styles.desc}>{b.desc}</span>
            </li>
          ))}
        </ul>
        <p className={styles.hint}>Press Esc or click outside to close.</p>
      </div>
    </div>
  )
}
