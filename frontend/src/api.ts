import type {
  AlbumSources,
  BtDevice,
  Config,
  DeviceConfig,
  DspProfile,
  InputStatusResponse,
  OutputDevice,
  PlayerStatus,
  QueueItem,
  QueueResponse,
  RadioStation,
  ScanResultsResponse,
  Track,
} from './types'

/**
 * The wire envelope sent to the playback endpoints (play-next, clear-play,
 * playlist add). Maps a library `Track` to the shape the backend `TrackRef`
 * deserializer expects: `{ uri, start, end, track_id }`. This is the *request*
 * shape and is distinct from the `TrackRef` *response* model in `types.ts`.
 */
export interface PlayRef {
  uri: string
  start: number | undefined
  end: number | undefined
  track_id: number
}

export function toPlayRef(t: Track): PlayRef {
  return {
    uri: t.uri,
    start: t.start_time ?? undefined,
    end: t.end_time ?? undefined,
    track_id: t.id,
  }
}

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
  albumSources: () =>
    fetch('/api/library/albums/sources').then((r) => json<AlbumSources[]>(r)),
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
  enableDeviceDsp: (id: number) =>
    fetch(`/api/devices/${id}/dsp/enable`, { method: 'POST' }).then((r) => json<unknown>(r)),
  disableDeviceDsp: (id: number) =>
    fetch(`/api/devices/${id}/dsp/disable`, { method: 'POST' }).then((r) => json<unknown>(r)),

  // —— Device config fragments ——
  deviceConfigs: () => fetch('/api/devices/configs').then((r) => json<DeviceConfig[]>(r)),
  createDeviceConfig: (cfg: Partial<DeviceConfig>) => {
    // Map frontend output_type → backend 'type' field name
    const { output_type, ...rest } = cfg
    const body = { type: output_type, ...rest }
    return fetch('/api/devices/configs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }).then((r) => json<DeviceConfig>(r))
  },
  updateDeviceConfig: (name: string, cfg: Partial<DeviceConfig>) => {
    const { output_type, ...rest } = cfg
    const body = output_type !== undefined ? { type: output_type, ...rest } : rest
    return fetch(`/api/devices/configs/${encodeURIComponent(name)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    }).then((r) => json<DeviceConfig>(r))
  },
  deleteDeviceConfig: (name: string) =>
    fetch(`/api/devices/configs/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }).then((r) => json<unknown>(r)),
  restartMpd: () =>
    fetch('/api/devices/restart-mpd', { method: 'POST' }).then((r) => json<{ status: string }>(r)),

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

  // —— Radio stations ——
  listRadio: () => fetch('/api/radio').then((r) => json<RadioStation[]>(r)),
  addRadio: (name: string, url: string, artwork: string | null = null) =>
    fetch('/api/radio', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, url, artwork }),
    }).then((r) => json<RadioStation>(r)),
  updateRadio: (id: string, name: string, artwork: string | null) =>
    fetch(`/api/radio/${encodeURIComponent(id)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, artwork }),
    }).then((r) => json<RadioStation>(r)),
  deleteRadio: (id: string) =>
    fetch(`/api/radio/${encodeURIComponent(id)}`, { method: 'DELETE' }).then((r) =>
      json<unknown>(r),
    ),
  playRadio: (id: string) =>
    fetch(`/api/radio/${encodeURIComponent(id)}/play`, { method: 'POST' }).then((r) =>
      json<unknown>(r),
    ),

  // `tracks` is one or more `PlayRef` envelopes (a single track or a whole
  // album), wrapped in `{ tracks }` to match the add-to-playlist envelope.
  playNext: (tracks: PlayRef[]) =>
    fetch('/api/playback/play-next', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tracks }),
    }).then((r) => json<unknown>(r)),
  clearAndPlay: (tracks: PlayRef[]) =>
    fetch('/api/playback/clear-play', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tracks }),
    }).then((r) => json<unknown>(r)),
  addToPlaylist: (name: string, tracks: PlayRef[]) =>
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
  version: () => fetch('/api/version').then((r) => json<{ version: string }>(r)),
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

  // —— Visualizer tuning (persisted on disk, not the browser) ——
  getVizParams: () =>
    fetch('/api/visualizer/params').then((r) => json<Record<string, number | string>>(r)),
  saveVizParams: (params: Record<string, number | string>) =>
    fetch('/api/visualizer/params', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    }).then((r) => json<unknown>(r)),

  // —— Bluetooth ——
  btDevices: () =>
    fetch('/api/bluetooth/devices').then((r) => json<BtDevice[]>(r)),
  btAudioDevices: () =>
    fetch('/api/bluetooth/devices/audio').then((r) => json<BtDevice[]>(r)),
  btScan: (timeout = 15) =>
    fetch('/api/bluetooth/scan', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ timeout }),
    }).then((r) => json<unknown>(r)),
  btScanStop: () =>
    fetch('/api/bluetooth/scan/stop', { method: 'POST' }).then((r) => json<unknown>(r)),
  btScanResults: () =>
    fetch('/api/bluetooth/scan/results').then((r) => json<ScanResultsResponse>(r)),
  btPair: (address: string) =>
    fetch('/api/bluetooth/pair', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btConnect: (address: string) =>
    fetch('/api/bluetooth/connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btWakeConnect: (address: string) =>
    fetch('/api/bluetooth/wake-connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btDisconnect: (address: string) =>
    fetch('/api/bluetooth/disconnect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btForget: (address: string) =>
    fetch('/api/bluetooth/forget', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btRemoveOutput: (address: string) =>
    fetch('/api/bluetooth/remove-output', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<unknown>(r)),
  btRename: (address: string, name: string) =>
    fetch('/api/bluetooth/rename', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address, name }),
    }).then((r) => json<unknown>(r)),
  btTestConnect: (address: string) =>
    fetch('/api/bluetooth/test-connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ address }),
    }).then((r) => json<{ success: boolean; message: string }>(r)),
  btInputEnable: () =>
    fetch('/api/bluetooth/input/enable', { method: 'POST' }).then((r) =>
      json<unknown>(r),
    ),
  btInputDisable: () =>
    fetch('/api/bluetooth/input/disable', { method: 'POST' }).then((r) =>
      json<unknown>(r),
    ),
  btInputStatus: () =>
    fetch('/api/bluetooth/input/status').then((r) => json<InputStatusResponse>(r)),
}
