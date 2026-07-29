export interface TrackRef {
  id: number
  uri: string
  title: string | null
  artist: string | null
  album: string | null
  has_cover: boolean
  cover_key: string | null
  format: string | null
  sample_rate: number | null
  bit_depth: number | null
  channels: number | null
  duration: number | null
  cue_start: number | null
}

export interface Track extends TrackRef {
  path: string
  genre: string | null
  year: number | null
  track: number | null
  duration: number | null
  album_artist: string | null
  format: string | null
  sample_rate: number | null
  bit_depth: number | null
  channels: number | null
  cue_index: number | null
  start_time: number | null
  end_time: number | null
  file_mtime: number | null
  source: string | null
}

/**
 * An album paired with the library source folder(s) that produced it. A single
 * album can list more than one source when parent/child sources are both
 * configured (issue #46).
 */
export type AlbumSources = [string, string[]]

export interface QueueItem {
  pos: number
  id: number
  uri: string
  title: string | null
  artist: string | null
  album: string | null
  duration: number | null
}

export interface QueueResponse {
  entries: QueueItem[]
  current: number | null
}

export interface OutputDevice {
  id: number
  name: string
  enabled: boolean
}

export type PlaybackState = 'playing' | 'paused' | 'stopped'

export interface PlayerStatus {
  state: PlaybackState
  volume: number
  elapsed: number
  duration: number
  outputs: OutputDevice[]
  error: string | null
  current_song: TrackRef | null
  random: boolean
}

// Mirrors the backend's `StatusEvent` (serde internally-tagged enum). The
// variant struct is flattened into the object next to `type` — there is no
// `Status`/`Queue` wrapper key.
export type StatusEvent =
  | ({ type: 'status' } & PlayerStatus)
  | ({ type: 'queue' } & QueueResponse)

export type DspMode = 'bit_perfect' | 'resample'
export type ResamplePreset = 'balanced' | 'high' | 'extreme'
export type EqBandType = 'peaking' | 'low_shelf' | 'high_shelf'

export interface EqBand {
  type: EqBandType
  freq: number
  gain: number
  q: number
}

export interface DspProfile {
  device: string
  mode: DspMode
  target_rate: number | null
  preset: ResamplePreset
  eq_bands: EqBand[]
}

export interface DeviceConfig {
  name: string
  output_type: string
  device: string | null
  format: string | null
  mixer_type: string | null
  mixer_device: string | null
  dop: boolean
  restart_pending: boolean
  include_warning?: boolean
}

export interface Config {
  mpd_host: string
  mpd_port: number
  mpd_autostart: boolean
  mpd_binary: string | null
  mpd_config: string | null
  mpd_music_directory: string | null
  listen: string
  data_dir: string
  library_dirs: string[]
  static_dir: string
  camilladsp_config_path: string
  camilladsp_ws_url: string | null
  camilladsp_capture_device: string | null
  camilladsp_capture_rate: number | null
  default_dsp_profiles: DspProfile[]
  visualizer_fft: boolean
  visualizer_capture_device: string | null
  visualizer_capture_rate: number | null
}
