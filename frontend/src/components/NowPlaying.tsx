import { useEffect, useState } from 'react'
import type { MouseEvent } from 'react'
import type { PlayerStatus, QueueResponse } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, audioQuality } from '../util'
import { QueueView } from './QueueView'
import styles from './NowPlaying.module.css'

interface Props {
  status: PlayerStatus | null
  onTogglePlay: () => void
  onStop: () => void
  onNext: () => void
  onPrev: () => void
  onSeek: (seconds: number) => void
  onVolume: (volume: number) => void
  onReloadStatus: () => void
}
export function NowPlaying({
  status,
  onTogglePlay,
  onStop,
  onNext,
  onPrev,
  onSeek,
  onVolume,
  onReloadStatus,
}: Props) {
  const song = status?.current_song ?? null
  const elapsed = status?.elapsed ?? 0
  const duration = status?.duration ?? 0
  const fraction = duration > 0 ? Math.min(1, elapsed / duration) : 0

  const [queue, setQueue] = useState<QueueResponse | null>(null)
  const [queueOpen, setQueueOpen] = useState(false)

  // Keep the open panel in sync with playback (next/prev, external changes).
  useEffect(() => {
    if (!queueOpen) return
    let alive = true
    const tick = async () => {
      try {
        const q = await api.queue()
        if (alive) setQueue(q)
      } catch {
        /* keep previous */
      }
    }
    const id = setInterval(tick, 1000)
    return () => {
      alive = false
      clearInterval(id)
    }
  }, [queueOpen])

  const onScrub = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    const frac = (e.clientX - rect.left) / rect.width
    if (duration > 0) onSeek(frac * duration)
  }

  const toggleQueue = async () => {
    if (!queueOpen) {
      try {
        setQueue(await api.queue())
      } catch {
        setQueue({ entries: [], current: null })
      }
    }
    setQueueOpen((o) => !o)
  }

  const onShuffle = async () => {
    await api.shuffle(!status?.random)
    onReloadStatus()
    if (queueOpen) {
      try {
        setQueue(await api.queue())
      } catch {
        /* keep previous */
      }
    }
  }

  const onJump = async (pos: number) => {
    await api.jump(pos)
    onReloadStatus()
    setQueueOpen(false)
  }

  const onRemove = async (pos: number) => {
    await api.remove(pos)
    onReloadStatus()
    try {
      setQueue(await api.queue())
    } catch {
      /* keep previous */
    }
  }

  return (
    <footer className={styles.bar}>
      <div className={styles.meta}>
        <a className={styles.coverLink} href="/kiosk" title="Open kiosk mode">
          {song?.has_cover ? (
            <img className={styles.cover} src={api.coverUrl(song.id)} alt="" />
          ) : (
            <div className={styles.coverPlaceholder}>♪</div>
          )}
        </a>
        <div className={styles.text}>
          <div className={styles.title}>{song ? displayTitle(song) : 'Nothing playing'}</div>
          <div className={styles.artist}>
            {[song?.artist, song?.album].filter(Boolean).join(' — ')}
          </div>
          {song && <div className={styles.quality}>{audioQuality(song)}</div>}
        </div>
      </div>

      <div className={styles.center}>
        <div className={styles.controls}>
          <button className={styles.btn} onClick={onPrev} aria-label="previous">
            ⏮
          </button>
          <button className={styles.btnPrimary} onClick={onTogglePlay} aria-label="play/pause">
            {status?.state === 'playing' ? '⏸' : '▶'}
          </button>
          <button className={styles.btn} onClick={onStop} aria-label="stop">
            ⏹
          </button>
          <button className={styles.btn} onClick={onNext} aria-label="next">
            ⏭
          </button>
        </div>
        <div className={styles.progressWrap}>
          <span className={styles.time}>{fmtTime(elapsed)}</span>
          <div className={styles.progress} onClick={onScrub}>
            <div className={styles.progressFill} style={{ width: `${fraction * 100}%` }} />
          </div>
          <span className={styles.time}>{fmtTime(duration)}</span>
        </div>
      </div>

      <div className={styles.right}>
        {queueOpen && queue && (
          <QueueView
            queue={queue}
            onJump={onJump}
            onRemove={onRemove}
            onClose={() => setQueueOpen(false)}
          />
        )}
        <div className={styles.volume}>
          <button
            className={status?.random ? `${styles.btn} ${styles.btnActive}` : styles.btn}
            onClick={onShuffle}
            aria-label="shuffle queue"
            aria-pressed={status?.random ?? false}
            title={status?.random ? 'Shuffle: on' : 'Shuffle: off'}
          >
            🔀
          </button>
          <button
            className={queueOpen ? `${styles.btn} ${styles.btnActive}` : styles.btn}
            onClick={toggleQueue}
            aria-label="view queue"
            aria-pressed={queueOpen}
            title="View queue"
          >
            ☰
          </button>
          <span aria-hidden>🔊</span>
          <input
            type="range"
            min={0}
            max={100}
            value={status?.volume ?? 0}
            onChange={(e) => onVolume(Number(e.target.value))}
          />
        </div>
      </div>
    </footer>
  )
}
