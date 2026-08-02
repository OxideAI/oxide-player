import { useCallback, useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { api } from '../api'
import type { RadioStation } from '../types'
import styles from './RadioView.module.css'

interface Props {
  nowPlayingUri: string | null
  isPlaying: boolean
}

const URL_RE = /^https?:\/\//i

function normalizeArtwork(value: string): string | null {
  const artwork = value.trim()
  return artwork || null
}

export function RadioView({ nowPlayingUri, isPlaying }: Props) {
  const [stations, setStations] = useState<RadioStation[] | null>(null)
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
  const [artwork, setArtwork] = useState('')
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [editArtwork, setEditArtwork] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    setError(null)
    try {
      setStations(await api.listRadio())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const add = async (e: FormEvent) => {
    e.preventDefault()
    setError(null)
    const trimmedName = name.trim()
    const trimmedUrl = url.trim()
    const trimmedArtwork = normalizeArtwork(artwork)
    if (!trimmedName) {
      setError('Station name is required.')
      return
    }
    if (!URL_RE.test(trimmedUrl)) {
      setError('Station URL must start with http:// or https://')
      return
    }
    if (trimmedArtwork && !URL_RE.test(trimmedArtwork)) {
      setError('Artwork URL must start with http:// or https://')
      return
    }
    setBusy(true)
    try {
      await api.addRadio(trimmedName, trimmedUrl, trimmedArtwork)
      setName('')
      setUrl('')
      setArtwork('')
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const beginEdit = (station: RadioStation) => {
    setError(null)
    setEditingId(station.id)
    setEditName(station.name)
    setEditArtwork(station.artwork ?? '')
  }

  const cancelEdit = () => {
    setEditingId(null)
    setEditName('')
    setEditArtwork('')
  }

  const saveEdit = async (e: FormEvent, id: string) => {
    e.preventDefault()
    setError(null)
    const trimmedName = editName.trim()
    const trimmedArtwork = normalizeArtwork(editArtwork)
    if (!trimmedName) {
      setError('Station name is required.')
      return
    }
    if (trimmedArtwork && !URL_RE.test(trimmedArtwork)) {
      setError('Artwork URL must start with http:// or https://')
      return
    }
    setBusy(true)
    try {
      await api.updateRadio(id, trimmedName, trimmedArtwork)
      cancelEdit()
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const play = async (id: string) => {
    setError(null)
    try {
      await api.playRadio(id)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const remove = async (id: string) => {
    setError(null)
    try {
      await api.deleteRadio(id)
      if (editingId === id) cancelEdit()
      await load()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const playingUrl = isPlaying ? nowPlayingUri : null

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>Streams</span>
        <h2 className={styles.h}>Radio</h2>
      </div>
      {error && <div className={styles.error}>{error}</div>}

      <form className={styles.addRow} onSubmit={add}>
        <input
          className={styles.input}
          placeholder="Station name…"
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label="Station name"
        />
        <input
          className={styles.input}
          placeholder="Stream URL (https://…)"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          aria-label="Stream URL"
        />
        <input
          className={styles.input}
          placeholder="Artwork URL (optional)"
          value={artwork}
          onChange={(e) => setArtwork(e.target.value)}
          aria-label="Artwork URL"
        />
        <button className={styles.save} disabled={busy || (!name.trim() && !url.trim())}>
          Add station
        </button>
      </form>

      {stations === null && <p className={styles.dim}>loading…</p>}
      {stations !== null && stations.length === 0 && (
        <p className={styles.dim}>No stations yet — add one above.</p>
      )}
      <ul className={styles.list}>
        {stations?.map((s) => (
          <li key={s.id} className={styles.group}>
            <div
              className={`${styles.row} ${s.url === playingUrl ? styles.rowPlaying : ''}`}
            >
              <div className={styles.art} aria-hidden>
                <span className={styles.artFallback}>FM</span>
                {s.artwork && <img className={styles.artImage} src={s.artwork} alt="" />}
              </div>
              <div className={styles.ident}>
                <span className={styles.liveDot} aria-hidden hidden={s.url !== playingUrl} />
                <button
                  className={styles.nameBtn}
                  onClick={() => play(s.id)}
                  title={`Play ${s.name}`}
                >
                  {s.name}
                </button>
                <span className={styles.url}>{s.url}</span>
              </div>
              <div className={styles.actions}>
                {s.homepage && (
                  <a
                    className={styles.iconBtn}
                    href={s.homepage}
                    target="_blank"
                    rel="noreferrer"
                    title="Station website"
                  >
                    Site
                  </a>
                )}
                <button
                  className={styles.iconBtn}
                  onClick={() => play(s.id)}
                  disabled={s.url === playingUrl}
                >
                  {s.url === playingUrl ? 'Playing' : 'Play'}
                </button>
                <button className={styles.iconBtn} onClick={() => beginEdit(s)}>
                  Edit
                </button>
                <button className={styles.iconBtnDanger} onClick={() => remove(s.id)}>
                  Delete
                </button>
              </div>
            </div>
            {editingId === s.id && (
              <form className={styles.editPanel} onSubmit={(e) => saveEdit(e, s.id)}>
                <label className={styles.field}>
                  <span>Station name</span>
                  <input
                    className={styles.input}
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    aria-label={`Edit station name for ${s.name}`}
                    autoFocus
                  />
                </label>
                <label className={styles.field}>
                  <span>Artwork URL</span>
                  <input
                    className={styles.input}
                    placeholder="https://…"
                    value={editArtwork}
                    onChange={(e) => setEditArtwork(e.target.value)}
                    aria-label={`Artwork URL for ${s.name}`}
                  />
                </label>
                <div className={styles.editActions}>
                  <button className={styles.save} disabled={busy}>
                    Save changes
                  </button>
                  <button className={styles.iconBtn} type="button" onClick={cancelEdit}>
                    Cancel
                  </button>
                </div>
              </form>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
