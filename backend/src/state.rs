use crate::bluetooth::BluetoothManager;
use crate::config::Config;
use crate::devices::config_fragment::ConfigFragmentManager;
use crate::dsp::DspManager;
use crate::library::LibraryDb;
use crate::mpd::{Mpd, MpdStatus};
use crate::radio::RadioManager;
use crate::types::{
    PlaybackNotice, PlaybackNoticeReason, PlaybackState, PlayerStatus, QueueResponse, StatusEvent,
};
use crate::visualizer::VisualizerAnalyzer;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Mutex, RwLock};

fn bluetooth_address_from_device(device: Option<&str>) -> Option<String> {
    let value = device?.strip_prefix("bluealsa:DEV=")?;
    value.split(',').next().filter(|address| !address.is_empty()).map(str::to_string)
}

fn configured_bluetooth_addresses(configs: &[crate::devices::config_fragment::DeviceConfig]) -> std::collections::HashSet<String> {
    configs
        .iter()
        .filter_map(|config| bluetooth_address_from_device(config.device.as_deref()))
        .collect()
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

#[derive(Clone)]
struct FailedEntry {
    queue_id: u64,
    track_id: i64,
    label: String,
    handled: bool,
}

#[derive(Clone, Copy)]
enum FailureKind {
    Missing,
    Unplayable,
}

fn classify_failure(error: &str) -> Option<FailureKind> {
    if error.contains("No such song")
        || error.contains("No such file")
        || error.contains("No such directory")
    {
        return Some(FailureKind::Missing);
    }
    let lower = error.to_ascii_lowercase();
    if lower.contains("unsupported format")
        || lower.contains("no decoder")
        || lower.contains("unknown file format")
        || lower.contains("unrecognized format")
    {
        return Some(FailureKind::Unplayable);
    }
    None
}

fn failure_target(error: &str) -> Option<String> {
    let start = error.find('"')?;
    let rest = &error[start + 1..];
    let end = rest.find('"')?;
    let target = &rest[..end];
    (!target.is_empty()).then(|| target.to_string())
}

fn same_uri_or_suffix(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix('/')
            .is_some_and(|value| value == right || right.ends_with(&format!("/{value}")))
        || right
            .strip_prefix('/')
            .is_some_and(|value| value == left || left.ends_with(&format!("/{value}")))
        || left.ends_with(&format!("/{right}"))
        || right.ends_with(&format!("/{left}"))
}

fn notice_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

fn bounded_track_label(title: &Option<String>, artist: &Option<String>) -> String {
    let label = match (
        title.as_deref().map(str::trim).filter(|value| !value.is_empty()),
        artist.as_deref().map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(title), Some(artist)) => format!("{artist} — {title}"),
        (Some(title), None) => title.to_string(),
        (None, Some(artist)) => artist.to_string(),
        (None, None) => "unknown track".to_string(),
    };
    label.chars().take(160).collect()
}

