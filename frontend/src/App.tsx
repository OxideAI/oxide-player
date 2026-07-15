import { useCallback, useEffect, useState } from 'react'
import type { PlayerStatus } from './types'
import { api } from './api'
import { NowPlaying } from './components/NowPlaying'
import { KioskView } from './components/KioskView'
import { LibraryView } from './components/LibraryView'
import { DevicesView } from './components/DevicesView'
import { DspView } from './components/DspView'
import { PlaylistsView } from './components/PlaylistsView'
import styles from './App.module.css'

type Tab = 'library' | 'devices' | 'dsp' | 'playlists'

export function App() {
  const [status, setStatus] = useState<PlayerStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<Tab>('library')
  const [refreshToken, setRefreshToken] = useState(0)
  const [kiosk] = useState(() => window.location.pathname === '/kiosk')

  const loadStatus = useCallback(async () => {
    try {
      const s = await api.status()
      setStatus(s)
      setError(s.error)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    loadStatus()
    const id = setInterval(loadStatus, 1000)
    return () => clearInterval(id)
  }, [loadStatus])

  const refreshLibrary = useCallback(async () => {
    setError(null)
    try {
      await api.refresh()
      setRefreshToken((n) => n + 1)
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [loadStatus])

  const rescanArt = useCallback(async () => {
    setError(null)
    try {
      await api.rescanArt()
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [loadStatus])

  const togglePlay = useCallback(async () => {
    if (!status) return
    try {
      if (status.state === 'playing') await api.pause(true)
      else if (status.state === 'paused') await api.pause(false)
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [status, loadStatus])

  const stop = useCallback(async () => {
    try {
      await api.stop()
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [loadStatus])

  const next = useCallback(async () => {
    try {
      await api.next()
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [loadStatus])

  const prev = useCallback(async () => {
    try {
      await api.prev()
      await loadStatus()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [loadStatus])

  const seek = useCallback(
    async (seconds: number) => {
      try {
        await api.seek(seconds)
        await loadStatus()
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [loadStatus],
  )

  const setVolume = useCallback(
    async (v: number) => {
      try {
        await api.setVolume(v)
        await loadStatus()
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [loadStatus],
  )

  if (kiosk) {
    return (
      <KioskView
        status={status}
        onTogglePlay={togglePlay}
        onStop={stop}
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
      />
    )
  }

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <div className={styles.brand}>Oxide</div>
        <nav className={styles.tabs}>
          {(['library', 'devices', 'dsp', 'playlists'] as Tab[]).map((t) => (
            <button
              key={t}
              className={t === tab ? styles.tabActive : styles.tab}
              onClick={() => setTab(t)}
            >
              {t}
            </button>
          ))}
          <a className={styles.tab} href="/kiosk">
            kiosk
          </a>
        </nav>
      </header>

      {error && (
        <div className={styles.banner} role="alert">
          {error}
        </div>
      )}

      <main className={styles.main}>
        {tab === 'library' && (
          <LibraryView
            refreshToken={refreshToken}
            onPlay={api.play}
            onRefresh={refreshLibrary}
            onRescanArt={rescanArt}
            nowPlayingUri={status?.current_song?.uri ?? null}
            nowPlayingId={status?.current_song?.id ?? null}
            isPlaying={status?.state === 'playing'}
          />
        )}
        {tab === 'devices' && <DevicesView />}
        {tab === 'dsp' && <DspView />}
        {tab === 'playlists' && <PlaylistsView />}
      </main>

      <NowPlaying
        status={status}
        onTogglePlay={togglePlay}
        onStop={stop}
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
        onReloadStatus={loadStatus}
      />
    </div>
  )
}
