import { useEffect, useRef, useState } from 'react'
import type { Track } from '../types'
import { api } from '../api'
import { displayTitle } from '../util'
import { FileInfo } from './FileInfo'
import styles from './TrackMenu.module.css'

interface Props {
  tracks: Track[]
  label?: string
  onPlayNext?: () => void
  onClearAndPlay?: () => void
  onAdded?: (msg: string) => void
  onError?: (msg: string) => void
}

export function TrackMenu({ tracks, label, onPlayNext, onClearAndPlay, onAdded, onError }: Props) {
  const [open, setOpen] = useState(false)
  const [showPlaylists, setShowPlaylists] = useState(false)
  const [playlists, setPlaylists] = useState<string[]>([])
  const [infoTrack, setInfoTrack] = useState<Track | null>(null)
  const wrapRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const close = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [open])

  const run = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn()
      onAdded?.(ok)
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e))
    }
  }

  const openPlaylists = async () => {
    try {
      setPlaylists(await api.playlists())
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e))
    }
    setShowPlaylists(true)
  }

  const addTo = (name: string) => {
    setShowPlaylists(false)
    setOpen(false)
    void run(
      () => api.addToPlaylist(name, tracks.map((t) => trackRef(t))),
      `Added ${tracks.length} to “${name}”`,
    )
  }

  const single = tracks.length === 1 ? tracks[0] : null

  return (
    <div className={styles.trackMenu} ref={wrapRef}>
      <button
        className={styles.trackMenuBtn}
        title={label ?? 'More actions'}
        aria-label={label ?? 'More actions'}
        onClick={(e) => {
          e.stopPropagation()
          setOpen((v) => !v)
        }}
      >
        ⋮
      </button>
      {open && (
        <div className={styles.trackMenuPop} onClick={(e) => e.stopPropagation()}>
          <button
            className={styles.trackMenuItem}
            onClick={() => {
              setOpen(false)
              if (onPlayNext) onPlayNext()
              else
                void run(
                  () => api.playNext(tracks.map(trackRef)),
                  `Queued “${single ? displayTitle(single) : `${tracks.length} tracks`}” next`,
                )
            }}
          >
            ▶ Play next
          </button>
          <button
            className={styles.trackMenuItem}
            onClick={() => {
              setOpen(false)
              if (onClearAndPlay) onClearAndPlay()
              else
                void run(
                  () => api.clearAndPlay(tracks.map(trackRef)),
                  `Playing ${single ? displayTitle(single) : `${tracks.length} tracks`}`,
                )
            }}
          >
            ⟳ Clear and play
          </button>
          <button className={styles.trackMenuItem} onClick={() => void openPlaylists()}>
            ＋ Add to playlist…
          </button>
          {single && (
            <button className={styles.trackMenuItem} onClick={() => setInfoTrack(single)}>
              ⓘ File info
            </button>
          )}
        </div>
      )}
      {showPlaylists && (
        <div className={styles.trackMenuModal} onClick={() => setShowPlaylists(false)}>
          <div className={styles.trackMenuModalBox} onClick={(e) => e.stopPropagation()}>
            <div className={styles.trackMenuModalTitle}>Add to playlist</div>
            <ul className={styles.trackMenuPlaylistList}>
              {playlists.length === 0 && <li className={styles.trackMenuEmpty}>No playlists yet</li>}
              {playlists.map((p) => (
                <li key={p}>
                  <button className={styles.trackMenuItem} onClick={() => addTo(p)}>
                    {p}
                  </button>
                </li>
              ))}
            </ul>
            <button className={styles.trackMenuClose} onClick={() => setShowPlaylists(false)}>
              Close
            </button>
          </div>
        </div>
      )}
      {infoTrack && <FileInfo track={infoTrack} onClose={() => setInfoTrack(null)} />}
    </div>
  )
}

function trackRef(t: Track) {
  return { uri: t.uri, start: t.start_time ?? undefined, end: t.end_time ?? undefined, track_id: t.id }
}
