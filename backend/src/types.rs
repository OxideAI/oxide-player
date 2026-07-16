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
}

/// A single entry in the play queue.
#[derive(Serialize)]
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
#[derive(Serialize)]
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerStatus {
    pub state: PlaybackState,
    pub volume: u8,
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

impl PlayerStatus {
    pub fn stopped() -> Self {
        Self {
            state: PlaybackState::Stopped,
            volume: 0,
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
