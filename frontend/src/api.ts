import type {
  Config,
  DspProfile,
  OutputDevice,
  PlayerStatus,
  QueueItem,
  QueueResponse,
  Track,
} from './types'

async function json<T>(res: Response): Promise<T> {
  const text = await res.text()
  const body = text ? JSON.parse(text) : undefined
  if (!res.ok) {
    const detail = body && typeof body.error === 'string' ? body.error : res.statusText
    throw new Error(detail)
  }
  return body as T
}

export const api = {
  status: () => fetch('/api/status').then((r) => json<PlayerStatus>(r)),

  library: (q?: string) => {
    const qs = q ? `?q=${encodeURIComponent(q)}` : ''
    return fetch(`/api/library${qs}`).then((r) => json<Track[]>(r))
  },
  albums: () => fetch('/api/library/albums').then((r) => json<string[]>(r)),
  artists: () => fetch('/api/library/artists').then((r) => json<string[]>(r)),
  coverUrl: (key: string | number) => `/api/cover/${key}`,
  // Prefer the album-level cover key; fall back to the track id for rows not
  // yet migrated to album-keyed covers.
  coverFor: (hasCover: boolean, coverKey: string | null, id: number) =>
    hasCover ? `/api/cover/${coverKey ?? id}` : null,

  scan: () =>
    fetch('/api/library/scan', { method: 'POST' }).then((r) => json<{ scanned: number }>(r)),
  refresh: () =>
    fetch('/api/library/refresh', { method: 'POST' }).then((r) =>
      json<{ scanned: number }>(r),
    ),
  rescanArt: () =>
    fetch('/api/library/rescan-art', { method: 'POST' }).then((r) =>
      json<{ with_cover: number }>(r),
    ),

  play: (uri?: string, start?: number, end?: number, trackId?: number) =>
    fetch('/api/playback/play', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(
        uri === undefined
          ? {}
          : { uri, start, end, track_id: trackId },
      ),
    }).then((r) => json<unknown>(r)),
  pause: (pause: boolean) =>
    fetch('/api/playback/pause', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pause }),
    }).then((r) => json<unknown>(r)),
  stop: () => fetch('/api/playback/stop', { method: 'POST' }).then((r) => json<unknown>(r)),
  next: () => fetch('/api/playback/next', { method: 'POST' }).then((r) => json<unknown>(r)),
  prev: () => fetch('/api/playback/prev', { method: 'POST' }).then((r) => json<unknown>(r)),
  seek: (seconds: number) =>
    fetch('/api/playback/seek', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ seconds }),
    }).then((r) => json<unknown>(r)),
  setVolume: (volume: number) =>
    fetch('/api/playback/volume', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ volume }),
    }).then((r) => json<unknown>(r)),

  devices: () => fetch('/api/devices').then((r) => json<OutputDevice[]>(r)),
  enableDevice: (id: number) =>
    fetch(`/api/devices/${id}/enable`, { method: 'POST' }).then((r) => json<unknown>(r)),
  disableDevice: (id: number) =>
    fetch(`/api/devices/${id}/disable`, { method: 'POST' }).then((r) => json<unknown>(r)),

  dsp: () => fetch('/api/dsp').then((r) => json<DspProfile[]>(r)),
  setDsp: (profile: DspProfile) =>
    fetch('/api/dsp', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(profile),
    }).then((r) => json<unknown>(r)),

  playlists: () => fetch('/api/playlists').then((r) => json<string[]>(r)),
  playlist: (name: string) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}`).then((r) => json<QueueItem[]>(r)),
  playPlaylist: (name: string) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}/play`, { method: 'POST' }).then((r) =>
      json<unknown>(r),
    ),
  renamePlaylist: (name: string, newName: string) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}/rename`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ new_name: newName }),
    }).then((r) => json<unknown>(r)),
  removeFromPlaylist: (name: string, pos: number) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}/remove`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pos }),
    }).then((r) => json<unknown>(r)),
  deletePlaylist: (name: string) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}`, { method: 'DELETE' }).then((r) =>
      json<unknown>(r),
    ),
  savePlaylist: (name: string) =>
    fetch('/api/playlists', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    }).then((r) => json<unknown>(r)),

  // `tracks` is a single track object or an array of them (whole album),
  // wrapped in `{ tracks }` to match the add-to-playlist envelope.
  playNext: (tracks: unknown) =>
    fetch('/api/playback/play-next', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tracks }),
    }).then((r) => json<unknown>(r)),
  clearAndPlay: (tracks: unknown) =>
    fetch('/api/playback/clear-play', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tracks }),
    }).then((r) => json<unknown>(r)),
  addToPlaylist: (name: string, tracks: unknown) =>
    fetch(`/api/playlists/${encodeURIComponent(name)}/add`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tracks }),
    }).then((r) => json<unknown>(r)),

  queue: () => fetch('/api/queue').then((r) => json<QueueResponse>(r)),
  shuffle: (on: boolean) =>
    fetch('/api/playback/shuffle', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ on }),
    }).then((r) => json<unknown>(r)),
  jump: (pos: number) =>
    fetch('/api/playback/jump', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pos }),
    }).then((r) => json<unknown>(r)),
  remove: (pos: number) =>
    fetch('/api/playback/remove', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pos }),
    }).then((r) => json<unknown>(r)),
  clearQueue: () =>
    fetch('/api/playback/clear-queue', {
      method: 'POST',
    }).then((r) => json<unknown>(r)),

  // —— Settings / config ——
  getConfig: () => fetch('/api/config').then((r) => json<Config>(r)),
  updateConfig: (cfg: Config) =>
    fetch('/api/config', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(cfg),
    }).then((r) => json<Config>(r)),
  addLibraryDir: (path: string) =>
    fetch('/api/config/library-dirs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    }).then((r) => json<{ scanned: number; duplicate?: boolean }>(r)),
  removeLibraryDir: (path: string) =>
    fetch('/api/config/library-dirs', {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path }),
    }).then((r) => json<unknown>(r)),
}
