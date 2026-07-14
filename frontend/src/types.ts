export interface TrackRef {
  id: number
  uri: string
  title: string | null
  artist: string | null
  album: string | null
  has_cover: boolean
  format: string | null
  sample_rate: number | null
  bit_depth: number | null
  channels: number | null
}

export interface Track extends TrackRef {
  path: string
  genre: string | null
  year: number | null
  track: number | null
  duration: number
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
