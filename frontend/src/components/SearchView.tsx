import { useCallback, useEffect, useState } from 'react'
import type { Track } from '../types'
import { api, toPlayRef } from '../api'
import { fmtTime, displayTitle } from '../util'
import { TrackMenu } from './TrackMenu'
import styles from './SearchView.module.css'

interface SearchViewProps {
  query: string
  nowPlayingId: number | null
  isPlaying: boolean
  onBack: () => void
  onOpenAlbum: (album: string) => void
}

export function SearchView({
  query,
  nowPlayingId,
  isPlaying,
  onBack,
  onOpenAlbum,
}: SearchViewProps) {
  const [tracks, setTracks] = useState<Track[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [playingUri, setPlayingUri] = useState<string | null>(null)

  const nowId = nowPlayingId ?? (playingUri ? tracks.find((t) => t.uri === playingUri)?.id ?? null : null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setTracks(await api.library(query))
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [query])

  useEffect(() => {
    load()
  }, [load])

  const play = async (t: Track) => {
    setPlayingUri(t.uri)
    try {
      await api.clearAndPlay([toPlayRef(t)])
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className={styles.wrap}>
      <div className={styles.head}>
        <button className={styles.ghost} onClick={onBack}>
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
            <path d="M14 6l-6 6 6 6" />
          </svg>
          Back
        </button>
        <div className={styles.title}>
          Search results
          <span className={styles.q}>“{query}”</span>
        </div>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {!loading && !error && tracks.length === 0 && (
        <div className={styles.empty}>
          <div className={styles.emptyMark}>♪</div>
          <p>No tracks matched “{query}”.</p>
        </div>
      )}

      {!error && tracks.length > 0 && (
        <ul className={styles.list}>
          {tracks.map((t) => (
            <li
              key={t.id}
              data-track-id={t.id}
              className={
                nowId === t.id
                  ? isPlaying
                    ? styles.rowPlaying
                    : styles.rowPaused
                  : styles.row
              }
              onClick={() => play(t)}
            >
              <span className={styles.tPos}>
                {nowId === t.id ? (
                  <span className={styles.rowEq} aria-hidden>
                    <i />
                    <i />
                    <i />
                  </span>
                ) : (
                  (t.track ?? '') || '·'
                )}
              </span>
              <span className={styles.tTitle}>{displayTitle(t)}</span>
              <span className={styles.tArtist}>{t.artist ?? '—'}</span>
              <span className={styles.tAlbum} onClick={(e) => { e.stopPropagation(); if (t.album) onOpenAlbum(t.album) }}>
                {t.album ?? '—'}
              </span>
              <span className={styles.tTime}>{fmtTime(t.duration)}</span>
              <span className={styles.rowMenu}>
                <TrackMenu tracks={[t]} playing={nowId === t.id && isPlaying} onAdded={() => {}} onError={setError} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
