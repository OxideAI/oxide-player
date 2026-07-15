import { useCallback, useEffect, useState } from 'react'
import type { OutputDevice } from '../types'
import { api } from '../api'
import styles from './DevicesView.module.css'

export function DevicesView() {
  const [devices, setDevices] = useState<OutputDevice[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<number | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setDevices(await api.devices())
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const toggle = async (d: OutputDevice) => {
    setBusy(d.id)
    setError(null)
    try {
      if (d.enabled) await api.disableDevice(d.id)
      else await api.enableDevice(d.id)
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className={styles.wrap}>
      <div>
        <span className={styles.eyebrow}>System</span>
        <h2 className={styles.h}>Output devices</h2>
      </div>
      {error && <div className={styles.error}>{error}</div>}
      {loading && <p className={styles.dim}>loading…</p>}
      {!loading && !error && devices.length === 0 && (
        <p className={styles.dim}>No output devices reported by MPD.</p>
      )}
      <ul className={styles.list}>
        {devices.map((d) => (
          <li key={d.id} className={d.enabled ? styles.rowOn : styles.rowOff}>
            <div>
              <div className={styles.name}>{d.name}</div>
              <div className={styles.id}>#{d.id}</div>
            </div>
            <button
              className={d.enabled ? styles.on : styles.off}
              disabled={busy === d.id}
              onClick={() => toggle(d)}
            >
              {d.enabled ? 'Enabled' : 'Disabled'}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
