import { useCallback, useEffect, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import type { PlayerStatus, QueueResponse } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, audioQuality } from '../util'
import { QueueView } from './QueueView'
import styles from './NowPlaying.module.css'

interface Props {
  status: PlayerStatus | null
  onTogglePlay: () => void
  onNext: () => void
  onPrev: () => void
  onSeek: (seconds: number) => void
  onVolume: (volume: number) => void
  onReloadStatus: () => void
}
export function NowPlaying({
  status,
  onTogglePlay,
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
  const playing = status?.state === 'playing'

  const [queue, setQueue] = useState<QueueResponse | null>(null)
  const [queueOpen, setQueueOpen] = useState(false)

  const reqId = useRef(0)
  const loadQueue = useCallback(async () => {
    const token = ++reqId.current
    try {
      const q = await api.queue()
      if (token === reqId.current) setQueue(q)
    } catch {
      /* keep previous */
    }
  }, [])

  useEffect(() => {
    if (!queueOpen) return
    const id = setInterval(loadQueue, 1000)
    return () => clearInterval(id)
  }, [queueOpen, loadQueue])

  const shuffleRef = useRef<boolean | undefined>(status?.random)
  useEffect(() => {
    shuffleRef.current = status?.random
  }, [status?.random])

  const onScrub = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    const frac = (e.clientX - rect.left) / rect.width
    if (duration > 0) onSeek(frac * duration)
  }

  const toggleQueue = async () => {
    if (!queueOpen) {
      await loadQueue()
    }
    setQueueOpen((o) => !o)
  }

  const onShuffle = async () => {
    const next = !shuffleRef.current
    shuffleRef.current = next
    await api.shuffle(next)
    onReloadStatus()
    if (queueOpen) await loadQueue()
  }

  const onJump = async (pos: number) => {
    await api.jump(pos)
    onReloadStatus()
    setQueueOpen(false)
  }

  const onRemove = async (pos: number) => {
    await api.remove(pos)
    onReloadStatus()
    await loadQueue()
  }

  return (
    <footer className={styles.bar}>
      <div className={styles.meta}>
        <a className={styles.coverShell} href="/kiosk" title="Open kiosk mode">
          <span className={styles.coverCore}>
            {song?.has_cover ? (
              <img className={styles.cover} src={api.coverUrl(song.id)} alt="" />
            ) : (
              <span className={styles.coverPlaceholder}>
                <span className={styles.eq} aria-hidden>
                  <i />
                  <i />
                  <i />
                </span>
              </span>
            )}
            {playing && song?.has_cover && <span className={styles.coverGlow} aria-hidden />}
          </span>
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
          <button className={styles.iconBtn} onClick={onPrev} aria-label="previous">
            <Glyph name="prev" />
          </button>
          <button className={styles.playBtn} onClick={onTogglePlay} aria-label="play/pause">
            <span className={styles.playCore}>{playing ? <Glyph name="pause" /> : <Glyph name="play" />}</span>
          </button>
          <button className={styles.iconBtn} onClick={onNext} aria-label="next">
            <Glyph name="next" />
          </button>
        </div>
        <div className={styles.progressWrap}>
          <span className={styles.time}>{fmtTime(elapsed)}</span>
          <div
            className={styles.progress}
            onClick={onScrub}
            style={{ ['--frac' as string]: fraction }}
            role="slider"
            aria-label="Seek"
            aria-valuemin={0}
            aria-valuemax={Math.floor(duration)}
            aria-valuenow={Math.floor(elapsed)}
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'ArrowRight') onSeek(Math.min(duration, elapsed + 5))
              if (e.key === 'ArrowLeft') onSeek(Math.max(0, elapsed - 5))
            }}
          >
            <div className={styles.progressFill} />
            <div className={styles.progressThumb} />
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
            className={`${styles.iconBtn} ${status?.random ? styles.iconActive : ''}`}
            onClick={onShuffle}
            aria-label="shuffle queue"
            aria-pressed={status?.random ?? false}
            title={status?.random ? 'Shuffle: on' : 'Shuffle: off'}
          >
            <Glyph name="shuffle" />
          </button>
          <button
            className={`${styles.iconBtn} ${queueOpen ? styles.iconActive : ''}`}
            onClick={toggleQueue}
            aria-label="view queue"
            aria-pressed={queueOpen}
            title="View queue"
          >
            <Glyph name="queue" />
          </button>
          <span className={styles.volIcon} aria-hidden>
            <Glyph name="vol" />
          </span>
          <input
            className={styles.volRange}
            type="range"
            min={0}
            max={100}
            value={status?.volume ?? 0}
            style={{ ['--val' as string]: status?.volume ?? 0 }}
            onChange={(e) => onVolume(Number(e.target.value))}
            aria-label="Volume"
          />
        </div>
      </div>
    </footer>
  )
}

type GlyphName = 'prev' | 'play' | 'pause' | 'next' | 'shuffle' | 'queue' | 'vol'
function Glyph({ name }: { name: GlyphName }) {
  const c = 'var(--text)'
  const a = 'var(--accent)'
  switch (name) {
    case 'prev':
      return (
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M18 6v12M8 12l8-6v12l-8-6z" />
        </svg>
      )
    case 'next':
      return (
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M6 6v12M16 12L8 6v12l8-6z" />
        </svg>
      )
    case 'play':
      return (
        <svg viewBox="0 0 24 24" width="20" height="20" fill={a} stroke="none">
          <path d="M8 5.5v13l11-6.5-11-6.5z" />
        </svg>
      )
    case 'pause':
      return (
        <svg viewBox="0 0 24 24" width="20" height="20" fill={a} stroke="none">
          <rect x="7" y="5.5" width="3.4" height="13" rx="1.2" />
          <rect x="13.6" y="5.5" width="3.4" height="13" rx="1.2" />
        </svg>
      )
    case 'shuffle':
      return (
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M3 5h3l11 14h3M3 19h3l3.5-4.3M14.5 9.3 18 5h3M18 5l-3-2.4M18 5l3-2.4M6 19l-3 2.4M6 19l3 2.4" />
        </svg>
      )
    case 'queue':
      return (
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M4 6h12M4 12h12M4 18h7M18 11v7M18 18a2.4 2.4 0 1 0 0 .01" />
        </svg>
      )
    case 'vol':
      return (
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke={c} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M4 9v6h4l5 4V5L8 9H4zM16.5 8.5a5 5 0 0 1 0 7M19 6a8 8 0 0 1 0 12" />
        </svg>
      )
  }
}
