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
}

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
}
