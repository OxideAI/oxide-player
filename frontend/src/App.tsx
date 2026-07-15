import { useCallback, useEffect, useState } from 'react'
import type { PlayerStatus } from './types'
import { api } from './api'
import { NowPlaying } from './components/NowPlaying'
import { KioskView } from './components/KioskView'
import { LibraryView } from './components/LibraryView'
import { DevicesView } from './components/DevicesView'
import { DspView } from './components/DspView'
import { PlaylistsView } from './components/PlaylistsView'
import { Reveal } from './components/Reveal'
import styles from './App.module.css'

type Tab = 'library' | 'devices' | 'dsp' | 'playlists'

const TABS: { id: Tab; label: string }[] = [
  { id: 'library', label: 'Library' },
  { id: 'playlists', label: 'Playlists' },
  { id: 'devices', label: 'Devices' },
  { id: 'dsp', label: 'DSP' },
]

export function App() {
  const [status, setStatus] = useState<PlayerStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<Tab>('library')
  const [refreshToken, setRefreshToken] = useState(0)
  const [navOpen, setNavOpen] = useState(false)
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
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
      />
    )
  }

  const go = (t: Tab) => {
    setTab(t)
    setNavOpen(false)
  }

  return (
    <div className={styles.app}>
      <header className={styles.nav}>
        <button className={styles.brand} onClick={() => go('library')} aria-label="Oxide home">
          <span className={styles.brandMark} aria-hidden>
            <span className={styles.brandCore} />
          </span>
          <span className={styles.brandWord}>Oxide</span>
        </button>

        <nav className={styles.tabs}>
          {TABS.map((t) => (
            <button
              key={t.id}
              className={t.id === tab ? styles.tabActive : styles.tab}
              onClick={() => go(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>

        <button
          className={`${styles.burger} ${navOpen ? styles.burgerOpen : ''}`}
          onClick={() => setNavOpen((o) => !o)}
          aria-label={navOpen ? 'Close menu' : 'Open menu'}
          aria-expanded={navOpen}
        >
          <span />
          <span />
        </button>
      </header>

      <div className={`${styles.scrim} ${navOpen ? styles.scrimOn : ''}`} onClick={() => setNavOpen(false)} />

      <div className={`${styles.overlay} ${navOpen ? styles.overlayOn : ''}`} aria-hidden={!navOpen}>
        <div className={styles.overlayInner}>
          {TABS.map((t, i) => (
            <Reveal
              key={t.id}
              as="button"
              delay={90 + i * 70}
              className={`${styles.bigLink} ${t.id === tab ? styles.bigLinkActive : ''}`}
              onClick={() => go(t.id)}
            >
              <span className={styles.bigIndex}>0{i + 1}</span>
              {t.label}
            </Reveal>
          ))}
        </div>
      </div>

      {error && (
        <div className={styles.banner} role="alert">
          <span className={styles.bannerDot} />
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
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
        onReloadStatus={loadStatus}
      />
    </div>
  )
}
