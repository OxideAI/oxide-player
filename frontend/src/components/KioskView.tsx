import { useEffect, useState } from 'react'
import type { PlayerStatus, QueueResponse, Config } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, audioQuality, folderKey } from '../util'
import { useDragValue, useSmoothElapsed } from './playerHooks'
import { Visualizer, DEFAULT_VIZ_PARAMS, type VizParams } from './Visualizer'
import { VisualizerControls } from './VisualizerControls'
import { useVisualizer } from '../useVisualizer'
import styles from './KioskView.module.css'

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

export function KioskView({
  status,
  onTogglePlay,
  onNext,
  onPrev,
  onSeek,
  onVolume,
  onOpenAlbum,
}: Props) {
  const loading = status === null
  const song = status?.current_song ?? null
  const cover = song?.has_cover ? api.coverUrl(song.cover_key ?? song.id) : null
  const title = loading ? 'Loading…' : (song ? displayTitle(song) : 'Nothing playing')
  const duration = status?.duration ?? 0
  const playing = status?.state === 'playing'

  const { elapsed: smoothElapsed, reset: resetElapsed } = useSmoothElapsed(status, duration)
  const seek = useDragValue(smoothElapsed, onSeek)
  const vol = useDragValue(status?.volume ?? 0, onVolume)
  const volumeAvailable = status?.volume !== null && status?.volume !== undefined

  // Whether the real FFT visualizer is enabled server-side. When off we pass
  // `enabled=false` so the hook stays disconnected (zero cost).
  const [fftEnabled, setFftEnabled] = useState(false)
  useEffect(() => {
    let alive = true
    api.getConfig().then((c: Config) => { if (alive) setFftEnabled(!!c.visualizer_fft) }).catch(() => {})
    return () => { alive = false }
  }, [])
  const frame = useVisualizer(fftEnabled)

  // Temporary live-tuning state for the visualizer (sliders + save button).
  // Loads the saved look from disk (`/api/visualizer/params`) on mount, so a
  // restart keeps the look you tuned; falls back to the code defaults.
  const [vizParams, setVizParams] = useState<VizParams>(DEFAULT_VIZ_PARAMS)
  const [vizTuning, setVizTuning] = useState(false)
  useEffect(() => {
    let alive = true
    api.getVizParams()
      .then((p: Record<string, number>) => {
        if (!alive) return
        // Backend keys are snake_case; map to the frontend camelCase shape.
        setVizParams({
          bloomAlpha: p.bloom_alpha ?? DEFAULT_VIZ_PARAMS.bloomAlpha,
          bloomBeat: p.bloom_beat ?? DEFAULT_VIZ_PARAMS.bloomBeat,
          bloomEnergy: p.bloom_energy ?? DEFAULT_VIZ_PARAMS.bloomEnergy,
          bloomRadius: p.bloom_radius ?? DEFAULT_VIZ_PARAMS.bloomRadius,
          barIdle: p.bar_idle ?? DEFAULT_VIZ_PARAMS.barIdle,
          barPeak: p.bar_peak ?? DEFAULT_VIZ_PARAMS.barPeak,
          barGap: p.bar_gap ?? DEFAULT_VIZ_PARAMS.barGap,
          barRadius: p.bar_radius ?? DEFAULT_VIZ_PARAMS.barRadius,
          phaseSpeed: p.phase_speed ?? DEFAULT_VIZ_PARAMS.phaseSpeed,
          blur: p.blur ?? DEFAULT_VIZ_PARAMS.blur,
        })
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  const seekFrac = duration > 0 ? Math.min(1, seek.local / duration) : 0

  return (
    <div className={styles.kiosk}>
      <Visualizer
        playing={playing}
        frame={frame}
        params={vizParams}
      />
      <button
        className={styles.tune}
        onClick={() => setVizTuning((v) => !v)}
        title="Tune visualizer"
        aria-label="Tune visualizer"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
          <path d="M4 8h10M18 8h2M4 16h2M10 16h10M14 6v4M8 14v4" />
        </svg>
      </button>
      {vizTuning && (
        <VisualizerControls
          params={vizParams}
          onChange={setVizParams}
          onClose={() => setVizTuning(false)}
        />
      )}
      <button
        className={styles.exit}
        onClick={() => {
          if (window.history.length > 1) window.history.back()
          else window.location.pathname = window.location.pathname.replace(/\/kiosk$/, '') || '/'
        }}
        title="Back"
        aria-label="Back"
      >
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
          <path d="M6 6l12 12M18 6 6 18" />
        </svg>
      </button>

      <div className={styles.stage}>
        <div className={styles.art} style={cover ? { backgroundImage: `url(${cover})` } : undefined}>
          {!cover && <span className={styles.note} aria-hidden />}
          {cover && <span className={styles.artGlow} aria-hidden />}
        </div>

        <div className={styles.meta}>
          <span className={styles.eyebrow}>Now playing</span>
          <div className={styles.title}>{title}</div>
          {song && (
            <button
              type="button"
              className={styles.subBtn}
              onClick={() => onOpenAlbum(folderKey(song.uri))}
              title="Open album"
              aria-label={`Open album: ${[song.artist, song.album].filter(Boolean).join(' — ')}`}
            >
              {[song.artist, song.album].filter(Boolean).join(' · ')}
            </button>
          )}
          {song && <div className={styles.quality}>{audioQuality(song)}</div>}
        </div>

        <div className={styles.progress}>
          <span className={styles.time}>{fmtTime(seek.isDragging() ? seek.local : smoothElapsed)}</span>
          <input
            className={styles.bar}
            type="range"
            min={0}
            max={duration || 0}
            step="any"
            value={seek.local}
            style={{ ['--frac' as string]: seekFrac }}
            onPointerDown={seek.begin}
            onChange={(e) => seek.move(Number(e.target.value))}
            onPointerUp={(e) => { resetElapsed(Number((e.target as HTMLInputElement).value)); seek.end() }}
            onPointerCancel={(e) => { resetElapsed(Number((e.target as HTMLInputElement).value)); seek.end() }}
            aria-label="Seek"
          />
          <span className={styles.time}>{fmtTime(duration)}</span>
        </div>

        <div className={styles.controls}>
          <button className={styles.btn} onClick={onPrev} aria-label="previous">
            <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 6v12M8 12l8-6v12l-8-6z" />
            </svg>
          </button>
          <button className={`${styles.btn} ${styles.main}`} onClick={onTogglePlay} aria-label="play/pause">
            <span className={styles.playCore}>
              {playing ? (
                <svg viewBox="0 0 24 24" width="30" height="30" fill="currentColor" stroke="none">
                  <rect x="7" y="5.5" width="3.4" height="13" rx="1.2" />
                  <rect x="13.6" y="5.5" width="3.4" height="13" rx="1.2" />
                </svg>
              ) : (
                <svg viewBox="0 0 24 24" width="30" height="30" fill="currentColor" stroke="none">
                  <path d="M8 5.5v13l11-6.5-11-6.5z" />
                </svg>
              )}
            </span>
          </button>
          <button className={styles.btn} onClick={onNext} aria-label="next">
            <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M6 6v12M16 12L8 6v12l8-6z" />
            </svg>
          </button>
        </div>

        <div className={styles.volume}>
          {volumeAvailable && (
            <>
              <span className={styles.volIcon} aria-hidden>
                <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M4 9v6h4l5 4V5L8 9H4zM16.5 8.5a5 5 0 0 1 0 7M19 6a8 8 0 0 1 0 12" />
                </svg>
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
    </div>
  )
}
