import { useEffect, useRef, useState } from 'react'
import type { PointerEvent } from 'react'
import type { PlayerStatus, QueueResponse } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, audioQuality, folderKey } from '../util'
import { QueueView } from './QueueView'
import { fracFromPointer, useDragValue, useSmoothElapsed } from './playerHooks'
import styles from './NowPlaying.module.css'

interface Props {
  status: PlayerStatus | null
  queue: QueueResponse | null
  onTogglePlay: () => void
  onNext: () => void
  onPrev: () => void
  onSeek: (seconds: number) => void
  onVolume: (volume: number) => void
  onOpenAlbum: (album: string) => void
}
export function NowPlaying({
  status,
  queue,
  onTogglePlay,
  onNext,
  onPrev,
  onSeek,
  onVolume,
  onOpenAlbum,
}: Props) {
  const loading = status === null
  const song = status?.current_song ?? null
  const duration = status?.duration ?? 0
  const playing = status?.state === 'playing'

  const { elapsed: smoothElapsed, reset: resetElapsed } = useSmoothElapsed(status, duration)
  const seek = useDragValue(smoothElapsed, onSeek)
  const vol = useDragValue(status?.volume ?? 0, onVolume)
  const volumeAvailable = status?.volume !== null && status?.volume !== undefined
  const displayElapsed = seek.isDragging() ? seek.local : smoothElapsed
  const fraction = duration > 0 ? Math.min(1, seek.local / duration) : 0

  // Queue is pushed over the WebSocket (App owns the connection); no polling.
  const [queueOpen, setQueueOpen] = useState(false)

  const shuffleRef = useRef<boolean | undefined>(status?.random)
  useEffect(() => {
    shuffleRef.current = status?.random
  }, [status?.random])

  const onScrubDown = (e: PointerEvent<HTMLDivElement>) => {
    if (duration <= 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    seek.begin()
    seek.move(fracFromPointer(e.currentTarget, e.clientX) * duration)
  }
  const onScrubMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!seek.isDragging()) return
    seek.move(fracFromPointer(e.currentTarget, e.clientX) * duration)
  }
  const onScrubUp = (e: PointerEvent<HTMLDivElement>) => {
    if (!seek.isDragging()) return
    const v = fracFromPointer(e.currentTarget, e.clientX) * duration
    seek.move(v)
    resetElapsed(v)
    seek.end()
  }

  const toggleQueue = () => {
    setQueueOpen((o) => !o)
  }

  const onShuffle = async () => {
    const next = !shuffleRef.current
    shuffleRef.current = next
    await api.shuffle(next)
  }

  const onJump = async (pos: number) => {
    await api.jump(pos)
    setQueueOpen(false)
  }

  const onRemove = async (pos: number) => {
    await api.remove(pos)
  }

  const onClearQueue = async () => {
    await api.clearQueue()
  }

  return (
    <footer className={styles.bar}>
      <div className={styles.meta}>
        <a className={styles.coverShell} href="/kiosk" title="Open kiosk mode">
          <span className={styles.coverCore}>
            {api.coverFor(!!song?.has_cover, song?.cover_key ?? null, song?.id ?? 0) ? (
              <img
                className={styles.cover}
                src={api.coverFor(!!song?.has_cover, song?.cover_key ?? null, song?.id ?? 0)!}
                alt=""
              />
            ) : (
              <span className={styles.coverPlaceholder}>
                <span className={styles.eq} aria-hidden>
                  <i />
                  <i />
                  <i />
                </span>
              </span>
            )}
            {api.coverFor(!!song?.has_cover, song?.cover_key ?? null, song?.id ?? 0) && playing && (
              <span className={styles.coverGlow} aria-hidden />
            )}
          </span>
        </a>
        <div className={styles.text}>
          <div className={styles.title}>{loading ? 'Loading…' : (song ? displayTitle(song) : 'Nothing playing')}</div>
          {song ? (
            <button
              type="button"
              className={styles.artistBtn}
              onClick={() => onOpenAlbum(folderKey(song.uri))}
              title="Open album"
              aria-label={`Open album: ${[song.artist, song.album].filter(Boolean).join(' — ')}`}
            >
              {[song.artist, song.album].filter(Boolean).join(' — ')}
            </button>
          ) : (
            <div className={styles.artist} />
          )}
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
          <span className={styles.time}>{fmtTime(displayElapsed)}</span>
          <div
            className={styles.progress}
            style={{ ['--frac' as string]: fraction }}
            role="slider"
            aria-label="Seek"
            aria-valuemin={0}
            aria-valuemax={Math.floor(duration)}
            aria-valuenow={Math.floor(displayElapsed)}
            tabIndex={0}
            onPointerDown={onScrubDown}
            onPointerMove={onScrubMove}
            onPointerUp={onScrubUp}
            onPointerCancel={onScrubUp}
            onKeyDown={(e) => {
              if (e.key === 'ArrowRight') onSeek(Math.min(duration, displayElapsed + 5))
              if (e.key === 'ArrowLeft') onSeek(Math.max(0, displayElapsed - 5))
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
            onClear={onClearQueue}
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
          {volumeAvailable && (
            <>
              <span className={styles.volIcon} aria-hidden>
                <Glyph name="vol" />
              </span>
              <input
                className={styles.volRange}
                type="range"
                min={0}
                max={100}
                value={vol.local}
                style={{ ['--val' as string]: vol.local }}
                onPointerDown={vol.begin}
                onChange={(e) => vol.move(Number(e.target.value))}
                onPointerUp={vol.end}
                onPointerCancel={vol.end}
                aria-label="Volume"
              />
            </>
          )}
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
