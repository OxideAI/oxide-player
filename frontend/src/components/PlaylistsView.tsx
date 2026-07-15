import { useCallback, useEffect, useState } from 'react'
import { api } from '../api'
import styles from './PlaylistsView.module.css'

export function PlaylistsView() {
  const [names, setNames] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [name, setName] = useState('')

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

  return (
    <div className={styles.wrap}>
      <h2 className={styles.h}>Playlists</h2>
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
          <li key={n} className={styles.row}>
            {n}
          </li>
        ))}
      </ul>
    </div>
  )
}
