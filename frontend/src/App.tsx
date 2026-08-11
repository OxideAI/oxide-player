import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from './api'
import { usePlayerStatus } from './ws'
import { removeTrackFromLibraryCache } from './libraryCache'
import { NowPlaying } from './components/NowPlaying'
import { KioskView } from './components/KioskView'
import { LibraryView } from './components/LibraryView'
import { RadioView } from './components/RadioView'
import { ConfigView } from './components/ConfigView'
import { PlaylistsView } from './components/PlaylistsView'
import { Reveal } from './components/Reveal'
import { InstallPrompt } from './components/InstallPrompt'
import { OfflineBanner } from './components/OfflineBanner'
import { useKeyboardShortcuts } from './components/useKeyboardShortcuts'
import { ShortcutToast } from './components/ShortcutToast'
import { ShortcutHelp } from './components/ShortcutHelp'
import { SearchBar } from './components/SearchBar'
import { SearchView } from './components/SearchView'
import styles from './App.module.css'

type Tab = 'library' | 'playlists' | 'settings' | 'radio'

const TABS: { id: Tab; label: string }[] = [
  { id: 'library', label: 'Library' },
  { id: 'playlists', label: 'Playlists' },
  { id: 'radio', label: 'Radio' },
  { id: 'settings', label: 'Settings' },
]

interface Route {
  tab: Tab
  album: string | null
}

export function parsePath(): Route {
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

export function buildPath(route: Route): string {
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
  const { status, queue, notice, connected, error: statusError, lastSnapshotAt } = usePlayerStatus()
  const connectionError =
    statusError ?? (status === null && !connected ? 'Connecting to player…' : null)
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

  const prevVolume = useRef<number>(status?.volume && status.volume > 0 ? status.volume : 80)
  useEffect(() => {
    if (status?.volume && status.volume > 0) prevVolume.current = status.volume
  }, [status?.volume])
  const toggleMute = useCallback(() => {
    if (status?.volume === null || status?.volume === undefined) return
    const cur = status.volume
    if (cur > 0) {
      prevVolume.current = cur
      void setVolume(0)
    } else {
      void setVolume(prevVolume.current || 80)
    }
  }, [status, setVolume])

  const toggleKiosk = useCallback(() => {
    if (window.location.pathname === '/kiosk') window.location.pathname = '/'
    else window.location.pathname = '/kiosk'
  }, [])

  const toggleShuffle = useCallback(async () => {
    if (!status) return
    try {
      await api.shuffle(!status.random)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [status])

  const [helpOpen, setHelpOpen] = useState(false)
  const [toast, setToast] = useState<string | null>(null)
  const [searchOpen, setSearchOpen] = useState(false)
  const [searchQuery, setSearchQuery] = useState<string | null>(null)
  useEffect(() => {
    if (!notice) return
    const reason = notice.reason === 'missing' ? 'missing' : 'permanently unplayable'
    setToast(`Skipped ${notice.label}: ${reason}.`)
    void removeTrackFromLibraryCache(notice.track_id)
    setRefreshToken((n) => n + 1)
  }, [notice?.id])

  const openSearch = useCallback((q: string) => {
    setSearchQuery(q)
    setSearchOpen(false)
  }, [])

  useKeyboardShortcuts({
    status,
    onTogglePlay: togglePlay,
    onNext: next,
    onPrev: prev,
    onSeek: seek,
    onVolume: setVolume,
    onToggleKiosk: toggleKiosk,
    onToggleShuffle: toggleShuffle,
    onToggleMute: toggleMute,
    onHelp: () => setHelpOpen((o) => !o),
    onFeedback: setToast,
    onSearch: () => setSearchOpen(true),
  })

  const openAlbum = useCallback((album: string) => {
    setRoute({ tab: 'library', album })
    window.history.pushState(null, '', buildPath({ tab: 'library', album }))
    setNavOpen(false)
  }, [])

  if (kiosk) {
    return (
      <>
        <KioskView
          status={status}
          queue={queue}
          onTogglePlay={togglePlay}
          onNext={next}
          onPrev={prev}
          onSeek={seek}
          onVolume={setVolume}
          onOpenAlbum={openAlbum}
        />
        <ShortcutToast text={toast} onClear={() => setToast(null)} />
      </>
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
        {tab === 'playlists' && <PlaylistsView onOpenAlbum={openAlbum} />}
        {tab === 'radio' && (
          <RadioView
            nowPlayingUri={status?.current_song?.uri ?? null}
            isPlaying={status?.state === 'playing'}
          />
        )}
        {tab === 'settings' && (
          <ConfigView
            status={status}
            statusConnected={connected}
            statusError={statusError}
            statusLastUpdatedAt={lastSnapshotAt}
          />
        )}
        {searchQuery && (
          <SearchView
            query={searchQuery}
            nowPlayingId={status?.current_song?.id ?? null}
            isPlaying={status?.state === 'playing'}
            onBack={() => setSearchQuery(null)}
            onOpenAlbum={openAlbum}
          />
        )}
      </main>

      <NowPlaying
        status={status}
        queue={queue}
        onTogglePlay={togglePlay}
        onNext={next}
        onPrev={prev}
        onSeek={seek}
        onVolume={setVolume}
        onOpenAlbum={openAlbum}
      />

      <InstallPrompt />
      <OfflineBanner />
      <ShortcutToast text={toast} onClear={() => setToast(null)} />
      <ShortcutHelp open={helpOpen} onClose={() => setHelpOpen(false)} />
      <SearchBar open={searchOpen} onClose={() => setSearchOpen(false)} onSearch={openSearch} />
    </div>
  )
}