struct Inner {
    pub config: RwLock<Config>,
    pub config_path: Option<PathBuf>,
    pub db: LibraryDb,
    pub dsp: DspManager,
    pub mpd: Mpd,
    pub bluetooth: BluetoothManager,
    pub visualizer: VisualizerAnalyzer,
    pub radio: RadioManager,
    pub status: RwLock<PlayerStatus>,
    /// Latest player status, queue, and recovery notice are authoritative for
    /// a newly connected WebSocket client.
    pub event_tx: broadcast::Sender<StatusEvent>,
    pub latest_notice: RwLock<Option<PlaybackNotice>>,
    pub notice_seq: AtomicU64,
    pub failed_entry: Mutex<Option<FailedEntry>>,
    /// Serializes library scans so concurrent scan/refresh requests can't stack
    /// blocking-pool tasks and starve the runtime.
    pub scan_lock: tokio::sync::Mutex<()>,
    /// Manages MPD output device config fragments on disk.
    pub device_configs: ConfigFragmentManager,
    /// Whether device config fragments were created/updated/deleted since last
    /// MPD restart.
    pub config_restart_pending: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn new(
        config: Config,
        db: LibraryDb,
        dsp: DspManager,
        mpd: Mpd,
        visualizer: VisualizerAnalyzer,
        bluetooth: BluetoothManager,
        config_path: Option<PathBuf>,
    ) -> Self {
        let profiles = config.default_dsp_profiles.clone();
        let data_dir = config.data_dir.clone();
        // Capacity covers a small backlog so a momentarily-slow WS client
        // doesn't block the sender; lagging receivers resync on their next
        // reconnect rather than replaying every missed frame.
        let (event_tx, _) = broadcast::channel(32);
        let device_configs = ConfigFragmentManager::new(data_dir.join("mpd-outputs.d"))
            .expect("create mpd-outputs.d directory");
        let radio = RadioManager::load(&data_dir);
        let state = AppState {
            inner: Arc::new(Inner {
                config: RwLock::new(config),
                config_path,
                db,
                dsp,
                mpd,
                bluetooth,
                visualizer,
                radio,
                status: RwLock::new(PlayerStatus::stopped()),
                event_tx,
                latest_notice: RwLock::new(None),
                notice_seq: AtomicU64::new(notice_seed()),
                failed_entry: Mutex::new(None),
                scan_lock: tokio::sync::Mutex::new(()),
                device_configs,
                config_restart_pending: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        let dsp = state.inner.dsp.clone();
        tokio::spawn(async move {
            dsp.seed(profiles).await;
        });
        state
    }

    /// Reconnect Bluetooth outputs that were configured by Oxide. This runs
    /// after startup in the background so a sleeping speaker cannot delay the
    /// HTTP server from becoming available.
    pub fn spawn_bluetooth_reconnect(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            if !state.config().await.bluetooth_reconnect_on_startup {
                tracing::info!("Bluetooth startup reconnect disabled by configuration");
                return;
            }

            let addresses = configured_bluetooth_addresses(&state.device_configs().list());
            if addresses.is_empty() {
                return;
            }

            for device in state.bluetooth().list_devices().await {
                if !device.paired || device.connected || !addresses.contains(&device.address) {
                    continue;
                }
                match state.bluetooth().wake_and_connect(&device.address).await {
                    Ok(()) => tracing::info!(
                        "reconnected Bluetooth output '{}' ({}) on startup",
                        device.name.as_deref().unwrap_or("unknown"),
                        device.address
                    ),
                    Err(e) => tracing::warn!(
                        "could not reconnect Bluetooth output '{}' ({}): {e}",
                        device.name.as_deref().unwrap_or("unknown"),
                        device.address
                    ),
                }
            }
        });
    }

    pub fn db(&self) -> &LibraryDb {
        &self.inner.db
    }

    pub fn dsp(&self) -> &DspManager {
        &self.inner.dsp
    }

    pub fn mpd(&self) -> &Mpd {
        &self.inner.mpd
    }

    pub fn bluetooth(&self) -> &BluetoothManager {
        &self.inner.bluetooth
    }

    pub fn device_configs(&self) -> &ConfigFragmentManager {
        &self.inner.device_configs
    }

    pub fn config_restart_pending(&self) -> bool {
        self.inner.config_restart_pending.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_config_restart_pending(&self, pending: bool) {
        self.inner.config_restart_pending.store(pending, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn visualizer(&self) -> &VisualizerAnalyzer {
        &self.inner.visualizer
    }

    pub fn radio(&self) -> &RadioManager {
        &self.inner.radio
    }

    pub async fn config(&self) -> Config {
        self.inner.config.read().await.clone()
    }

    /// The path the config was loaded from, if any. When the server started
    /// from defaults (no `--config` file), this is `None` and config is
    /// persisted to `<data_dir>/config.json` instead.
    #[allow(dead_code)]
    pub fn config_path(&self) -> Option<&PathBuf> {
        self.inner.config_path.as_ref()
    }

    /// Replace the in-memory config and persist it to disk atomically. The
    /// config file path is reused when known; otherwise we write a new
    /// `config.json` into `data_dir`. Returns the path it was written to.
    pub async fn set_config(&self, config: Config) -> anyhow::Result<PathBuf> {
        let path = match &self.inner.config_path {
            Some(p) => p.clone(),
            None => config.data_dir.join("config.json"),
        };
        // Persist first so a failed write never desynchronizes memory from
        // disk; only swap the in-memory value once the file is safely on disk.
        config.write_to(&path)?;
        *self.inner.config.write().await = config;
        Ok(path)
    }

    pub async fn status_snapshot(&self) -> PlayerStatus {
        self.inner.status.read().await.clone()
    }

    /// Subscribe a WebSocket client to status/queue events. The caller must send
    /// the current snapshot itself (see `api::ws`) before draining this
    /// receiver so the client starts in sync.
    pub fn subscribe_events(&self) -> broadcast::Receiver<StatusEvent> {
        self.inner.event_tx.subscribe()
    }

    pub async fn notice_snapshot(&self) -> Option<PlaybackNotice> {
        self.inner.latest_notice.read().await.clone()
    }

    /// Current queue snapshot (entries + highlighted position).
    pub async fn queue_snapshot(&self) -> QueueResponse {
        match self.inner.mpd.queue().await {
            Ok(entries) => {
                let current = self.current_pos(&entries).await;
                QueueResponse { entries, current }
            }
            Err(_) => QueueResponse {
                entries: Vec::new(),
                current: None,
            },
        }
    }

    /// Broadcast the current queue to WS clients without returning it. Called by
    /// mutation endpoints (remove, jump, shuffle, …) so the UI updates the very
    /// next frame instead of waiting for the 1s poller to notice.
    pub async fn broadcast_queue_now(&self) {
        let _ = self
            .inner
            .event_tx
            .send(StatusEvent::Queue(self.queue_snapshot().await));
    }

    /// Acquire the scan serialization guard (held across a scan/refresh).
    pub async fn scan_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.inner.scan_lock.lock().await
    }

    async fn recover_bad_track(&self, ms: &MpdStatus) -> bool {
        let Some(error) = ms.error.as_deref() else {
            return false;
        };
        let Some(kind) = classify_failure(error) else {
            return false;
        };
        let Some(current_uri) = ms.current_uri.clone() else {
            return false;
        };
        let target = failure_target(error);
        let queue_id = match (ms.current_id, target.as_deref()) {
            (Some(queue_id), None) => queue_id,
            (Some(queue_id), Some(target)) if same_uri_or_suffix(&current_uri, target) => queue_id,
            (_, Some(target)) => {
                let Ok(entries) = self.inner.mpd.queue().await else {
                    return false;
                };
                let matches: Vec<_> = entries
                    .iter()
                    .filter(|entry| same_uri_or_suffix(&entry.uri, target))
                    .collect();
                if matches.len() != 1 {
                    return false;
                }
                matches[0].id
            }
            _ => return false,
        };
        let lookup_uri = target.clone().unwrap_or_else(|| current_uri.clone());
        let cue_index = ms.current_track.map(|track| track as i32);

        let mut failed = self.inner.failed_entry.lock().await;
        if let Some(existing) = failed.as_ref() {
            if existing.queue_id == queue_id && existing.handled {
                return false;
            }
            if existing.queue_id != queue_id {
                *failed = None;
            }
        }
        if failed.is_none() {
            let db = self.inner.db.clone();
            let lookup_path = lookup_uri.clone();
            let track = tokio::task::spawn_blocking(move || {
                let by_path = if lookup_path.starts_with('/') {
                    db.track_by_path_unique(&lookup_path).ok().flatten()
                } else {
                    None
                };
                by_path
                    .or_else(|| db.track_by_cue_address(&lookup_path).ok().flatten())
                    .or_else(|| {
                        cue_index.and_then(|index| {
                            db.track_by_uri_cue(&lookup_path, Some(index))
                                .ok()
                                .flatten()
                        })
                    })
                    .or_else(|| {
                        db.track_by_uri_suffix_unique(&lookup_path)
                            .ok()
                            .flatten()
                    })
            })
            .await
            .ok()
            .flatten();
            let Some(track) = track else {
                return false;
            };
            let label = bounded_track_label(&track.title, &track.artist);
            *failed = Some(FailedEntry {
                queue_id,
                track_id: track.id,
                label,
                handled: false,
            });
        }

        let entry = failed.as_mut().expect("failed entry inserted above");
        if entry.handled {
            return false;
        }
        let track_id = entry.track_id;
        let label = entry.label.clone();
        drop(failed);

        let db = self.inner.db.clone();
        let removed = match tokio::task::spawn_blocking(move || match kind {
            FailureKind::Missing => db.delete_track_if_missing(track_id),
            FailureKind::Unplayable => db.delete_track(track_id),
        })
        .await
        {
            Ok(Ok(removed)) => removed,
            _ => return false,
        };
        if !removed {
            if let Some(entry) = self.inner.failed_entry.lock().await.as_mut() {
                if entry.queue_id == queue_id {
                    entry.handled = true;
                }
            }
            return false;
        }
        if let Some(entry) = self.inner.failed_entry.lock().await.as_mut() {
            if entry.queue_id == queue_id {
                entry.handled = true;
            }
        }

        let notice = PlaybackNotice {
            id: self.inner.notice_seq.fetch_add(1, Ordering::Relaxed) + 1,
            track_id,
            label,
            reason: match kind {
                FailureKind::Missing => PlaybackNoticeReason::Missing,
                FailureKind::Unplayable => PlaybackNoticeReason::Unplayable,
            },
        };
        *self.inner.latest_notice.write().await = Some(notice.clone());
        let _ = self.inner.event_tx.send(StatusEvent::Notice(notice));

        let _ = self.inner.mpd.clear_error().await;
        if self
            .inner
            .mpd
            .status()
            .await
            .ok()
            .and_then(|status| status.current_id)
            == Some(queue_id)
        {
            match self.inner.mpd.queue().await {
                Ok(entries) => {
                    let has_next = entries
                        .iter()
                        .position(|entry| entry.id == queue_id)
                        .is_some_and(|pos| pos + 1 < entries.len());
                    if has_next {
                        let _ = self.inner.mpd.next().await;
                    } else {
                        let _ = self.inner.mpd.stop().await;
                    }
                }
                Err(_) => {
                    let _ = self.inner.mpd.stop().await;
                }
            }
        }
        let _ = self.inner.mpd.clear_error().await;
        self.broadcast_queue_now().await;
        true
    }

    pub async fn refresh_status(&self) {
        let mut mpd_status = self.inner.mpd.status().await;
        let outputs = self.inner.mpd.outputs().await;
        if let Ok(ms) = &mpd_status {
            if self.recover_bad_track(ms).await {
                mpd_status = self.inner.mpd.status().await;
            }
        }

        let mut status = self.inner.status.write().await;
        match mpd_status {
            Ok(ms) => {
                let current_song = self.resolve_current_song(&ms).await;
                status.state = ms.state;
                status.volume = ms.volume;
                status.elapsed = current_song
                    .as_ref()
                    .and_then(|s| s.cue_start)
                    .map(|start| (ms.elapsed - start).max(0.0))
                    .unwrap_or(ms.elapsed);
                status.duration = current_song
                    .as_ref()
                    .and_then(|s| s.duration)
                    .unwrap_or(ms.duration);
                status.error = ms.error;
                status.random = ms.random;
                status.current_id = ms.current_id;
                status.current_song = current_song;
            }
            Err(e) => {
                // MPD is unreachable (or the connection dropped): don't keep
                // showing the last "Playing" track. Mark stopped and clear the
                // now-playing fields so the UI reflects the lost link instead of
                // a phantom playing state.
                status.error = Some(e.to_string());
                status.state = PlaybackState::Stopped;
                status.current_id = None;
                status.current_song = None;
                status.elapsed = 0.0;
            }
        }
        status.outputs = match outputs {
            Ok(o) => o,
            Err(_) => Vec::new(),
        };
        // Push the new status to WS clients.
        let _ = self.inner.event_tx.send(StatusEvent::Status(status.clone()));
    }

    /// Position of the current song within `entries`, or `None` when nothing is
    /// playing. Uses the cached current song id from the status snapshot instead
    /// of a second MPD round-trip. `QueueEntry.id` and `current_id` are both MPD
    /// SongIds (not DB track ids), so the comparison is sound.
    pub async fn current_pos(&self, entries: &[crate::types::QueueEntry]) -> Option<u32> {
        match self.status_snapshot().await.current_id {
            Some(id) => entries.iter().position(|e| e.id == id).map(|p| p as u32),
            None => None,
        }
    }

    /// Resolve the now-playing track from the library. Runs synchronous SQLite
    /// queries off the async runtime via `spawn_blocking`.
    async fn resolve_current_song(&self, ms: &MpdStatus) -> Option<crate::types::TrackRef> {
        let db = self.inner.db.clone();
        let active = self.inner.mpd.active_track().await;
        let uri = ms.current_uri.clone();
        let elapsed = ms.elapsed;
        let current_track = ms.current_track;
        let uri_for_blocking = uri.clone();
        let track = tokio::task::spawn_blocking(move || {
            let by_elapsed = uri_for_blocking
                .as_ref()
                .and_then(|u| db.track_by_uri_and_elapsed(u, elapsed).ok().flatten());
            if by_elapsed.is_some() {
                return by_elapsed;
            }
            // After a restart MPD resumes at the CUE address URI
            // (`<stem>.cue/trackNNNN`), which doesn't match the library DB's
            // audio-file URI. Map it back to the CUE split by its track number.
            let by_cue_address = uri_for_blocking
                .as_ref()
                .and_then(|u| db.track_by_cue_address(u).ok().flatten());
            if by_cue_address.is_some() {
                return by_cue_address;
            }
            let by_active = active.and_then(|id| {
                uri_for_blocking.as_ref().and_then(|u| {
                    db.track_by_id(id)
                        .ok()
                        .flatten()
                        .filter(|t| &t.uri == u)
                })
            });
            let by_uri = uri_for_blocking.as_ref().and_then(|u| {
                db.track_by_uri_cue(u, current_track.map(|t| t as i32))
                    .ok()
                    .flatten()
            });
            // MPD reports URIs relative to its `music_directory`, while the DB
            // stores them relative to the scanned library dir — the two differ
            // by a leading segment (e.g. MPD `MyMusic/Artist/Album.flac` vs DB
            // `Artist/Album.flac`). Match by suffix so a restart (where the
            // in-memory active-track cache is empty) still resolves the track
            // and its title/cover.
            let by_suffix = uri_for_blocking
                .as_ref()
                .and_then(|u| db.track_by_uri_suffix(u).ok().flatten());
            by_active.or(by_uri).or(by_suffix)
        })
        .await
        .ok()
        .flatten()
        .map(|t| crate::types::TrackRef {
            id: t.id,
            uri: t.uri,
            title: t.title,
            artist: t.artist,
            album: t.album,
            has_cover: t.has_cover,
            cover_key: t.cover_key,
            format: t.format,
            sample_rate: t.sample_rate,
            bit_depth: t.bit_depth,
            channels: t.channels,
            duration: t.cue_index.and_then(|_| match (t.start_time, t.end_time) {
                (Some(start), Some(end)) => Some((end - start).max(0.0)),
                (Some(start), None) => t.duration.map(|d| (d - start).max(0.0)),
                _ => t.duration,
            }),
            cue_start: t.cue_index.and(t.start_time),
        });
        // Fallback: MPD is playing a song but we couldn't resolve it from the
        // library DB — still return a minimal TrackRef so the UI doesn't show
        // "Nothing playing" for a song that is clearly audible.
        track.or_else(|| uri.map(|u| {
            // Streams (http(s) URLs) never resolve from the library DB. Surface
            // the live icy-metadata title MPD reports and the station name as
            // artist so NowPlaying shows something meaningful instead of a
            // bare URL.
            let (title, artist) = if u.starts_with("http://") || u.starts_with("https://") {
                let station = self.inner.radio.by_url(&u);
                (ms.current_title.clone(), station.map(|s| s.name))
            } else {
                (None, None)
            };
            crate::types::TrackRef {
                id: 0,
                uri: u,
                title,
                artist,
                album: None,
                has_cover: false,
                cover_key: None,
                format: None,
                sample_rate: None,
                bit_depth: None,
                channels: None,
                duration: None,
                cue_start: None,
            }
        }))
    }

    pub fn spawn_status_poller(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                state.refresh_status().await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth::BluetoothManager;
    use crate::config::Config;
    use crate::dsp::DspManager;
    use crate::mpd::{Mpd, MpdStatus};
    use std::path::Path;

    async fn test_state() -> (AppState, LibraryDb) {
        let db = LibraryDb::open(Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        let cfg = Config::default_config();
        let dsp = DspManager::new(
            std::env::temp_dir().join("oxide_test_dsp.yaml"),
            None,
            "".to_string(),
            44100,
            false,
            None,
        );
        let mpd = Mpd::with_connection("127.0.0.1", 6600, false, None, None);
        let visualizer = VisualizerAnalyzer::new(&cfg);
        let bt = BluetoothManager::new().await;
        (AppState::new(cfg, db.clone(), dsp, mpd, visualizer, bt, None), db)
    }

    /// Regression: after a restart MPD resumes at the CUE address URI
    /// (`<stem>.cue/trackNNNN`), which differs from the library DB's audio-file
    /// URI. The now-playing resolver must map it back to the split track so the
    /// UI shows the real title/cover and highlights the album row — not a
    /// fallback "track0005" entry with id 0 and no cover.
    #[tokio::test]
    async fn resolve_current_song_maps_cue_address_after_restart() {
        let (state, db) = test_state().await;

        // CUE split keyed by the *audio file* URI, with cover metadata.
        let id = db
            .insert_track(
                "Music/Album.flac",
                "Music/Album.flac",
                Some("Real Title"),
                Some("Artist"),
                Some("Album"),
                None,
                None,
                None,
                Some(2),
                Some(80.0),
                Some("flac"),
                Some(44100),
                Some(16),
                Some(2),
                None,
                Some(2),
                Some(100.0),
                Some(180.0),
                Some(1),
                None,
            )
            .unwrap();
        db.set_cover(id, true, Some("al_coverkey")).unwrap();

        // Simulate a fresh start: MPD reports the CUE address URI and the
        // in-memory active_track cache is empty (user hasn't clicked play).
        let ms = MpdStatus {
            state: PlaybackState::Playing,
            volume: Some(80),
            elapsed: 120.0,
            duration: 180.0,
            error: None,
            current_uri: Some("Music/Album.cue/track0002".to_string()),
            current_track: None,
            current_id: Some(42),
            current_title: None,
            random: false,
        };

        let song = state.resolve_current_song(&ms).await;
        let song = song.expect("CUE address should resolve to a track");

        assert_eq!(song.id, id, "must resolve to the DB split track, not id 0");
        assert_eq!(song.title.as_deref(), Some("Real Title"));
        assert!(song.has_cover, "cover metadata must survive resolution");
        assert_eq!(song.cover_key.as_deref(), Some("al_coverkey"));
        assert_eq!(song.cue_start, Some(100.0));
    }

    /// Regression: MPD reports URIs relative to its `music_directory`, while the
    /// DB stores them relative to the scanned library dir. After a restart the
    /// in-memory active-track cache is empty, so the resolver must match the
    /// MPD URI against the DB URI by suffix (e.g. MPD `MyMusic/A/09.flac` vs DB
    /// `A/09.flac`) and recover title/artist/album/cover — not a fallback.
    #[tokio::test]
    async fn resolve_current_song_matches_mpd_uri_by_suffix_after_restart() {
        let (state, db) = test_state().await;

        let id = db
            .insert_track(
                "Cesaria Evora/09 - Historia De Un Amor.m4a",
                "/music/Cesaria Evora/09 - Historia De Un Amor.m4a",
                Some("Historia De Un Amor"),
                Some("Cesaria Evora"),
                Some("Cesaria Evora &"),
                None,
                None,
                None,
                Some(9),
                Some(237.0),
                Some("m4a"),
                Some(44100),
                Some(16),
                Some(2),
                None,
                None,
                None,
                None,
                Some(1),
                None,
            )
            .unwrap();
        db.set_cover(id, true, Some("al_coverkey")).unwrap();

        // Simulate a restart: MPD reports the URI prefixed with the
        // music_directory segment (`MyMusic/...`) and active_track is empty.
        let ms = MpdStatus {
            state: PlaybackState::Playing,
            volume: Some(49),
            elapsed: 10.0,
            duration: 237.0,
            error: None,
            current_uri: Some(
                "MyMusic/Cesaria Evora/09 - Historia De Un Amor.m4a".to_string(),
            ),
            current_track: Some(9),
            current_id: Some(19),
            current_title: None,
            random: false,
        };

        let song = state.resolve_current_song(&ms).await;
        let song = song.expect("MPD URI suffix should resolve to a track");

        assert_eq!(song.id, id);
        assert_eq!(song.title.as_deref(), Some("Historia De Un Amor"));
        assert_eq!(song.artist.as_deref(), Some("Cesaria Evora"));
        assert_eq!(song.album.as_deref(), Some("Cesaria Evora &"));
        assert!(song.has_cover);
        assert_eq!(song.cover_key.as_deref(), Some("al_coverkey"));
    }

    /// Streams (http(s) URLs) never resolve from the library DB. The fallback
    /// TrackRef must carry the icy-metadata title MPD reports and the matched
    /// station's name as artist so NowPlaying shows a meaningful row.
    #[tokio::test]
    async fn resolve_current_song_stream_falls_back_to_station() {
        let (state, _db) = test_state().await;

        // The default-config radio store is seeded with JFK Ibiza, whose URL
        // is exactly this one.
        let ms = MpdStatus {
            state: PlaybackState::Playing,
            volume: Some(80),
            elapsed: 0.0,
            duration: 0.0,
            error: None,
            current_uri: Some("https://stream.aiir.com/7dsjltmny8cvv".to_string()),
            current_track: None,
            current_id: Some(7),
            current_title: Some("Kool Tune - Some Artist".to_string()),
            random: false,
        };

        let song = state
            .resolve_current_song(&ms)
            .await
            .expect("stream URI must resolve to a fallback TrackRef");

        assert_eq!(song.id, 0, "streams are not DB tracks");
        assert_eq!(song.uri, "https://stream.aiir.com/7dsjltmny8cvv");
        assert_eq!(song.title.as_deref(), Some("Kool Tune - Some Artist"));
        assert_eq!(song.artist.as_deref(), Some("JFK Ibiza"));
        assert_eq!(song.duration, None, "streams have no duration");
        assert!(!song.has_cover);
    }

    /// Regression for #3: a freshly subscribed broadcast receiver must receive
    /// a `Status` event after `refresh_status` writes a new snapshot. This is
    /// the mechanism the `/api/ws` handler relies on to push live updates.
    #[tokio::test]
    async fn refresh_status_broadcasts_status_event() {
        let (state, _db) = test_state().await;
        let mut rx = state.subscribe_events();

        // `refresh_status` will fail to reach MPD (no server) and mark the
        // status stopped — but it must still broadcast that change.
        state.refresh_status().await;

        // The receiver should get at least one Status event.
        let mut got_status = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
                Ok(Ok(crate::types::StatusEvent::Status(_))) => {
                    got_status = true;
                    break;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                Err(_) => continue,
            }
        }
        assert!(got_status, "refresh_status must broadcast a Status event");

    }
    #[test]
    fn classifies_missing_before_unplayable_errors() {
        assert!(matches!(
            classify_failure("Failed to open \"x\": No such file or directory"),
            Some(FailureKind::Missing)
        ));
        assert!(matches!(
            classify_failure("Unsupported format: x"),
            Some(FailureKind::Unplayable)
        ));
        assert!(classify_failure("No error").is_none());
    }

    #[test]
    fn bounds_playback_notice_labels() {
        let title = Some("  Track title  ".to_string());
        let artist = Some("Artist".to_string());
        assert_eq!(bounded_track_label(&title, &artist), "Artist — Track title");

        let long = Some("x".repeat(200));
        assert_eq!(bounded_track_label(&long, &None).chars().count(), 160);
    }

    #[test]
    fn configured_bluetooth_addresses_only_include_bluealsa_outputs() {
        let configs = vec![
            crate::devices::config_fragment::DeviceConfig {
                name: "Speaker".to_string(),
                output_type: "alsa".to_string(),
                device: Some("bluealsa:DEV=AA:BB:CC:DD:EE:FF,PROFILE=a2dp".to_string()),
                format: None,
                mixer_type: None,
                mixer_device: None,
                mixer_control: None,
                dop: false,
            },
            crate::devices::config_fragment::DeviceConfig {
                name: "Loopback".to_string(),
                output_type: "alsa".to_string(),
                device: Some("hw:Loopback,1".to_string()),
                format: None,
                mixer_type: None,
                mixer_device: None,
                mixer_control: None,
                dop: false,
            },
        ];

        assert_eq!(
            configured_bluetooth_addresses(&configs),
            std::collections::HashSet::from(["AA:BB:CC:DD:EE:FF".to_string()])
        );
    }
}
