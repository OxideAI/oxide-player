import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from './api'
import { usePlayerStatus } from './ws'
import { NowPlaying } from './components/NowPlaying'
import { KioskView } from './components/KioskView'
import { LibraryView } from './components/LibraryView'
import { ConfigView } from './components/ConfigView'
import { PlaylistsView } from './components/PlaylistsView'
import { Reveal } from './components/Reveal'
import { InstallPrompt } from './components/InstallPrompt'
import { OfflineBanner } from './components/OfflineBanner'
import styles from './App.module.css'

type Tab = 'library' | 'playlists' | 'settings'

const TABS: { id: Tab; label: string }[] = [
  { id: 'library', label: 'Library' },
  { id: 'playlists', label: 'Playlists' },
  { id: 'settings', label: 'Settings' },
]

interface Route {
  tab: Tab
  album: string | null
}

function parsePath(): Route {
  const raw = window.location.pathname.replace(/^\/+/, '')
  const parts = raw.split('/').filter(Boolean)
  const head = parts[0] as Tab | undefined
  if (head && TABS.some((t) => t.id === head)) {
    if (head === 'library' && parts[1]) {
      try {
        return { tab: head, album: decodeURIComponent(parts[1]) }
      } catch {
        return { tab: head, album: parts[1] }
      }
    }
    return { tab: head, album: null }
  }
  return { tab: 'library', album: null }
}

function buildPath(route: Route): string {
  if (route.tab === 'library' && route.album) return `/library/${encodeURIComponent(route.album)}`
  return `/${route.tab}`
}

export function App() {
  const [error, setError] = useState<string | null>(null)
  const [route, setRoute] = useState<Route>(() => parsePath())
  const tab = route.tab
  const [refreshToken, setRefreshToken] = useState(0)
  const [navOpen, setNavOpen] = useState(false)
  const [kiosk] = useState(() => window.location.pathname === '/kiosk')
  const overlayRef = useRef<HTMLDivElement>(null)

  // Live player status + queue, pushed over a single WebSocket (issue #3).
  const { status, queue, connected } = usePlayerStatus()
  const connectionError =
    status === null && !connected ? 'Connecting to player…' : null
  const banner = error ?? connectionError

  useEffect(() => {
    if (overlayRef.current) overlayRef.current.inert = !navOpen
  }, [navOpen])

  useEffect(() => {
    const onPop = () => setRoute(parsePath())
    window.addEventListener('popstate', onPop)
    return () => window.removeEventListener('popstate', onPop)
  }, [])

  const refreshLibrary = useCallback(async () => {
    setError(null)
    try {
      await api.refresh()
      setRefreshToken((n) => n + 1)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const rescanArt = useCallback(async () => {
    setError(null)
    try {
      await api.rescanArt()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const togglePlay = useCallback(async () => {
    if (!status) return
    try {
      if (status.state === 'playing') await api.pause(true)
      else if (status.state === 'paused') await api.pause(false)
      else await api.play()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [status])

  const next = useCallback(async () => {
    try {
      await api.next()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const prev = useCallback(async () => {
    try {
      await api.prev()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  // Optimistic local update so the scrubber/volume feel instant; the WS push
  // from the server reconciles within a frame.
  const seek = useCallback(
    async (seconds: number) => {
      try {
        await api.seek(seconds)
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [],
  )

  const setVolume = useCallback(
    async (v: number) => {
      try {
        await api.setVolume(v)
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e))
      }
    },
    [],
  )

  if (kiosk) {
    return (
      <KioskView
        status={status}
        queue={queue}
        onTogglePlay={togglePlay}
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
      />
    )
  }

  const go = (t: Tab) => {
    setRoute((r) => {
      const next = { tab: t, album: t === 'library' ? r.album : null }
      window.history.pushState(null, '', buildPath(next))
      return next
    })
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

      <div
        ref={overlayRef}
        className={`${styles.overlay} ${navOpen ? styles.overlayOn : ''}`}
        aria-hidden={!navOpen}
        onKeyDown={(e) => {
          if (e.key === 'Escape') setNavOpen(false)
        }}
      >
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

      {banner && (
        <div className={styles.banner} role="alert">
          <span className={styles.bannerDot} />
          {banner}
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
            album={route.album}
            onAlbumChange={(album) => {
              setRoute((r) => {
                const next = { tab: r.tab, album }
                window.history.pushState(null, '', buildPath(next))
                return next
              })
            }}
          />
        )}
        {tab === 'playlists' && <PlaylistsView />}
        {tab === 'settings' && <ConfigView />}
      </main>

      <NowPlaying
        status={status}
        queue={queue}
        onTogglePlay={togglePlay}
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
      />

      <InstallPrompt />
      <OfflineBanner />
    </div>
  )
}
