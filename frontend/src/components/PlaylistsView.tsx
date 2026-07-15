import { useCallback, useEffect, useState } from 'react'
import { api } from '../api'
import type { QueueItem } from '../types'
import styles from './PlaylistsView.module.css'

function fmt(d: number | null): string {
  if (d == null) return '--:--'
  const m = Math.floor(d / 60)
  const s = Math.floor(d % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

export function PlaylistsView() {
  const [names, setNames] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [name, setName] = useState('')

  const [open, setOpen] = useState<string | null>(null)
  const [tracks, setTracks] = useState<QueueItem[]>([])
  const [tracksLoading, setTracksLoading] = useState(false)
  const [tracksError, setTracksError] = useState<string | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setNames(await api.playlists())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const save = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setError(null)
    try {
      await api.savePlaylist(trimmed)
      setName('')
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const openPlaylist = async (n: string) => {
    if (open === n) {
      setOpen(null)
      setTracks([])
      return
    }
    setOpen(n)
    setTracks([])
    setTracksError(null)
    setTracksLoading(true)
    try {
      setTracks(await api.playlist(n))
    } catch (e) {
      setTracksError(e instanceof Error ? e.message : String(e))
    } finally {
      setTracksLoading(false)
    }
  }

  const play = async (n: string) => {
    setError(null)
    try {
      await api.playPlaylist(n)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const removeTrack = async (n: string, pos: number) => {
    setTracksError(null)
    try {
      await api.removeFromPlaylist(n, pos)
      setTracks(await api.playlist(n))
    } catch (e) {
      setTracksError(e instanceof Error ? e.message : String(e))
    }
  }

  const del = async (n: string) => {
    setError(null)
    try {
      await api.deletePlaylist(n)
      if (open === n) {
        setOpen(null)
        setTracks([])
      }
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  const startRename = (n: string) => {
    setRenaming(n)
    setRenameValue(n)
  }

  const commitRename = async (n: string) => {
    const target = renameValue.trim()
    setRenaming(null)
    if (!target || target === n) return
    setError(null)
    try {
      await api.renamePlaylist(n, target)
      if (open === n) {
        setOpen(target)
        setTracks(await api.playlist(target))
      }
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Collections</span>
        <h2 className={styles.h}>Playlists</h2>
      </div>
      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.saveRow}>
        <input
          className={styles.input}
          placeholder="New playlist name…"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && save()}
        />
        <button className={styles.save} onClick={save} disabled={!name.trim()}>
          Save current queue
        </button>
      </div>

      {loading && <p className={styles.dim}>loading…</p>}
      {!loading && !error && names.length === 0 && (
        <p className={styles.dim}>No playlists yet.</p>
      )}
      <ul className={styles.list}>
        {names.map((n) => (
          <li key={n} className={styles.group}>
            <div className={styles.row}>
              <button className={styles.nameBtn} onClick={() => openPlaylist(n)}>
                {n}
              </button>
              <div className={styles.actions}>
                <button className={styles.iconBtn} onClick={() => play(n)}>
                  Play
                </button>
                <button className={styles.iconBtn} onClick={() => startRename(n)}>
                  Rename
                </button>
                <button className={styles.iconBtnDanger} onClick={() => del(n)}>
                  Delete
                </button>
              </div>
            </div>

            {renaming === n && (
              <div className={styles.renameRow}>
                <input
                  className={styles.input}
                  autoFocus
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') commitRename(n)
                    if (e.key === 'Escape') setRenaming(null)
                  }}
                />
                <button className={styles.save} onClick={() => commitRename(n)}>
                  Rename
                </button>
              </div>
            )}

            {open === n && (
              <div className={styles.tracks}>
                {tracksLoading && <p className={styles.dim}>loading…</p>}
                {tracksError && <div className={styles.error}>{tracksError}</div>}
                {!tracksLoading && !tracksError && tracks.length === 0 && (
                  <p className={styles.dim}>Empty playlist.</p>
                )}
                {tracks.map((t) => (
                  <div key={t.pos} className={styles.trackRow}>
                    <span className={styles.trackTitle}>{t.title ?? t.uri}</span>
                    <span className={styles.trackMeta}>
                      {[t.artist, t.album].filter(Boolean).join(' · ')}
                    </span>
                    <span className={styles.trackDur}>{fmt(t.duration)}</span>
                    <button
                      className={styles.iconBtnDanger}
                      title="Remove from playlist"
                      onClick={() => removeTrack(n, t.pos)}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
