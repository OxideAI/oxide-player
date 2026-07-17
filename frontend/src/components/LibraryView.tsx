import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Track } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle, toPlayRef } from '../util'
import { TrackMenu } from './TrackMenu'
import { Reveal } from './Reveal'
import styles from './LibraryView.module.css'

interface Props {
  refreshToken: number
  onRefresh: () => Promise<void>
  onRescanArt: () => Promise<void>
  nowPlayingUri: string | null
  nowPlayingId: number | null
  isPlaying: boolean
  album: string | null
  onAlbumChange: (album: string | null) => void
}

interface Folder {
  key: string
  name: string
  artist: string | null
  coverKey: string | null
  tracks: Track[]
}

function folderKey(uri: string): string {
  const idx = uri.lastIndexOf('/')
  return idx >= 0 ? uri.slice(0, idx) : ''
}

function trackOrder(a: Track, b: Track): number {
  const ai = a.cue_index ?? a.track ?? 0
  const bi = b.cue_index ?? b.track ?? 0
  if (ai !== bi) return ai - bi
  return displayTitle(a).localeCompare(displayTitle(b))
}

export function LibraryView({
  refreshToken,
  onRefresh,
  onRescanArt,
  nowPlayingUri,
  nowPlayingId,
  isPlaying,
  album,
  onAlbumChange,
}: Props) {
  const [tracks, setTracks] = useState<Track[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [playingUri, setPlayingUri] = useState<string | null>(null)
  const openFolder = album
  const setOpenFolder = onAlbumChange
  const [toast, setToast] = useState<string | null>(null)

  const toastTimer = useRef<number | undefined>(undefined)
  const notify = useCallback((msg: string) => {
    setToast(msg)
    if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current)
    toastTimer.current = window.setTimeout(() => setToast(null), 2500)
  }, [])
  useEffect(() => () => {
    if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current)
  }, [])

  const nowId = nowPlayingId ?? (playingUri ? tracks.find((t) => t.uri === playingUri)?.id ?? null : null)

  useEffect(() => {
    if (nowPlayingUri) setPlayingUri(null)
  }, [nowPlayingUri])

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setTracks(await api.library())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load, refreshToken])

  const folders = useMemo(() => {
    const map = new Map<string, Folder>()
    for (const t of tracks) {
      const key = folderKey(t.uri)
      let f = map.get(key)
      if (!f) {
        f = {
          key,
          name: t.album || key.split('/').pop() || key || 'Unknown',
          artist: t.artist ?? null,
          coverKey: null,
          tracks: [],
        }
        map.set(key, f)
      }
      f.tracks.push(t)
      if (f.coverKey === null && t.has_cover && t.cover_key) f.coverKey = t.cover_key
    }
    const arr = [...map.values()]
    arr.forEach((f) => f.tracks.sort(trackOrder))
    arr.sort((a, b) => a.name.localeCompare(b.name))
    return arr
  }, [tracks])

  const filteredFolders = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return folders
    return folders.filter((f) =>
      [f.name, f.artist, f.key]
        .filter(Boolean)
        .some((v) => v!.toLowerCase().includes(q)) ||
      f.tracks.some((t) =>
        [t.title, t.artist].filter(Boolean).some((v) => v!.toLowerCase().includes(q)),
      ),
    )
  }, [folders, query])

  useEffect(() => {
    if (openFolder !== null && folders.length > 0 && !folders.some((f) => f.key === openFolder))
      setOpenFolder(null)
  }, [folders, openFolder])

  const current = useMemo(
    () => (openFolder !== null ? folders.find((f) => f.key === openFolder) ?? null : null),
    [folders, openFolder],
  )

  const play = async (t: Track) => {
    setPlayingUri(t.uri)
    try {
      await api.clearAndPlay([toPlayRef(t)])
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const rescanArt = async () => {
    setError(null)
    try {
      await onRescanArt()
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const count =
    loading
      ? 'loading…'
      : current
        ? `${current.tracks.length} tracks`
        : `${filteredFolders.length} / ${folders.length} albums`

  return (
    <div className={styles.wrap}>
      {toast && <div className={styles.toast}>{toast}</div>}

      <div className={styles.toolbar}>
        {current && (
          <button className={styles.ghost} onClick={() => setOpenFolder(null)}>
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
              <path d="M14 6l-6 6 6 6" />
            </svg>
            Folders
          </button>
        )}
        <div className={styles.searchShell}>
          <svg className={styles.searchIcon} viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
            <circle cx="11" cy="11" r="6.5" />
            <path d="M20 20l-3.6-3.6" />
          </svg>
          <input
            className={styles.search}
            placeholder={current ? 'Search this folder…' : 'Search albums, artists…'}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className={styles.pill} onClick={onRefresh}>
          Refresh
        </button>
        <button className={styles.pill} onClick={rescanArt}>
          Rescan art
        </button>
        <span className={styles.count}>{count}</span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {!loading && !error && tracks.length === 0 && (
        <div className={styles.empty}>
          <div className={styles.emptyMark}>♪</div>
          <p>The library is empty.</p>
          <button className={styles.pill} onClick={onRefresh}>
            Scan music folder
          </button>
        </div>
      )}

      {!loading && !error && tracks.length > 0 && !current && (
        <div className={styles.grid}>
          {filteredFolders.map((f, i) => (
            <Reveal key={f.key} delay={Math.min(i * 35, 350)}>
              <button className={styles.tile} onClick={() => setOpenFolder(f.key)}>
                <span className={styles.tileShell}>
                  <span className={styles.tileCore}>
                    {f.coverKey !== null ? (
                      <img src={api.coverUrl(f.coverKey)} alt="" loading="lazy" />
                    ) : (
                      <span className={styles.tilePh}>♪</span>
                    )}
                  </span>
                </span>
                <span className={styles.tileName} title={f.name}>
                  {f.name}
                </span>
                <span className={styles.tileArtist} title={f.artist ?? ''}>
                  {f.artist ?? '—'}
                </span>
              </button>
            </Reveal>
          ))}
        </div>
      )}

      {!loading && !error && current && (
        <div className={styles.album}>
          <div className={styles.albumHead}>
            <span className={styles.albumShell}>
              <span className={styles.albumCore}>
                {current.coverKey !== null ? (
                  <img src={api.coverUrl(current.coverKey)} alt="" />
                ) : (
                  <span className={styles.tilePh}>♪</span>
                )}
              </span>
            </span>
            <div className={styles.albumInfo}>
              <span className={styles.eyebrow}>Album</span>
              <div className={styles.albumTitle}>{current.name}</div>
              <div className={styles.albumArtist}>{current.artist ?? '—'}</div>
              <div className={styles.albumMeta}>{current.tracks.length} tracks</div>
            </div>
            <div className={styles.albumActions}>
              <TrackMenu tracks={current.tracks} label="Album actions" onAdded={notify} onError={setError} />
            </div>
          </div>

          <ul className={styles.list}>
            {current.tracks
              .filter((t) => {
                const q = query.trim().toLowerCase()
                if (!q) return true
                return [t.title, t.artist].filter(Boolean).some((v) => v!.toLowerCase().includes(q))
              })
              .map((t) => (
                <li
                  key={t.id}
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
                  <span className={styles.tTime}>{fmtTime(t.duration)}</span>
                  <span className={styles.rowMenu}>
                    <TrackMenu tracks={[t]} playing={nowId === t.id && isPlaying} onAdded={notify} onError={setError} />
                  </span>
                </li>
              ))}
          </ul>
        </div>
      )}
    </div>
  )
}
