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
    <div className={styles.panel} role="dialog" aria-label="Play queue">
      <div className={styles.head}>
        <span className={styles.eyebrow}>Queue · {queue.entries.length}</span>
        <button className={styles.close} onClick={onClose} aria-label="Close queue">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
            <path d="M6 6l12 12M18 6 6 18" />
          </svg>
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
              {active && (
                <span className={styles.live} aria-hidden>
                  <i />
                  <i />
                  <i />
                </span>
              )}
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
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
                  <path d="M6 6l12 12M18 6 6 18" />
                </svg>
              </button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
