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

export function RadioView({ nowPlayingUri, isPlaying }: Props) {
  const [stations, setStations] = useState<RadioStation[] | null>(null)
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
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
    if (!trimmedName) {
      setError('Station name is required.')
      return
    }
    if (!URL_RE.test(trimmedUrl)) {
      setError('Station URL must start with http:// or https://')
      return
    }
    setBusy(true)
    try {
      await api.addRadio(trimmedName, trimmedUrl)
      setName('')
      setUrl('')
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
                <button className={styles.iconBtnDanger} onClick={() => remove(s.id)}>
                  Delete
                </button>
              </div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  )
}
