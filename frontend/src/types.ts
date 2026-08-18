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
  dsp_supported?: boolean
  dsp_enabled?: boolean
  dsp_reason?: string
}

export type DeviceOutputRole = 'playback' | 'system' | 'unknown'
export type DeviceOutputDiagnosticCode =
  | 'reload_error'
  | 'unsupported_output_type'
  | 'disconnected'
  | 'inactive'
  | 'missing_profile'
  | 'unknown_output'

/** Enriched `/api/devices` detail; distinct from the live WS output shape. */
export interface DeviceOutput extends OutputDevice {
  role: DeviceOutputRole
  selectable: boolean
  selection_key: string
  configured: boolean
  available: boolean
  connected: boolean | null
  active: boolean
  dsp_supported: boolean
  dsp_enabled: boolean
  /** Configured ALSA/BlueALSA endpoint used to match the selected DSP profile. */
  dsp_device?: string
  diagnostic_code?: DeviceOutputDiagnosticCode
  technical_detail?: string
}

export type PlaybackState = 'playing' | 'paused' | 'stopped'

export interface PlayerStatus {
  state: PlaybackState
  volume: number | null
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
export interface PlaybackNotice {
  id: number
  track_id: number
  label: string
  reason: 'missing' | 'unplayable'
}

export type StatusEvent =
  | ({ type: 'status' } & PlayerStatus)
  | ({ type: 'queue' } & QueueResponse)
  | ({ type: 'notice' } & PlaybackNotice)

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
  preamp: number
  eq_bands: EqBand[]
}
export interface DspSettings {
  preamp: number
  eq_bands: EqBand[]
}

export interface DspApplyResult {
  device: string
  persisted: boolean
  reload_confirmed: boolean
  active: boolean
  reload_error?: string
}

/// A Bluetooth device as returned by the backend.
export interface BtDevice {
  address: string
  name: string | null
  alias: string | null
  class: number | null
  icon: string | null
  rssi: number | null
  connected: boolean
  paired: boolean
  trusted: boolean
}

/** Returns the best display name: alias > name > address */
export function btDisplayName(d: BtDevice): string {
  return d.alias ?? d.name ?? d.address
}

/** Returns true if this device is an audio output device (speaker/headphone) */
export function btIsAudioOutput(d: BtDevice): boolean {
  if (!d.class) return false
  const major = (d.class >>> 8) & 0x1f
  const minor = (d.class >>> 2) & 0x3f
  if (major !== 0x04) return false // Audio/Video major class
  // Minor classes for audio output:
  // 0x04 = Headset, 0x08 = Hands-free, 0x14 = Loudspeaker,
  // 0x18 = Headphones, 0x1c = Portable Audio, 0x20 = Car Audio, 0x28 = HiFi Audio
  return [0x04, 0x08, 0x14, 0x18, 0x1c, 0x20, 0x28].includes(minor)
}

/** Returns a human-readable device type string based on Bluetooth class */
export function btDeviceType(d: BtDevice): string | null {
  if (!d.class) return null
  const major = (d.class >>> 8) & 0x1f
  const minor = (d.class >>> 2) & 0x3f
  if (major !== 0x04) return null
  switch (minor) {
    case 0x04: return 'Headset'
    case 0x08: return 'Hands-free'
    case 0x10: return 'Microphone'
    case 0x14: return 'Speaker'
    case 0x18: return 'Headphones'
    case 0x1c: return 'Portable Audio'
    case 0x20: return 'Car Audio'
    case 0x28: return 'HiFi Audio'
    default: return 'Audio Device'
  }
}

/// Response from `GET /api/bluetooth/scan/results`.
export interface ScanResultsResponse {
  active: boolean
  devices: BtDevice[]
}

/// Response from `GET /api/bluetooth/input/status`.
export interface InputStatusResponse {
  enabled: boolean
  streaming: boolean
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

/** A USB ALSA playback endpoint returned by `/api/devices/usb`. */
export interface UsbAudioDevice {
  id: string
  name: string
  card: number
  device: number
  alsa_device: string
}

/** A user-managed internet radio station (persisted on the server). */
export interface RadioStation {
  id: string
  name: string
  url: string
  homepage: string | null
  artwork: string | null
}

export interface Config {
  mpd_host: string
  mpd_port: number
  mpd_autostart: boolean
  mpd_binary: string | null
  mpd_config: string | null
  bluetooth_reconnect_on_startup: boolean
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

export type VisualizerStatusState =
  | 'disabled'
  | 'enabled-pending-restart'
  | 'running'
  | 'waiting-for-capture'
  | 'startup/runtime-error'

export interface VisualizerStatus {
  status: VisualizerStatusState
  configured_enabled: boolean
  applied_enabled: boolean
  configured_source: string | null
  configured_rate: number | null
  applied_source: string | null
  applied_rate: number | null
  restart_required: boolean
  detail: string | null
}
