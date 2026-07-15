import type { QueueResponse } from '../types'
import { fmtTime, displayTitle } from '../util'
import styles from './QueueView.module.css'

interface Props {
  queue: QueueResponse
  onJump: (pos: number) => void
  onRemove: (pos: number) => void
  onClose: () => void
}

export function QueueView({ queue, onJump, onRemove, onClose }: Props) {
  return (
    <div className={styles.panel}>
      <div className={styles.head}>
        <span className={styles.title}>Queue · {queue.entries.length}</span>
        <button className={styles.close} onClick={onClose} aria-label="Close queue">
          ✕
        </button>
      </div>
      <ul className={styles.list}>
        {queue.entries.length === 0 && (
          <li className={styles.empty}>The queue is empty.</li>
        )}
        {queue.entries.map((t) => {
          const active = queue.current !== null && t.pos === queue.current
          return (
            <li
              key={t.id}
              className={active ? styles.itemActive : styles.item}
              onClick={() => onJump(t.pos)}
            >
              <span className={styles.pos}>{t.pos + 1}</span>
              <span className={styles.meta}>
                <span className={styles.tTitle}>{displayTitle(t)}</span>
                <span className={styles.tArtist}>{t.artist ?? t.album ?? '—'}</span>
              </span>
              <span className={styles.tTime}>{fmtTime(t.duration ?? 0)}</span>
              <button
                className={styles.remove}
                aria-label="Remove from queue"
                onClick={(e) => {
                  e.stopPropagation()
                  onRemove(t.pos)
                }}
              >
                ✕
              </button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
