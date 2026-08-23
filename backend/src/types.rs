use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputDevice {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
}

/// Enriched per-output facts returned by `GET /api/devices`.
///
/// This is deliberately separate from [`OutputDevice`], which is the live
/// status/WebSocket contract and must remain small and stable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceOutput {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
    pub role: DeviceOutputRole,
    pub selectable: bool,
    /// Stable across MPD runtime-ID changes whenever the output is managed.
    pub selection_key: String,
    pub configured: bool,
    /// True means MPD currently reports this output, not that a physical DAC
    /// was probed successfully.
    pub available: bool,
    /// Only meaningful for Bluetooth outputs. Other output types leave this
    /// unset because physical connection probing is not available.
    pub connected: Option<bool>,
    /// MPD's enabled state is the backend's per-output active fact. Route
    /// health still belongs to the status/reduction layer.
    pub active: bool,
    pub dsp_supported: bool,
    pub dsp_enabled: bool,
    /// The configured ALSA/BlueALSA endpoint used by the DSP profile.
    /// Kept separate from the live OutputDevice contract.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsp_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsp_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<DeviceOutputDiagnosticCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceOutputRole {
    Playback,
    System,
    Unknown,
}

impl Default for DeviceOutputRole {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceOutputDiagnosticCode {
    ReloadError,
    UnsupportedOutputType,
    Disconnected,
    Inactive,
    MissingProfile,
    UnknownOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub uri: String,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track: Option<i32>,
    pub duration: Option<f64>,
    pub format: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub channels: Option<u32>,
    pub has_cover: bool,
    /// Album-level cover key (hash of album identity). Tracks in the same album
    /// share one cover file named `{cover_key}.{ext}`, so the cover is read from
    /// disk once per album instead of once per track.
    pub cover_key: Option<String>,
    pub cue_index: Option<i32>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    /// Last-modified time of the underlying file (unix seconds), if readable.
    pub file_mtime: Option<i64>,
    /// Absolute library source folder that produced this track (one of the
    /// configured `library_dirs`). Lets us drop every track of a removed source
    /// and surface which source(s) an album came from (issue #46).
    pub source: Option<String>,
}

/// A single entry in the play queue.
#[derive(Debug, Clone, Serialize)]
pub struct QueueEntry {
    pub pos: u32,
    pub id: u64,
    pub uri: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
}

/// The play queue plus the position of the currently playing entry.
#[derive(Debug, Clone, Serialize)]
pub struct QueueResponse {
    pub entries: Vec<QueueEntry>,
    pub current: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRef {
    pub id: i64,
    pub uri: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub has_cover: bool,
    pub cover_key: Option<String>,
    pub format: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub channels: Option<u32>,
    /// Duration of this (possibly CUE-split) track in seconds. For CUE tracks
    /// this is the split range (`end_time - start_time`), not the full file.
    pub duration: Option<f64>,
    /// For CUE-split tracks, the offset into the backing file where this track
    /// begins. MPD reports `elapsed` against the full file, so the UI subtracts
    /// this to get the position within the track.
    pub cue_start: Option<f64>,
    /// Remote artwork for streams (the playing radio station's art). Library
    /// tracks carry their art via `has_cover`/`cover_key` instead.
    #[serde(default)]
    pub art_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerStatus {
    pub state: PlaybackState,
    pub volume: Option<u8>,
    pub current_song: Option<TrackRef>,
    /// MPD's current song id (a queue entry id, not a DB track id). Cached so
    /// the queue endpoint can locate the playing position without a second MPD
    /// round-trip.
    pub current_id: Option<u64>,
    pub elapsed: f64,
    pub duration: f64,
    pub outputs: Vec<OutputDevice>,
    pub error: Option<String>,
    pub random: bool,
}

/// A message pushed to connected WebSocket clients. Carries the full state the
/// UI needs so a single stream replaces the `/api/status` + `/api/queue` polls.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StatusEvent {
    /// Sent on connect and whenever the player status changes.
    Status(PlayerStatus),
    /// Sent on connect and whenever the queue content or playing position changes.
    Queue(QueueResponse),
    /// Sent when recovery removes one confirmed unplayable library track.
    Notice(PlaybackNotice),
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackNotice {
    pub id: u64,
    pub track_id: i64,
    pub label: String,
    pub reason: PlaybackNoticeReason,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackNoticeReason {
    Missing,
    Unplayable,
}

impl PlayerStatus {
    pub fn stopped() -> Self {
        Self {
            state: PlaybackState::Stopped,
            volume: None,
            current_song: None,
            current_id: None,
            elapsed: 0.0,
            duration: 0.0,
            outputs: Vec::new(),
            error: None,
            random: false,
        }
    }
}
