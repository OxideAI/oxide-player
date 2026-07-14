use crate::config::Config;
use crate::dsp::DspManager;
use crate::library::LibraryDb;
use crate::mpd::Mpd;
use crate::types::PlayerStatus;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pub config: Config,
    pub db: LibraryDb,
    pub dsp: DspManager,
    pub mpd: Mpd,
    pub status: RwLock<PlayerStatus>,
}

impl AppState {
    pub fn new(config: Config, db: LibraryDb, dsp: DspManager, mpd: Mpd) -> Self {
        let state = AppState {
            inner: Arc::new(Inner {
                config,
                db,
                dsp,
                mpd,
                status: RwLock::new(PlayerStatus::stopped()),
            }),
        };
        let profiles = state.inner.config.default_dsp_profiles.clone();
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

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn status_snapshot(&self) -> PlayerStatus {
        self.inner.status.read().await.clone()
    }

    pub async fn refresh_status(&self) {
        let mpd_status = self.inner.mpd.status().await;
        let outputs = self.inner.mpd.outputs().await;

        // A deleted/missing file: MPD reports a player error
        // ("No such song" / "No such file" / "No such directory") and either
        // stops or auto-skips to the next track. Drop the dead entry from the
        // library so it stops being clickable; if MPD is stuck on it, advance.
        if let Ok(ms) = &mpd_status {
            if let Some(err) = &ms.error {
                let missing = err.contains("No such song")
                    || err.contains("No such file")
                    || err.contains("No such directory");
                if missing {
                    let mut removed = false;
                    if let Some(uri) = &ms.current_uri {
                        if self.inner.db.delete_by_uri_if_missing(uri).unwrap_or(false) {
                            removed = true;
                        }
                    }
                    if let Some(p) = missing_path_from_error(err) {
                        if self.inner.db.delete_by_path_if_missing(&p).unwrap_or(false) {
                            removed = true;
                        }
                    }
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
                status.state = ms.state;
                status.volume = ms.volume;
                status.elapsed = ms.elapsed;
                status.duration = ms.duration;
                status.error = ms.error;
                status.random = ms.random;
                status.current_song = {
                    let by_active = self
                        .inner
                        .mpd
                        .active_track()
                        .await
                        .and_then(|id| {
                            ms.current_uri.as_ref().and_then(|uri| {
                                self.inner
                                    .db
                                    .track_by_id(id)
                                    .ok()
                                    .flatten()
                                    .filter(|t| &t.uri == uri)
                                    .map(|t| t)
                            })
                        });
                    let by_elapsed = ms.current_uri.as_ref().and_then(|uri| {
                        self.inner
                            .db
                            .track_by_uri_and_elapsed(uri, ms.elapsed)
                            .ok()
                            .flatten()
                    });
                    let by_uri = ms.current_uri.as_ref().and_then(|uri| {
                        self.inner
                            .db
                            .track_by_uri_cue(uri, ms.current_track.map(|t| t as i32))
                            .ok()
                            .flatten()
                    });
                    by_active
                        .or(by_elapsed)
                        .or(by_uri)
                        .map(|t| crate::types::TrackRef {
                            id: t.id,
                            uri: t.uri,
                            title: t.title,
                            artist: t.artist,
                            album: t.album,
                            has_cover: t.has_cover,
                            format: t.format,
                            sample_rate: t.sample_rate,
                            bit_depth: t.bit_depth,
                            channels: t.channels,
                        })
                };
            }
            Err(e) => {
                status.error = Some(e.to_string());
            }
        }
        status.outputs = match outputs {
            Ok(o) => o,
            Err(_) => Vec::new(),
        };
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
