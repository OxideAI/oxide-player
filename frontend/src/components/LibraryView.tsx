import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Track } from '../types'
import { api } from '../api'
import { fmtTime, displayTitle } from '../util'
import { TrackMenu } from './TrackMenu'
import styles from './LibraryView.module.css'

interface Props {
  refreshToken: number
  onPlay: (uri: string, start?: number, end?: number, trackId?: number) => Promise<unknown>
  onRefresh: () => Promise<void>
  onRescanArt: () => Promise<void>
  nowPlayingUri: string | null
  nowPlayingId: number | null
  isPlaying: boolean
}

interface Folder {
  key: string
  name: string
  artist: string | null
  coverId: number | null
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
  onPlay,
  onRefresh,
  onRescanArt,
  nowPlayingUri,
  nowPlayingId,
  isPlaying,
}: Props) {
  const [tracks, setTracks] = useState<Track[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [playingUri, setPlayingUri] = useState<string | null>(null)
  const [openFolder, setOpenFolder] = useState<string | null>(null)
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

  // Highlight from the backend's actual now-playing state so the row is
  // correct even when playback started elsewhere (other view, kiosk, etc.).
  // Match on id, not uri: every CUE track in a file shares the same uri, so
  // uri-only matching would light up the whole album.
  const nowId = nowPlayingId ?? (playingUri ? tracks.find((t) => t.uri === playingUri)?.id ?? null : null)

  // Once the backend confirms playback (covers clicks made elsewhere too),
  // drop the local click feedback so it can't linger in another folder.
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
          coverId: null,
          tracks: [],
        }
        map.set(key, f)
      }
      f.tracks.push(t)
      if (f.coverId === null && t.has_cover) f.coverId = t.id
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

  const current = useMemo(
    () => (openFolder !== null ? folders.find((f) => f.key === openFolder) ?? null : null),
    [folders, openFolder],
  )

  const play = async (t: Track) => {
    setPlayingUri(t.uri)
    try {
      await onPlay(t.uri, t.start_time ?? undefined, t.end_time ?? undefined, t.id)
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

  return (
    <div className={styles.wrap}>
      {toast && <div className={styles.toast}>{toast}</div>}
      <div className={styles.toolbar}>
        {current && (
          <button className={styles.refresh} onClick={() => setOpenFolder(null)}>
            ← Folders
          </button>
        )}
        <input
          className={styles.search}
          placeholder={current ? 'Search this folder…' : 'Search albums, artists…'}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button className={styles.refresh} onClick={onRefresh}>
          ↻ Refresh library
        </button>
        <button className={styles.refresh} onClick={rescanArt}>
          🖼 Rescan art
        </button>
        <span className={styles.count}>
          {loading
            ? 'loading…'
            : current
              ? `${current.tracks.length} tracks`
              : `${filteredFolders.length} / ${folders.length} albums`}
        </span>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {!loading && !error && tracks.length === 0 && (
        <div className={styles.empty}>
          <p>The library is empty.</p>
          <button className={styles.refresh} onClick={onRefresh}>
            ↻ Scan music folder
          </button>
        </div>
      )}

      {!loading && !error && tracks.length > 0 && !current && (
        <div className={styles.grid}>
          {filteredFolders.map((f) => (
            <button key={f.key} className={styles.tile} onClick={() => setOpenFolder(f.key)}>
              <div className={styles.tileArt}>
                {f.coverId !== null ? (
                  <img src={api.coverUrl(f.coverId)} alt="" loading="lazy" />
                ) : (
                  <span className={styles.tileArtPh}>♪</span>
                )}
              </div>
              <div className={styles.tileName} title={f.name}>
                {f.name}
              </div>
              <div className={styles.tileArtist} title={f.artist ?? ''}>
                {f.artist ?? '—'}
              </div>
            </button>
          ))}
        </div>
      )}

      {!loading && !error && current && (
        <>
          <div className={styles.albumHead}>
            <div className={styles.albumArt}>
              {current.coverId !== null ? (
                <img src={api.coverUrl(current.coverId)} alt="" />
              ) : (
                <span className={styles.tileArtPh}>♪</span>
              )}
            </div>
            <div className={styles.albumInfo}>
              <div className={styles.albumTitle}>{current.name}</div>
              <div className={styles.albumArtist}>{current.artist ?? '—'}</div>
              <div className={styles.albumMeta}>{current.tracks.length} tracks</div>
            </div>
            <TrackMenu
              tracks={current.tracks}
              label="Album actions"
              onAdded={notify}
              onError={setError}
            />
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
                  <span className={styles.tTitle}>{displayTitle(t)}</span>
                  <span className={styles.tArtist}>{t.artist ?? '—'}</span>
                  <span className={styles.tTime}>{fmtTime(t.duration)}</span>
                  <TrackMenu tracks={[t]} onAdded={notify} onError={setError} />
                </li>
              ))}
          </ul>
        </>
      )}
    </div>
  )
}
