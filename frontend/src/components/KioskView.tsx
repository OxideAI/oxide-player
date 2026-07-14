import type { PlayerStatus } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, audioQuality } from '../util'
import styles from './KioskView.module.css'

interface Props {
  status: PlayerStatus | null
  onTogglePlay: () => void
  onStop: () => void
  onNext: () => void
  onPrev: () => void
  onSeek: (seconds: number) => void
  onVolume: (volume: number) => void
}

export function KioskView({
  status,
  onTogglePlay,
  onStop,
  onNext,
  onPrev,
  onSeek,
  onVolume,
}: Props) {
  const song = status?.current_song ?? null
  const cover = song?.has_cover ? api.coverUrl(song.id) : null
  const title = song ? displayTitle(song) : 'Nothing playing'
  const elapsed = status?.elapsed ?? 0
  const duration = status?.duration ?? 0
  const playing = status?.state === 'playing'

  return (
    <div className={styles.kiosk}>
      <a className={styles.exit} href="/" title="Back to library">
        ✕
      </a>

      <div className={styles.art} style={cover ? { backgroundImage: `url(${cover})` } : undefined}>
        {!cover && <div className={styles.note}>♪</div>}
      </div>

      <div className={styles.meta}>
        <div className={styles.title}>{title}</div>
        {song && (
          <div className={styles.sub}>
            {[song.artist, song.album].filter(Boolean).join(' · ')}
          </div>
        )}
        {song && <div className={styles.quality}>{audioQuality(song)}</div>}
      </div>

      <div className={styles.progress}>
        <span className={styles.time}>{fmtTime(elapsed)}</span>
        <input
          className={styles.bar}
          type="range"
          min={0}
          max={Math.floor(duration) || 0}
          step={1}
          value={Math.floor(elapsed)}
          onChange={(e) => onSeek(Number(e.target.value))}
        />
        <span className={styles.time}>{fmtTime(duration)}</span>
      </div>

      <div className={styles.controls}>
        <button className={styles.btn} onClick={onPrev} aria-label="previous">
          ⏮
        </button>
        <button
          className={`${styles.btn} ${styles.main}`}
          onClick={onTogglePlay}
          aria-label="play/pause"
        >
          {playing ? '⏸' : '▶'}
        </button>
        <button className={styles.btn} onClick={onStop} aria-label="stop">
          ⏹
        </button>
        <button className={styles.btn} onClick={onNext} aria-label="next">
          ⏭
        </button>
      </div>

      <div className={styles.volume}>
        <span className={styles.volIcon}>🔊</span>
        <input
          type="range"
          min={0}
          max={100}
          value={status?.volume ?? 0}
          onChange={(e) => onVolume(Number(e.target.value))}
        />
      </div>
    </div>
  )
}
