use crate::config::Config;
use crate::dsp::DspManager;
use crate::library::LibraryDb;
use crate::mpd::{Mpd, MpdStatus};
use crate::types::{PlaybackState, PlayerStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pub config: RwLock<Config>,
    pub config_path: Option<PathBuf>,
    pub db: LibraryDb,
    pub dsp: DspManager,
    pub mpd: Mpd,
    pub status: RwLock<PlayerStatus>,
    /// Serializes library scans so concurrent scan/refresh requests can't stack
    /// blocking-pool tasks and starve the runtime.
    pub scan_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(
        config: Config,
        db: LibraryDb,
        dsp: DspManager,
        mpd: Mpd,
        config_path: Option<PathBuf>,
    ) -> Self {
        let profiles = config.default_dsp_profiles.clone();
        let state = AppState {
            inner: Arc::new(Inner {
                config: RwLock::new(config),
                config_path,
                db,
                dsp,
                mpd,
                status: RwLock::new(PlayerStatus::stopped()),
                scan_lock: tokio::sync::Mutex::new(()),
            }),
        };
        let dsp = state.inner.dsp.clone();
        tokio::spawn(async move {
            dsp.seed(profiles).await;
        });
        state
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

    /// Acquire the scan serialization guard (held across a scan/refresh).
    pub async fn scan_guard(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.inner.scan_lock.lock().await
    }

    pub async fn refresh_status(&self) {
        let mpd_status = self.inner.mpd.status().await;
        let outputs = self.inner.mpd.outputs().await;

        // A deleted/missing file: MPD reports a player error
        // ("No such song" / "No such file" / "No such directory") and either
        // stops or auto-skips to the next track. Drop the dead entry from the
        // library so it stops being clickable; if MPD is stuck on it, advance.
        // DB work runs off the async runtime via spawn_blocking.
        if let Ok(ms) = &mpd_status {
            if let Some(err) = &ms.error {
                let missing = err.contains("No such song")
                    || err.contains("No such file")
                    || err.contains("No such directory");
                if missing {
                    let db = self.inner.db.clone();
                    let uri = ms.current_uri.clone();
                    let err_text = err.clone();
                    let removed = tokio::task::spawn_blocking(move || {
                        let mut removed = false;
                        if let Some(uri) = &uri {
                            if db.delete_by_uri_if_missing(uri).unwrap_or(false) {
                                removed = true;
                            }
                        }
                        if let Some(p) = missing_path_from_error(&err_text) {
                            if db.delete_by_path_if_missing(&p).unwrap_or(false) {
                                removed = true;
                            }
                        }
                        removed
                    })
                    .await
                    .unwrap_or(false);
                    if removed {
                        let _ = self.inner.mpd.next().await;
                        let _ = self.inner.mpd.clear_error().await;
                    }
                }
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
            // If a CUE track matches the current playback position, always
            // prefer it: the active/by_id row is the full-file (non-CUE) entry
            // whose elapsed/duration reflect the whole album, not this track.
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
            by_active.or(by_uri)
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
        track.or_else(|| uri.map(|u| crate::types::TrackRef {
            id: 0,
            uri: u,
            title: None,
            artist: None,
            album: None,
            has_cover: false,
            cover_key: None,
            format: None,
            sample_rate: None,
            bit_depth: None,
            channels: None,
            duration: None,
            cue_start: None,
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

/// MPD's missing-file error names the offending file in quotes, e.g.
/// `Failed to decode "/abs/A.flac"; Failed to open "/abs/A.flac": No such
/// file or directory`. Return the first quoted absolute path so we can drop
/// the matching library entry even after MPD has auto-skipped past it.
fn missing_path_from_error(err: &str) -> Option<String> {
    let start = err.find('"')?;
    let rest = &err[start + 1..];
    let end = rest.find('"')?;
    let path = &rest[..end];
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::dsp::DspManager;
    use crate::mpd::{Mpd, MpdStatus};
    use std::path::Path;

    fn test_state() -> (AppState, LibraryDb) {
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
        (AppState::new(cfg, db.clone(), dsp, mpd, None), db)
    }

    /// Regression: after a restart MPD resumes at the CUE address URI
    /// (`<stem>.cue/trackNNNN`), which differs from the library DB's audio-file
    /// URI. The now-playing resolver must map it back to the split track so the
    /// UI shows the real title/cover and highlights the album row — not a
    /// fallback "track0005" entry with id 0 and no cover.
    #[test]
    fn resolve_current_song_maps_cue_address_after_restart() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
        let (state, db) = test_state();

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
            )
            .unwrap();
        db.set_cover(id, true, Some("al_coverkey")).unwrap();

        // Simulate a fresh start: MPD reports the CUE address URI and the
        // in-memory active_track cache is empty (user hasn't clicked play).
        let ms = MpdStatus {
            state: PlaybackState::Playing,
            volume: 80,
            elapsed: 120.0,
            duration: 180.0,
            error: None,
            current_uri: Some("Music/Album.cue/track0002".to_string()),
            current_track: None,
            current_id: Some(42),
            random: false,
        };

        let song = state.resolve_current_song(&ms).await;
        let song = song.expect("CUE address should resolve to a track");

        assert_eq!(song.id, id, "must resolve to the DB split track, not id 0");
        assert_eq!(song.title.as_deref(), Some("Real Title"));
        assert!(song.has_cover, "cover metadata must survive resolution");
        assert_eq!(song.cover_key.as_deref(), Some("al_coverkey"));
        assert_eq!(song.cue_start, Some(100.0));
        });
    }
}
