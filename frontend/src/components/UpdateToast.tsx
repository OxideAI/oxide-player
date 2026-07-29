import styles from './Pwa.module.css'

interface UpdateToastProps {
  show: boolean
  onReload: () => void
  onDismiss: () => void
}

export function UpdateToast({ show, onReload, onDismiss }: UpdateToastProps) {
  if (!show) return null

  return (
    <div className={styles.update} role="alertdialog" aria-label="Update available">
      <span className={styles.updateText}>A new version of Oxide is ready.</span>
      <button className={styles.updateAction} onClick={onReload}>
        Reload
      </button>
      <button className={styles.updateDismiss} onClick={onDismiss} aria-label="Dismiss update">
        ✕
      </button>
    </div>
  )
}
