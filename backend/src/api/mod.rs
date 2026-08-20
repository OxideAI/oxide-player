use crate::devices::config_fragment::{
    classify_output_role, diagnostic_code, output_selection_key, validate_config,
    DeviceConfig as DeviceConfigDto, MANAGED_DSP_LOOPBACK_OUTPUT,
    OutputDiagnosticFacts,
};
use crate::devices::usb;
use crate::radio::RadioStation;
use crate::dsp::{parse_dsp_text, DspSettings};
use crate::dsp::camilladsp::DspApplyResult;
use crate::dsp::profile::DspProfile;
use crate::error::{AppError, AppResult};
use crate::library::db::LibrarySnapshot;
use crate::types::{
    DeviceOutput, DeviceOutputDiagnosticCode, DeviceOutputRole, PlaybackState,
};
use crate::state::AppState;
use axum::extract::{Path, Query, Request, State, ws::WebSocketUpgrade};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use tower_http::services::ServeDir;
use serde::Deserialize;
use axum::{Json, Router};

mod bluetooth;

const DEFAULT_INDEX_HTML: &str = "<!doctype html><html><head><meta charset=utf-8>\
<title>Oxide</title></head><body><h1>Oxide</h1>\
<p>Backend is running. Build the frontend into the static directory to use the UI.</p></body></html>";

pub async fn router(state: AppState) -> Router {
    let static_dir = state.config().await.static_dir.clone();
    let spa_dir = static_dir.clone();
    let spa_service = (move |_: Request| async move {
        let html = std::fs::read_to_string(spa_dir.join("index.html"))
            .unwrap_or_else(|_| DEFAULT_INDEX_HTML.to_string());
        Html(html)
    })
    .into_service();

    Router::new()
        .route("/api/status", get(status))
        .route("/api/library", get(library_list))
        .route("/api/library/albums", get(library_albums))
        .route("/api/library/albums/sources", get(library_albums_sources))
        .route("/api/library/artists", get(library_artists))
        .route("/api/cover/{key}", get(cover))
        .route("/api/library/scan", post(library_scan))
        .route("/api/library/refresh", post(library_refresh))
        .route("/api/library/rescan-art", post(library_rescan_art))
        .route("/api/playback/play", post(play))
        .route("/api/playback/pause", post(pause))
        .route("/api/playback/stop", post(stop))
        .route("/api/playback/next", post(next))
        .route("/api/playback/prev", post(prev))
        .route("/api/playback/seek", post(seek))
        .route("/api/playback/volume", post(volume))
        .route("/api/playback/play-next", post(play_next))
        .route("/api/playback/clear-play", post(clear_play))
        .route("/api/queue", get(queue))
        .route("/api/ws", get(ws))
        .route("/api/visualizer", get(visualizer_ws))
        .route("/api/visualizer/status", get(visualizer_status))
        .route("/api/visualizer/params", get(visualizer_params_get))
        .route("/api/visualizer/params", put(visualizer_params_put))
        .route("/api/playback/shuffle", post(shuffle_queue))
        .route("/api/playback/jump", post(jump))
        .route("/api/playback/remove", post(remove))
        .route("/api/playback/clear-queue", post(clear_queue))
        .route("/api/devices", get(devices))
        .route("/api/devices/usb", get(usb_devices))
        .route("/api/devices/configs", get(list_device_configs))
        .route("/api/devices/configs", post(create_device_config))
        .route("/api/devices/configs/{name}", put(update_device_config))
        .route("/api/devices/configs/{name}", delete(delete_device_config))
        .route("/api/devices/restart-mpd", post(restart_mpd))
        .route("/api/devices/{id}/enable", post(enable_device))
        .route("/api/devices/{id}/disable", post(disable_device))
        .route("/api/devices/{id}/dsp/enable", post(enable_device_dsp))
        .route("/api/devices/{id}/dsp/disable", post(disable_device_dsp))
        .route("/api/radio", get(radio_list))
        .route("/api/radio", post(radio_add))
        .route("/api/radio/{id}", put(radio_update))
        .route("/api/radio/{id}", delete(radio_delete))
        .route("/api/radio/{id}/play", post(radio_play))
        .route("/api/dsp", get(dsp_get))
        .route("/api/dsp/import", post(dsp_import))
        .route("/api/dsp", put(dsp_set))
        .route("/api/playlists", get(playlists))
        .route("/api/playlists", post(save_playlist))
        // Playlist names are a single path segment, so a name containing '/'
        // cannot be addressed by these routes (it is still listed and usable
        // via MPD directly). Avoid '/' in playlist names.
        .route("/api/playlists/{name}", get(playlist_tracks))
        .route("/api/playlists/{name}/add", post(playlist_add))
        .route("/api/playlists/{name}/play", post(playlist_play))
        .route("/api/playlists/{name}/remove", post(playlist_remove))
        .route("/api/playlists/{name}/rename", post(playlist_rename))
        .route("/api/playlists/{name}", delete(playlist_delete))
        .route("/api/version", get(version_get))
        .route("/api/config", get(config_get))
        .route("/api/config", put(config_put))
        .route("/api/config/library-dirs", post(config_add_dir))
        .route("/api/config/library-dirs", delete(config_remove_dir))
        .route("/api/system/shutdown", post(system_shutdown))
        .route("/api/system/restart", post(system_restart))
        .merge(bluetooth::router())
        .fallback_service(
            ServeDir::new(static_dir.clone())
                .append_index_html_on_directories(false)
                .fallback(spa_service),
        )
        .with_state(state)
}

async fn status(State(s): State<AppState>) -> AppResult<Json<crate::types::PlayerStatus>> {
    Ok(Json(s.status_snapshot().await))
}

/// Stream player status, queue changes, and recovery notices over a WebSocket.
/// On connect the client receives the current snapshot (one `Status`, one
/// `Queue`, and the latest `Notice` when present), then subsequent events.
/// Lagging clients drop old messages but receive the retained notice on reconnect.
async fn ws(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio::sync::broadcast::error::RecvError;

        let (mut writer, _reader) = socket.split();
        let mut rx = s.subscribe_events();

        // Seed the client with the current authoritative snapshot before
        // draining the stream.
        let snapshot_status = s.status_snapshot().await;
        let snapshot_queue = s.queue_snapshot().await;
        let mut snapshot = vec![
            crate::types::StatusEvent::Status(snapshot_status),
            crate::types::StatusEvent::Queue(snapshot_queue),
        ];
        if let Some(notice) = s.notice_snapshot().await {
            snapshot.push(crate::types::StatusEvent::Notice(notice));
        }
        for event in snapshot {
            match serde_json::to_string(&event) {
                Ok(text) => {
                    if writer
                        .send(axum::extract::ws::Message::Text(text.into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!("ws snapshot serialize failed: {e}");
                    return;
                }
            }
        }

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let text = match serde_json::to_string(&event) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!("ws event serialize failed: {e}");
                            continue;
                        }
                    };
                    if writer
                        .send(axum::extract::ws::Message::Text(text.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::debug!("ws client lagged, dropped {n} messages");
                    continue;
                }
                Err(RecvError::Closed) => break,
            }
        }
    })
}

/// Return the visualizer's configured/applied capture lifecycle.
async fn visualizer_status(
    State(s): State<AppState>,
) -> AppResult<Json<crate::visualizer::VisualizerStatus>> {
    Ok(Json(s.visualizer_status().await))
}

/// Return the saved visualizer look-and-feel params (from `<data_dir>/vizparams.json`,
/// or code defaults when none are saved).
async fn visualizer_params_get(
    State(s): State<AppState>,
) -> AppResult<Json<crate::visualizer::VizParams>> {
    let data_dir = s.config().await.data_dir;
    Ok(Json(crate::visualizer::VizParams::load(&data_dir)))
}

/// Persist visualizer look-and-feel params to `<data_dir>/vizparams.json`.
async fn visualizer_params_put(
    State(s): State<AppState>,
    Json(body): Json<crate::visualizer::VizParams>,
) -> AppResult<StatusCode> {
    let data_dir = s.config().await.data_dir;
    body.save(&data_dir)
        .map_err(|e| AppError::Library(e.to_string()))?;
    Ok(StatusCode::OK)
}

/// Stream live FFT spectrum frames over a WebSocket. Each message is a JSON
/// object `{ "bins": [f32; BANDS], "level": f32 }` (low→high frequency). The
/// client only receives frames while audio is playing through the captured
/// device; an idle/stopped stream sends a single zeroed frame on connect so the
/// visualizer can render a calm baseline. Best-effort like `/api/ws`.
async fn visualizer_ws(
    ws: WebSocketUpgrade,
    State(s): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio::sync::broadcast::error::RecvError;

        let (mut writer, _reader) = socket.split();
        let mut rx = s.visualizer().subscribe();

        // Seed with a calm baseline so the visualizer isn't frozen on connect.
        let baseline = crate::visualizer::SpectrumFrame {
            bins: vec![0.0; crate::visualizer::BANDS],
            level: 0.0,
        };
        if let Ok(text) = serde_json::to_string(&baseline) {
            let _ = writer
                .send(axum::extract::ws::Message::Text(text.into()))
                .await;
        }

        loop {
            match rx.recv().await {
                Ok(frame) => {
                    let text = match serde_json::to_string(&frame) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!("visualizer serialize failed: {e}");
                            continue;
                        }
                    };
                    if writer
                        .send(axum::extract::ws::Message::Text(text.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::debug!("visualizer client lagged, dropped {n} frames");
                    continue;
                }
                Err(RecvError::Closed) => break,
            }
        }
    })
}

#[derive(Deserialize)]
struct LibraryQuery {
    q: Option<String>,
    artist: Option<String>,
    album: Option<String>,
}

async fn library_list(
    State(s): State<AppState>,
    Query(q): Query<LibraryQuery>,
    headers: axum::http::HeaderMap,
) -> AppResult<Response> {
    let unfiltered = q.q.is_none() && q.artist.is_none() && q.album.is_none();
    if !unfiltered {
        return Ok(Json(s.db().search(
            q.q.as_deref(),
            q.artist.as_deref(),
            q.album.as_deref(),
            None,
        )?).into_response());
    }

    let if_none_match = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    let response = match s.db().unfiltered_snapshot(if_none_match)? {
        LibrarySnapshot::NotModified { etag } => {
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::NOT_MODIFIED;
            response.headers_mut().insert(
                header::ETAG,
                HeaderValue::from_str(&etag).expect("generated etag is valid"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-cache"),
            );
            response
        }
        LibrarySnapshot::Fresh { etag, tracks } => {
            let mut response = Json(tracks).into_response();
            response.headers_mut().insert(
                header::ETAG,
                HeaderValue::from_str(&etag).expect("generated etag is valid"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-cache"),
            );
            response
        }
    };
    Ok(response)
}

async fn library_albums(State(s): State<AppState>) -> AppResult<Json<Vec<String>>> {
    Ok(Json(s.db().list_albums()?))
}

/// Albums paired with the library source folder(s) that produced them. An album
/// can list more than one source when parent/child sources are both configured
/// (issue #46).
async fn library_albums_sources(
    State(s): State<AppState>,
) -> AppResult<Json<Vec<(String, Vec<String>)>>> {
    Ok(Json(s.db().albums_with_sources()?))
}

async fn library_artists(State(s): State<AppState>) -> AppResult<Json<Vec<String>>> {
    Ok(Json(s.db().list_artists()?))
}

async fn cover(State(s): State<AppState>, Path(key): Path<String>) -> AppResult<Response> {
    let dir = s.config().await.cover_cache_dir();
    for ext in crate::library::scanner::COVER_EXTS {
        let p = dir.join(format!("{key}.{ext}"));
        if let Ok(bytes) = tokio::fs::read(&p).await {
            let ct = match *ext {
                "jpg" => "image/jpeg",
                "png" => "image/png",
                _ => "application/octet-stream",
            };
            return Ok((
                [
                    (header::CONTENT_TYPE, ct),
                    (header::CACHE_CONTROL, "public, max-age=31536000, stale-while-revalidate=86400"),
                ],
                bytes,
            )
                .into_response());
        }
    }
    Err(AppError::NotFound(format!("cover {key}")))
}

/// Scan the configured library directories into the DB. `incremental` chooses
/// whether MPD's index is refreshed (`rescan`, keeps existing data) or fully
/// rebuilt (`update`). Shared by the scan/refresh endpoints and library-dir
/// edits so all three paths use the same pipeline.
async fn run_scan(s: &AppState, incremental: bool) -> AppResult<u64> {
    let _guard = s.scan_guard().await;
    let cfg = s.config().await;
    let dirs = cfg.library_dirs.clone();
    let db = s.db().clone();
    let cover_dir = cfg.cover_cache_dir();
    std::fs::create_dir_all(&cover_dir).map_err(|e| AppError::Library(e.to_string()))?;
    let count = tokio::task::spawn_blocking(move || {
        crate::library::scan(&dirs, &db, &cover_dir, crate::config::CoverOptimization::from_config(&cfg))
    })
    .await
    .map_err(|e| AppError::Library(e.to_string()))??;
    if incremental {
        // Keep MPD's index in sync with the filesystem so every scanned track
        // is playable (a stale MPD db is the usual cause of "No such song" on
        // add). `scan` re-reads only files whose mtime changed and lets
        // `prune_missing` drop entries whose files are gone.
        let _ = s.mpd().rescan().await;
    } else {
        let _ = s.mpd().update().await;
    }
    Ok(count)
}

async fn library_scan(State(s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let count = run_scan(&s, false).await?;
    Ok(Json(serde_json::json!({ "scanned": count })))
}

async fn library_refresh(State(s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let count = run_scan(&s, true).await?;
    Ok(Json(serde_json::json!({ "scanned": count })))
}

/// Single app version (backend + frontend always in lockstep via release-please).
async fn version_get() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// Serialized view of the current config for the Settings UI.
async fn config_get(State(s): State<AppState>) -> AppResult<Json<crate::config::Config>> {
    Ok(Json(s.config().await))
}

/// Power off the machine. The backend runs as an unprivileged user, so it
/// escalates via a sudoers rule (installed by install.sh) granting passwordless
/// `systemctl poweroff`. The response may be cut short as the machine powers down.
async fn system_shutdown(State(_s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    run_systemctl("poweroff").await?;
    Ok(Json(serde_json::json!({ "status": "powering off" })))
}

/// Reboot the machine. Same privilege path as shutdown, via `systemctl reboot`
/// and the sudoers rule.
async fn system_restart(State(_s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    run_systemctl("reboot").await?;
    Ok(Json(serde_json::json!({ "status": "rebooting" })))
}

/// Run `systemctl <verb>` as root and surface a non-zero exit as an error. The
/// backend runs as an unprivileged service user, so it escalates via a sudoers
/// rule (installed by install.sh) granting passwordless `systemctl reboot` and
/// `systemctl poweroff`.
async fn run_systemctl(verb: &str) -> AppResult<()> {
    let out = tokio::process::Command::new("sudo")
        .args(["-n", "systemctl", verb])
        .output()
        .await
        .map_err(|e| AppError::Library(format!("failed to run systemctl {verb}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::Library(format!(
            "systemctl {verb} failed: {}",
            if stderr.is_empty() { "permission denied" } else { &stderr }
        )));
    }
    Ok(())
}

/// Replace the whole config (client sends the object it got from
/// `GET /api/config`, possibly edited). Validated, persisted atomically, and
/// swapped into memory. Editing live-applicable fields (e.g. `library_dirs`)
/// takes effect immediately; restart-required fields persist for next launch.
async fn config_put(
    State(s): State<AppState>,
    Json(body): Json<crate::config::Config>,
) -> AppResult<Json<crate::config::Config>> {
    body.validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    s.set_config(body).await
        .map_err(|e| AppError::Library(e.to_string()))?;
    Ok(Json(s.config().await))
}

#[derive(Deserialize)]
struct DirBody {
    path: String,
}

fn canonical_library_dir(path: &std::path::Path) -> AppResult<std::path::PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::BadRequest(
            "library dir must be an absolute path".to_string(),
        ));
    }
    if !path.is_dir() {
        return Err(AppError::BadRequest(format!(
            "library dir is not a valid folder: {}",
            path.display()
        )));
    }
    path.canonicalize().map_err(|e| {
        AppError::BadRequest(format!(
            "cannot resolve library dir {}: {e}",
            path.display()
        ))
    })
}

/// Add a music library source folder. Must be an existing directory. Persists
/// and immediately rescans so the new source is picked up without a restart.
///
/// Dedupe: a folder already covered by an existing source is rejected — if
/// `path` is inside an already-added dir (or already added), nothing changes
/// (`duplicate`). If `path` is a parent of one or more existing sources, those
/// child sources are dropped (they are now subsumed by `path`) before `path` is
/// added, so we never scan the same files twice (issue #46).
async fn config_add_dir(
    State(s): State<AppState>,
    Json(b): Json<DirBody>,
) -> AppResult<Json<serde_json::Value>> {
    let path = canonical_library_dir(std::path::Path::new(&b.path))?;
    let mut cfg = s.config().await;
    let previous_dirs = cfg.library_dirs.clone();
    // Dedupe against existing sources (rejects child-of-existing and exact
    // duplicates; drops child sources when `path` is their parent).
    let subsumed = cfg.add_library_dir(path);
    if subsumed.is_none() {
        return Ok(Json(serde_json::json!({ "scanned": 0, "duplicate": true })));
    }
    let subsumed = subsumed.unwrap();
    // Share the folder before changing config, deleting tracks, or rescanning.
    // If persistence fails, restore the previous share set as well.
    crate::shared_folders::sync(&cfg.data_dir, &cfg.library_dirs)
        .map_err(|e| AppError::Library(e.to_string()))?;
    let data_dir = cfg.data_dir.clone();
    if let Err(error) = s.set_config(cfg).await {
        let _ = crate::shared_folders::sync(&data_dir, &previous_dirs);
        return Err(AppError::Library(error.to_string()));
    }
    let mut removed_tracks = 0u64;
    if !subsumed.is_empty() {
        let db = s.db().clone();
        for d in &subsumed {
            removed_tracks += db.delete_by_source(d).map_err(|e| AppError::Library(e.to_string()))?;
        }
    }
    let count = run_scan(&s, true).await?;
    Ok(Json(serde_json::json!({ "scanned": count, "removed": removed_tracks })))
}

/// Remove a music library source folder by absolute path. Drops every track
/// that came from that source and removes its share before syncing MPD.
async fn config_remove_dir(
    State(s): State<AppState>,
    Json(b): Json<DirBody>,
) -> AppResult<StatusCode> {
    let mut path = std::path::PathBuf::from(&b.path);
    if !path.is_absolute() {
        return Err(AppError::BadRequest(
            "library dir must be an absolute path".to_string(),
        ));
    }
    if path.exists() {
        path = path.canonicalize().map_err(|e| {
            AppError::BadRequest(format!(
                "cannot resolve library dir {}: {e}",
                path.display()
            ))
        })?;
    }
    let mut cfg = s.config().await;
    let before = cfg.library_dirs.len();
    let previous_dirs = cfg.library_dirs.clone();
    cfg.library_dirs.retain(|d| d != &path);
    if cfg.library_dirs.len() == before {
        return Err(AppError::NotFound(format!("library dir {}", path.display())));
    }
    // Remove the share before deleting tracks or resyncing MPD. Restore it if
    // config persistence fails so the share and source list remain consistent.
    let data_dir = cfg.data_dir.clone();
    crate::shared_folders::sync(&data_dir, &cfg.library_dirs)
        .map_err(|e| AppError::Library(e.to_string()))?;
    if let Err(error) = s.set_config(cfg).await {
        let _ = crate::shared_folders::sync(&data_dir, &previous_dirs);
        return Err(AppError::Library(error.to_string()));
    }
    // Remove tracks produced by this source and resync MPD's index.
    let db = s.db().clone();
    let removed = db.delete_by_source(&path).map_err(|e| AppError::Library(e.to_string()))?;
    tracing::info!("removed {} tracks from deleted source {}", removed, path.display());
    let _ = s.mpd().rescan().await;
    Ok(StatusCode::OK)
}

async fn library_rescan_art(State(s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let _guard = s.scan_guard().await;
    let db = s.db().clone();
    let cfg = s.config().await;
    let cover_dir = cfg.cover_cache_dir();
    let opt = crate::config::CoverOptimization::from_config(&cfg);
    std::fs::create_dir_all(&cover_dir).map_err(|e| AppError::Library(e.to_string()))?;
    let with_cover =
        tokio::task::spawn_blocking(move || crate::library::scanner::rescan_art(&db, &cover_dir, opt))
            .await
            .map_err(|e| AppError::Library(e.to_string()))??;
    Ok(Json(serde_json::json!({ "with_cover": with_cover })))
}

#[derive(Deserialize)]
struct PlayBody {
    uri: Option<String>,
    start: Option<f64>,
    end: Option<f64>,
    track_id: Option<i64>,
}

async fn play(State(s): State<AppState>, Json(b): Json<PlayBody>) -> AppResult<StatusCode> {
    match b.uri {
        Some(uri) => {
            let (mpd_uri, is_cue) = resolve_play_uri(&s, &uri, b.track_id).await?;
            // CUE splits are addressed directly; applying the per-track
            // start/end offset would seek into the wrong position.
            if is_cue {
                s.mpd().play_uri(&mpd_uri).await?;
            } else {
                match (b.start, b.end) {
                    (Some(start), end) => s.mpd().play_uri_range(&mpd_uri, start, end).await?,
                    (None, Some(end)) => s.mpd().play_uri_range(&mpd_uri, 0.0, Some(end)).await?,
                    (None, None) => s.mpd().play_uri(&mpd_uri).await?,
                }
            }
            s.mpd().set_active_track(b.track_id).await;
        }
        None => {
            s.mpd().set_active_track(None).await;
            s.mpd().play().await?
        }
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct PauseBody {
    pause: bool,
}

async fn pause(State(s): State<AppState>, Json(b): Json<PauseBody>) -> AppResult<StatusCode> {
    s.mpd().pause(b.pause).await?;
    Ok(StatusCode::OK)
}

async fn stop(State(s): State<AppState>) -> AppResult<StatusCode> {
    s.mpd().stop().await?;
    Ok(StatusCode::OK)
}

async fn next(State(s): State<AppState>) -> AppResult<StatusCode> {
    if s.mpd().status().await?.state == PlaybackState::Stopped {
        let pos = s.mpd().queue_position().await?;
        let len = s.mpd().queue().await?.len() as u32;
        s.mpd().play_position(pos.saturating_add(1).min(len.saturating_sub(1))).await?;
    } else {
        s.mpd().next().await?;
    }
    Ok(StatusCode::OK)
}

async fn prev(State(s): State<AppState>) -> AppResult<StatusCode> {
    if s.mpd().status().await?.state == PlaybackState::Stopped {
        let pos = s.mpd().queue_position().await?;
        if pos == 0 {
            s.mpd().play_position(0).await?;
        } else {
            s.mpd().play_position(pos - 1).await?;
        }
    } else {
        s.mpd().previous().await?;
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct SeekBody {
    seconds: f64,
}

async fn seek(State(s): State<AppState>, Json(b): Json<SeekBody>) -> AppResult<StatusCode> {
    s.mpd().seek(b.seconds).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct VolumeBody {
    volume: u8,
}

async fn volume(State(s): State<AppState>, Json(b): Json<VolumeBody>) -> AppResult<StatusCode> {
    s.mpd().set_volume(b.volume.min(100)).await?;
    Ok(StatusCode::OK)
}

const DSP_LOOPBACK_NAME: &str = MANAGED_DSP_LOOPBACK_OUTPUT;

fn bluetooth_address(device: &str) -> Option<&str> {
    device
        .strip_prefix("bluealsa:DEV=")
        .and_then(|value| value.split(',').next())
        .filter(|value| !value.is_empty())
}

fn configured_dsp_device(
    configs: &[DeviceConfigDto],
    output: &crate::types::OutputDevice,
) -> Result<String, String> {
    if output.name == DSP_LOOPBACK_NAME {
        return Err("CamillaDSP loopback is the DSP path, not a playback target".to_string());
    }
    let config = configs
        .iter()
        .find(|config| config.name == output.name)
        .ok_or_else(|| "output has no configured device preset".to_string())?;
    if !config.output_type.eq_ignore_ascii_case("alsa") {
        return Err("only ALSA outputs support CamillaDSP".to_string());
    }
    let device = config
        .device
        .as_deref()
        .filter(|device| !device.trim().is_empty())
        .ok_or_else(|| "output has no ALSA device configured".to_string())?;
    Ok(device.to_string())
}
fn dsp_route_is_active(
    loopback_enabled: bool,
    target_enabled: bool,
    active_device: Option<&str>,
    target_device: &str,
    profile_configured: bool,
) -> bool {
    loopback_enabled
        && (active_device == Some(target_device)
            || (active_device.is_none() && !target_enabled && profile_configured))
}
/// Enumerate USB DAC playback endpoints exposed by ALSA.
async fn usb_devices(State(_s): State<AppState>) -> AppResult<Json<Vec<usb::UsbAudioDevice>>> {
    usb::scan()
        .await
        .map(Json)
        .map_err(AppError::AudioUnavailable)
}


async fn devices(State(s): State<AppState>) -> AppResult<Json<Vec<DeviceOutput>>> {
    let outputs = s.mpd().outputs().await?;
    let configs = s.device_configs().list();
    let bluetooth = s.bluetooth().list_devices().await;
    let active_device = s.dsp().active_device().await;
    let profiles = s.dsp().list_profiles().await;
    let loopback_enabled = outputs
        .iter()
        .find(|output| output.name == DSP_LOOPBACK_NAME)
        .is_some_and(|output| output.enabled);

    let response = outputs
        .into_iter()
        .map(|output| {
            let role = classify_output_role(&output.name, &configs);
            let config = configs.iter().find(|config| config.name == output.name);
            let device = config.and_then(|config| config.device.as_deref());
            let bluetooth_address = device.and_then(bluetooth_address);
            let connected = bluetooth_address.map(|address| {
                bluetooth
                    .iter()
                    .any(|device| device.address.eq_ignore_ascii_case(address) && device.connected)
            });
            let configured = config.is_some();
            let dsp_device = configured_dsp_device(&configs, &output);
            let profile_configured = dsp_device.as_ref().ok().is_some_and(|device| {
                profiles.iter().any(|profile| profile.device == *device)
                    || profiles.iter().any(|profile| profile.device == "default")
            });
            let dsp_route_active = dsp_device.as_ref().ok().is_some_and(|device| {
                dsp_route_is_active(
                    loopback_enabled,
                    output.enabled,
                    active_device.as_deref(),
                    device,
                    profile_configured,
                )
            });
            let capability_supported = matches!(
                (&config, device),
                (Some(config), Some(device))
                    if config.output_type.eq_ignore_ascii_case("alsa")
                        && !device.trim().is_empty()
            );
            let dsp_supported = capability_supported
                && connected != Some(false)
                && (output.enabled || dsp_route_active);
            let mut dsp_reason = if connected == Some(false) {
                Some("Bluetooth output is not connected".to_string())
            } else if !configured {
                Some("output has no configured device preset".to_string())
            } else if config.is_some_and(|config| !config.output_type.eq_ignore_ascii_case("alsa")) {
                Some("only ALSA outputs support CamillaDSP".to_string())
            } else if device.is_none_or(|device| device.trim().is_empty()) {
                Some("output has no ALSA device configured".to_string())
            } else {
                None
            };
            if capability_supported && !dsp_supported {
                dsp_reason = Some("output is not active".to_string());
            }
            let dsp_enabled = dsp_supported && dsp_route_active;
            let facts = OutputDiagnosticFacts {
                role,
                configured,
                dsp_supported: capability_supported,
                connected,
                enabled: output.enabled,
                profile_configured,
                reload_error: false,
            };
            let diagnostic_code = diagnostic_code(facts);
            let technical_detail = diagnostic_code.map(|code| match code {
                DeviceOutputDiagnosticCode::ReloadError => "CamillaDSP reload was not confirmed".to_string(),
                DeviceOutputDiagnosticCode::UnsupportedOutputType => dsp_reason
                    .clone()
                    .unwrap_or_else(|| "output type is not DSP-capable".to_string()),
                DeviceOutputDiagnosticCode::Disconnected => "configured Bluetooth endpoint is disconnected".to_string(),
                DeviceOutputDiagnosticCode::Inactive => "MPD reports this output as disabled".to_string(),
                DeviceOutputDiagnosticCode::MissingProfile => "no saved DSP profile matches this output".to_string(),
                DeviceOutputDiagnosticCode::UnknownOutput => "runtime output has no managed Oxide configuration".to_string(),
            });
            DeviceOutput {
                id: output.id,
                name: output.name.clone(),
                enabled: output.enabled,
                role,
                selectable: role == DeviceOutputRole::Playback && configured,
                selection_key: output_selection_key(
                    &output.name,
                    config.map_or("runtime", |config| config.output_type.as_str()),
                    device,
                    output.id,
                ),
                configured,
                available: true,
                connected,
                active: output.enabled,
                dsp_supported,
                dsp_enabled,
                dsp_reason,
                diagnostic_code,
                dsp_device: device.map(str::to_string),
                technical_detail,
            }
        })
        .collect();
    Ok(Json(response))
}

async fn enable_device(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    s.mpd().enable_output(id).await?;
    let _ = s.mpd().clear_error().await;
    // Output mixer capabilities can change with the active device. Refresh and
    // broadcast status now so clients show or hide volume controls without
    // waiting for the next poller tick.
    s.refresh_status().await;
    Ok(StatusCode::OK)
}

async fn disable_device(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    let target = s
        .mpd()
        .outputs()
        .await?
        .into_iter()
        .find(|output| output.id == id);
    let active_device = s.dsp().active_device().await;
    let target_device = target.as_ref().and_then(|output| {
        configured_dsp_device(&s.device_configs().list(), output).ok()
    });
    s.mpd().disable_output(id).await?;
    if target
        .as_ref()
        .is_some_and(|output| output.name == DSP_LOOPBACK_NAME)
        || target_device.as_deref() == active_device.as_deref()
    {
        s.dsp().clear_active_device().await;
        s.set_dsp_active_device(None)
            .await
            .map_err(|e| AppError::Library(e.to_string()))?;
    }
    let _ = s.mpd().clear_error().await;
    // Keep volume capability state synchronized after switching outputs.
    s.refresh_status().await;
    Ok(StatusCode::OK)
}

fn dsp_profile_target(
    outputs: &[crate::types::OutputDevice],
    configs: &[DeviceConfigDto],
    device: &str,
) -> Option<crate::types::OutputDevice> {
    outputs
        .iter()
        .find(|output| {
            output.name != DSP_LOOPBACK_NAME
                && configured_dsp_device(configs, output)
                    .ok()
                    .is_some_and(|configured| configured == device)
        })
        .cloned()
}

fn should_release_direct_dsp_output(target_enabled: bool, route_active: bool) -> bool {
    target_enabled && !route_active
}

const DIRECT_DSP_OUTPUT_RELEASE_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

fn direct_dsp_output_release_delay() -> std::time::Duration {
    DIRECT_DSP_OUTPUT_RELEASE_DELAY
}
async fn dsp_output_target(
    s: &AppState,
    id: u32,
) -> AppResult<(crate::types::OutputDevice, String, crate::types::OutputDevice)> {
    let outputs = s.mpd().outputs().await?;
    let target = outputs
        .iter()
        .find(|output| output.id == id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("output {id}")))?;
    let device = configured_dsp_device(&s.device_configs().list(), &target)
        .map_err(AppError::BadRequest)?;
    if let Some(address) = bluetooth_address(&device) {
        let connected = s
            .bluetooth()
            .list_devices()
            .await
            .iter()
            .any(|bt| bt.address == address && bt.connected);
        if !connected {
            return Err(AppError::BadRequest("Bluetooth output is not connected".to_string()));
        }
    }
    let loopback = outputs
        .into_iter()
        .find(|output| output.name == DSP_LOOPBACK_NAME)
        .ok_or_else(|| AppError::BadRequest("CamillaDSP loopback output is not available".to_string()))?;
    let profile_configured = s.dsp().get_profile(&device).await.is_some()
        || s.dsp().get_profile("default").await.is_some();
    let route_active = dsp_route_is_active(
        loopback.enabled,
        target.enabled,
        s.dsp().active_device().await.as_deref(),
        &device,
        profile_configured,
    );
    if !target.enabled && !route_active {
        return Err(AppError::BadRequest("output is not active".to_string()));
    }
    Ok((target, device, loopback))
}
async fn apply_dsp_route(
    s: &AppState,
    target: crate::types::OutputDevice,
    device: String,
    loopback: crate::types::OutputDevice,
    profile: Option<DspProfile>,
) -> AppResult<DspApplyResult> {
    let target_was_enabled = target.enabled;
    if should_release_direct_dsp_output(target_was_enabled, false) {
        s.mpd().disable_output(target.id).await?;
        tokio::time::sleep(direct_dsp_output_release_delay()).await;
    }

    let result = match profile {
        Some(profile) => s.dsp().apply_profile(profile).await,
        None => s.dsp().apply_profile_for_device(&device).await,
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            if target_was_enabled {
                let _ = s.mpd().enable_output(target.id).await;
            }
            return Err(AppError::Dsp(error.to_string()));
        }
    };
    if !result.reload_confirmed {
        if target_was_enabled {
            let _ = s.mpd().enable_output(target.id).await;
        }
        return Ok(result);
    }

    if let Err(error) = s.set_dsp_active_device(Some(device)).await {
        if target_was_enabled {
            let _ = s.mpd().enable_output(target.id).await;
        }
        return Err(AppError::Library(error.to_string()));
    }
    if !loopback.enabled {
        if let Err(error) = s.mpd().enable_output(loopback.id).await {
            s.dsp().clear_active_device().await;
            let _ = s.set_dsp_active_device(None).await;
            if target_was_enabled {
                let _ = s.mpd().enable_output(target.id).await;
            }
            return Err(error);
        }
    }
    let _ = s.mpd().clear_error().await;
    s.refresh_status().await;
    Ok(result)
}


async fn enable_device_dsp(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    let (target, device, loopback) = dsp_output_target(&s, id).await?;
    let result = apply_dsp_route(&s, target, device, loopback, None).await?;
    if !result.reload_confirmed {
        return Err(AppError::Dsp(
            result
                .reload_error
                .unwrap_or_else(|| "CamillaDSP did not confirm the route reload".to_string()),
        ));
    }
    Ok(StatusCode::OK)
}

async fn dsp_profile_route_target(
    s: &AppState,
    device: &str,
) -> AppResult<Option<(crate::types::OutputDevice, String, crate::types::OutputDevice)>> {
    let outputs = match s.mpd().outputs().await {
        Ok(outputs) => outputs,
        Err(_) => return Ok(None),
    };
    let configs = s.device_configs().list();
    let Some(target) = dsp_profile_target(&outputs, &configs, device) else {
        return Ok(None);
    };
    let loopback = outputs
        .into_iter()
        .find(|output| output.name == DSP_LOOPBACK_NAME)
        .ok_or_else(|| AppError::BadRequest("CamillaDSP loopback output is not available".to_string()))?;
    // The profile was persisted immediately before this lookup, so an
    // inactive target with the managed loopback enabled can be inferred as an
    // existing DSP route even before the in-memory profile cache is refreshed.
    let profile_configured = true;
    let route_active = dsp_route_is_active(
        loopback.enabled,
        target.enabled,
        s.dsp().active_device().await.as_deref(),
        device,
        profile_configured,
    );
    if !target.enabled && !route_active {
        return Ok(None);
    }
    Ok(Some((target, device.to_string(), loopback)))
}

async fn disable_device_dsp(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    let (target, _device, loopback) = dsp_output_target(&s, id).await?;
    if loopback.enabled {
        s.mpd().disable_output(loopback.id).await?;
    }
    if !target.enabled {
        s.mpd().enable_output(target.id).await?;
    }
    s.set_dsp_active_device(None)
        .await
        .map_err(|e| AppError::Library(e.to_string()))?;
    let _ = s.mpd().clear_error().await;
    s.refresh_status().await;
    Ok(StatusCode::OK)
}

// ---- Device config fragment CRUD ---- ///

/// Serialized device config for the API.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct DeviceConfigResponse {
    name: String,
    output_type: String,
    device: Option<String>,
    format: Option<String>,
    mixer_type: Option<String>,
    mixer_device: Option<String>,
    dop: bool,
    restart_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_warning: Option<bool>,
}

#[derive(serde::Deserialize)]
struct UpdateDeviceConfigBody {
    #[serde(rename = "type")]
    output_type: Option<String>,
    name: Option<String>,
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    mixer_type: Option<String>,
    #[serde(default)]
    mixer_device: Option<String>,
    #[serde(default)]
    dop: Option<bool>,
}

#[derive(serde::Deserialize)]
struct CreateDeviceConfigBody {
    #[serde(rename = "type")]
    output_type: String,
    name: String,
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    mixer_type: Option<String>,
    #[serde(default)]
    mixer_device: Option<String>,
    #[serde(default)]
    dop: bool,
}

/// List all managed device config fragments.
async fn list_device_configs(State(s): State<AppState>) -> AppResult<Json<Vec<DeviceConfigResponse>>> {
    let configs = s.device_configs().list();
    let pending = s.config_restart_pending();
    let cfg = s.config().await;
    let include_warning = cfg.mpd_config.is_none();
    let resp: Vec<DeviceConfigResponse> = configs
        .into_iter()
        .map(|c| DeviceConfigResponse {
            name: c.name,
            output_type: c.output_type,
            device: c.device,
            format: c.format,
            mixer_type: c.mixer_type,
            mixer_device: c.mixer_device,
            dop: c.dop,
            restart_pending: pending,
            include_warning: if include_warning { Some(true) } else { None },
        })
        .collect();
    Ok(Json(resp))
}

/// Create a new device config fragment.
async fn create_device_config(
    State(s): State<AppState>,
    Json(b): Json<CreateDeviceConfigBody>,
) -> AppResult<Json<DeviceConfigResponse>> {
    let validation = validate_config(
        &b.name, &b.output_type,
        b.device.as_deref(),
        b.format.as_deref(),
        b.mixer_type.as_deref(),
        b.mixer_device.as_deref(),
        b.dop,
    );
    if !validation.is_valid() {
        return Err(AppError::Unprocessable(validation.into_error_string().unwrap_or_default()));
    }

    let cfg = DeviceConfigDto {
        name: b.name.clone(),
        output_type: b.output_type.clone(),
        device: b.device.clone(),
        format: b.format.clone(),
        mixer_type: b.mixer_type.clone(),
        mixer_device: b.mixer_device.clone(),
        mixer_control: None,
        dop: b.dop,
    };
    s.device_configs().create(&cfg).map_err(|e| AppError::BadRequest(e.to_string()))?;
    s.set_config_restart_pending(true);
    restart_mpd_if_pending(&s).await?;

    // The installer owns the system MPD config and provisions this include.
    // Do not attempt to rewrite it from the unprivileged backend process.
    let include_warning = s.config().await.mpd_config.is_none();

    Ok(Json(DeviceConfigResponse {
        name: b.name,
        output_type: b.output_type,
        device: b.device,
        format: b.format,
        mixer_type: b.mixer_type,
        mixer_device: b.mixer_device,
        dop: b.dop,
        restart_pending: false,
        include_warning: if include_warning { Some(true) } else { None },
    }))
}

/// Update an existing device config fragment.
async fn update_device_config(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<UpdateDeviceConfigBody>,
) -> AppResult<Json<DeviceConfigResponse>> {
    // Read existing config
    let existing = s.device_configs().get(&name)
        .map_err(|_| AppError::NotFound(format!("device config '{name}'")))?;

    let new_name = b.name.unwrap_or_else(|| existing.name.clone());
    let output_type = b.output_type.unwrap_or_else(|| existing.output_type.clone());
    let device = b.device.or_else(|| existing.device.clone());
    let format = b.format.or_else(|| existing.format.clone());
    let mixer_type = b.mixer_type.or_else(|| existing.mixer_type.clone());
    let mixer_device = b.mixer_device.or_else(|| existing.mixer_device.clone());
    let dop = b.dop.unwrap_or(existing.dop);

    let validation = validate_config(
        &new_name, &output_type,
        device.as_deref(),
        format.as_deref(),
        mixer_type.as_deref(),
        mixer_device.as_deref(),
        dop,
    );
    if !validation.is_valid() {
        return Err(AppError::Unprocessable(validation.into_error_string().unwrap_or_default()));
    }

    let cfg = DeviceConfigDto {
        name: new_name.clone(),
        output_type: output_type.clone(),
        device: device.clone(),
        format: format.clone(),
        mixer_type: mixer_type.clone(),
        mixer_device: mixer_device.clone(),
        mixer_control: existing.mixer_control.clone(),
        dop,
    };
    s.device_configs().update(&name, &cfg).map_err(|e| AppError::BadRequest(e.to_string()))?;
    s.set_config_restart_pending(true);
    restart_mpd_if_pending(&s).await?;

    Ok(Json(DeviceConfigResponse {
        name: new_name,
        output_type,
        device,
        format,
        mixer_type,
        mixer_device,
        dop,
        restart_pending: false,
        include_warning: None,
    }))
}

/// Delete a device config fragment.
async fn delete_device_config(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    s.device_configs().delete(&name).map_err(|_| {
        AppError::NotFound(format!("device config '{name}'"))
    })?;
    s.set_config_restart_pending(true);
    restart_mpd_if_pending(&s).await?;
    Ok(StatusCode::OK)
}

async fn restart_mpd_now(s: &AppState) -> AppResult<()> {
    let cfg = s.config().await;
    let host = cfg.mpd_host.clone();
    drop(cfg);
    if !is_localhost(&host) {
        return Err(AppError::BadRequest(
            "cannot restart remote MPD — MPD must be running on the same machine as oxide-player".to_string()
        ));
    }
    // Kill MPD — the connection may close before the response arrives,
    // so ignore that command error and wait for the process to exit.
    let _ = s.mpd().raw(mpd_protocol::command::Command::new("kill")).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    s.mpd().ensure_running().await?;
    s.set_config_restart_pending(false);
    s.refresh_status().await;
    Ok(())
}

/// Restart MPD after a managed audio output fragment changed.
pub(super) async fn restart_mpd_if_pending(s: &AppState) -> AppResult<()> {
    if s.config_restart_pending() {
        restart_mpd_now(s).await?;
    }
    Ok(())
}

/// Restart MPD (only supported when MPD is running on localhost).
async fn restart_mpd(State(s): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    restart_mpd_now(&s).await?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

/// True when the host refers to the local machine (not a remote address).
fn is_localhost(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost" | "0.0.0.0")
}

const MAX_DSP_IMPORT_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
struct DspImportBody {
    text: Option<String>,
    url: Option<String>,
}

async fn dsp_import(Json(body): Json<DspImportBody>) -> AppResult<Json<DspSettings>> {
    let text = match (body.text, body.url) {
        (Some(text), None) => text,
        (None, Some(url)) => fetch_dsp_import_url(&url).await?,
        (Some(_), Some(_)) => {
            return Err(AppError::BadRequest(
                "provide either DSP import text or a URL, not both".to_string(),
            ))
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "DSP import text or URL is required".to_string(),
            ))
        }
    };
    parse_dsp_text(&text)
        .map(Json)
        .map_err(|e| AppError::BadRequest(e.to_string()))
}

async fn fetch_dsp_import_url(url: &str) -> AppResult<String> {
    let url = url.trim();
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| AppError::BadRequest(format!("invalid DSP import URL: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "DSP import URL must use http or https".to_string(),
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("oxide-player/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AppError::BadRequest(format!("cannot prepare DSP import request: {e}")))?;
    let response = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("DSP import URL request failed: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "DSP import URL returned HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DSP_IMPORT_BYTES as u64)
    {
        return Err(AppError::BadRequest(format!(
            "DSP import is too large (maximum is {MAX_DSP_IMPORT_BYTES} bytes)"
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("cannot read DSP import URL response: {e}")))?;
    if bytes.len() > MAX_DSP_IMPORT_BYTES {
        return Err(AppError::BadRequest(format!(
            "DSP import is too large (maximum is {MAX_DSP_IMPORT_BYTES} bytes)"
        )));
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::BadRequest("DSP import must be UTF-8 text".to_string()))
}

async fn dsp_get(State(s): State<AppState>) -> AppResult<Json<Vec<DspProfile>>> {
    Ok(Json(s.dsp().list_profiles().await))
}

async fn dsp_set(State(s): State<AppState>, Json(p): Json<DspProfile>) -> AppResult<Json<DspApplyResult>> {
    p.validate().map_err(|e| AppError::BadRequest(e.to_string()))?;
    s.persist_dsp_profile(&p)
        .await
        .map_err(|e| AppError::Library(e.to_string()))?;

    let device = p.device.clone();
    let result = if let Some((target, device, loopback)) =
        dsp_profile_route_target(&s, &device).await?
    {
        apply_dsp_route(&s, target, device, loopback, Some(p)).await?
    } else {
        s.dsp()
            .apply_profile(p)
            .await
            .map_err(|e| AppError::Dsp(e.to_string()))?
    };
    Ok(Json(result))
}

async fn playlists(State(s): State<AppState>) -> AppResult<Json<Vec<String>>> {
    Ok(Json(s.mpd().list_playlists().await?))
}

#[derive(Deserialize)]
struct SavePlaylistBody {
    name: String,
}

async fn save_playlist(
    State(s): State<AppState>,
    Json(b): Json<SavePlaylistBody>,
) -> AppResult<StatusCode> {
    validate_playlist_name(&b.name)?;
    s.mpd().save_playlist(&b.name).await?;
    Ok(StatusCode::OK)
}

/// A single track reference, optionally with a CUE `[start, end)` range.
#[derive(Deserialize)]
struct TrackRef {
    uri: String,
    start: Option<f64>,
    end: Option<f64>,
    track_id: Option<i64>,
}

/// Accept either a `{ tracks: ... }` envelope, a bare array of track objects,
/// or a single track object, so the same handlers back both per-track and
/// whole-album actions and stay consistent across endpoints.
fn into_tracks(b: serde_json::Value) -> Vec<TrackRef> {
    let value = match b.get("tracks") {
        Some(inner) => inner.clone(),
        None => b,
    };
    if let Some(arr) = value.as_array() {
        arr.iter()
            .filter_map(|v| serde_json::from_value::<TrackRef>(v.clone()).ok())
            .collect()
    } else {
        serde_json::from_value::<TrackRef>(value)
            .ok()
            .into_iter()
            .collect()
    }
}


/// MPD playback uses the absolute filesystem path stored in the library DB.
/// This supports library roots that have no common parent. Local MPD must be
/// reached through its Unix socket because MPD rejects absolute paths over TCP.
///
/// For CUE-split tracks MPD exposes each split as `<file>.cue/trackNNNN`, so we
/// return that and signal that no start/end offset should be applied (the split
/// already isolates the track).
async fn resolve_play_uri(
    s: &AppState,
    uri: &str,
    track_id: Option<i64>,
) -> AppResult<(String, bool)> {
    let track = track_id.and_then(|id| s.db().track_by_id(id).ok().flatten());
    let (path, cue_index) = match &track {
        Some(t) => (Some(t.path.clone()), t.cue_index),
        None => (s.db().path_for_uri(uri).ok().flatten(), None),
    };

    let abs = match path {
        Some(p) => p,
        None => return Ok((uri.to_string(), false)),
    };

    // CUE virtual files are addressable only as a path relative to MPD's
    // music_directory (`<dir>/<stem>.cue/trackNNNN`), not as an absolute
    // filesystem path: MPD interprets the absolute form as a real file and
    // ENotADirectory-fails on the trailing `/trackNNNN` (see report:
    // "...Trio Toykeat.cue/track0009: Not a directory").
    // Derive that relative form from the library source that contains the
    // track (longest matching library_dir).
    if let Some(cue) = cue_index {
        let cfg = s.config().await;
        let abs_path = std::path::Path::new(&abs);
        let rel = cfg
            .library_dirs
            .iter()
            .filter_map(|dir| abs_path.strip_prefix(dir).ok())
            .max_by_key(|rel| rel.components().count())
            .map(|rel| rel.to_string_lossy().to_string());
        if let Some(rel) = rel {
            match std::path::Path::new(&rel).file_stem() {
                Some(stem) => {
                    let stem = stem.to_string_lossy().to_string();
                    let parent = std::path::Path::new(&rel)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let cue_uri = if parent.is_empty() {
                        format!("{stem}.cue/track{cue:04}")
                    } else {
                        format!("{parent}/{stem}.cue/track{cue:04}")
                    };
                    return Ok((cue_uri, true));
                }
                None => return Ok((rel, false)),
            }
        }
        // No library root matched — fall back to absolute (will likely fail,
        // but avoids synthesizing a bogus relative path).
        return Ok((abs, true));
    }
    Ok((abs, false))
}

/// active (highlighted) track. Uses the DB track id from the request; never the
/// MPD song id, which is a different namespace (see AGENTS.md).
async fn enqueue(s: &AppState, t: &TrackRef) -> AppResult<()> {
    let (mpd_uri, is_cue) = resolve_play_uri(s, &t.uri, t.track_id).await?;
    if is_cue {
        s.mpd().play_next(&mpd_uri, 0.0, None).await?;
    } else {
        s.mpd().play_next(&mpd_uri, t.start.unwrap_or(0.0), t.end).await?;
    }
    s.mpd().set_active_track(t.track_id).await;
    Ok(())
}

async fn play_next(State(s): State<AppState>, Json(b): Json<serde_json::Value>) -> AppResult<StatusCode> {
    let tracks = into_tracks(b);
    if tracks.is_empty() {
        return Ok(StatusCode::OK);
    }
    // Insert after the current song. Because each insert lands right after the
    // current track, iterate in reverse so the resulting order matches the
    // requested order (otherwise the queue would be reversed).
    for t in tracks.iter().rev() {
        enqueue(&s, t).await?;
    }
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

async fn clear_play(State(s): State<AppState>, Json(b): Json<serde_json::Value>) -> AppResult<StatusCode> {
    let tracks = into_tracks(b);
    if tracks.is_empty() {
        return Ok(StatusCode::OK);
    }
    s.mpd().clear().await?;
    // First track becomes the new queue; play it. Remaining ones are appended in
    // order after it (reverse-inserted for the same ordering reason as above).
    let first = &tracks[0];
    let (mpd_uri, is_cue) = resolve_play_uri(&s, &first.uri, first.track_id).await?;
    if is_cue {
        s.mpd().play_uri(&mpd_uri).await?;
    } else {
        s.mpd().play_uri_range(&mpd_uri, first.start.unwrap_or(0.0), first.end).await?;
    }
    s.mpd().set_active_track(first.track_id).await;
    for t in tracks[1..].iter().rev() {
        enqueue(&s, t).await?;
    }
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct PlaylistAddBody {
    tracks: serde_json::Value,
}

async fn playlist_add(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<PlaylistAddBody>,
) -> AppResult<StatusCode> {
    require_playlist(&s, &name).await?;
    for t in into_tracks(b.tracks) {
        let (mpd_uri, _) = resolve_play_uri(&s, &t.uri, t.track_id).await?;
        s.mpd().add_to_playlist(&name, &mpd_uri).await?;
    }
    Ok(StatusCode::OK)
}

/// 404 unless `name` is an existing saved playlist.
async fn require_playlist(s: &AppState, name: &str) -> AppResult<()> {
    let lists = s.mpd().list_playlists().await?;
    if !lists.iter().any(|l| l == name) {
        return Err(AppError::NotFound(format!("playlist '{name}'")));
    }
    Ok(())
}

/// Reject names the `/api/playlists/{name}` routes cannot address (a `/`
/// splits the single path segment) or that are empty/whitespace.
fn validate_playlist_name(name: &str) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("playlist name is empty".into()));
    }
    if trimmed.contains('/') {
        return Err(AppError::BadRequest("playlist name must not contain '/'".into()));
    }
    Ok(())
}

async fn playlist_tracks(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Json<Vec<crate::types::QueueEntry>>> {
    require_playlist(&s, &name).await?;
    Ok(Json(s.mpd().playlist_tracks(&name).await?))
}

async fn playlist_play(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    require_playlist(&s, &name).await?;
    s.mpd().play_playlist(&name).await?;
    // Refresh the cached status so the UI reflects the new queue immediately.
    let _ = s.refresh_status().await;
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct PlaylistRemoveBody {
    pos: u32,
}

async fn playlist_remove(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<PlaylistRemoveBody>,
) -> AppResult<StatusCode> {
    require_playlist(&s, &name).await?;
    s.mpd().remove_from_playlist(&name, b.pos).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct PlaylistRenameBody {
    new_name: String,
}

async fn playlist_rename(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<PlaylistRenameBody>,
) -> AppResult<StatusCode> {
    require_playlist(&s, &name).await?;
    validate_playlist_name(&b.new_name)?;
    let new_name = b.new_name.trim();
    if new_name != name {
        let lists = s.mpd().list_playlists().await?;
        if lists.iter().any(|l| l == new_name) {
            return Err(AppError::BadRequest(format!(
                "playlist '{new_name}' already exists"
            )));
        }
    }
    s.mpd().rename_playlist(&name, new_name).await?;
    Ok(StatusCode::OK)
}

async fn playlist_delete(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    require_playlist(&s, &name).await?;
    s.mpd().delete_playlist(&name).await?;
    Ok(StatusCode::OK)
}

async fn queue(State(s): State<AppState>) -> AppResult<Json<crate::types::QueueResponse>> {
    // One-off REST fetch (fallback for non-WS clients). The live push is driven
    // by the broadcast on connect + the queue-mutating endpoints, so this must
    // NOT broadcast again or a single mutation would emit Queue twice.
    let resp = s.queue_snapshot().await;
    Ok(Json(resp))
}

#[derive(Deserialize)]
struct ShuffleBody {
    on: bool,
}

async fn shuffle_queue(State(s): State<AppState>, Json(b): Json<ShuffleBody>) -> AppResult<StatusCode> {
    s.mpd().random(b.on).await?;
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct JumpBody {
    pos: u32,
}

async fn jump(State(s): State<AppState>, Json(b): Json<JumpBody>) -> AppResult<StatusCode> {
    s.mpd().play_position(b.pos).await?;
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RemoveBody {
    pos: u32,
}

async fn remove(State(s): State<AppState>, Json(b): Json<RemoveBody>) -> AppResult<StatusCode> {
    s.mpd().delete_position(b.pos).await?;
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

async fn clear_queue(State(s): State<AppState>) -> AppResult<StatusCode> {
    let entries = s.mpd().queue().await?;
    // Skip the currently playing/paused song so playback is uninterrupted.
    // When nothing is current, every entry is removed.
    let current_pos = s.current_pos(&entries).await;
    // Delete from the highest position down so earlier indices stay valid.
    for pos in (0..entries.len() as u32).rev() {
        if Some(pos) == current_pos {
            continue;
        }
        s.mpd().delete_position(pos).await?;
    }
    s.broadcast_queue_now().await;
    Ok(StatusCode::OK)
}

// ---- Radio stations ---- ///

#[derive(Deserialize)]
struct AddRadioBody {
    name: String,
    url: String,
    homepage: Option<String>,
    artwork: Option<String>,
}

#[derive(Deserialize)]
struct UpdateRadioBody {
    name: String,
    artwork: Option<String>,
}

/// List all user-managed radio stations.
async fn radio_list(State(s): State<AppState>) -> AppResult<Json<Vec<RadioStation>>> {
    Ok(Json(s.radio().list()))
}

/// Add a radio station (validated: non-empty name, http(s) URL, no dupes).
async fn radio_add(
    State(s): State<AppState>,
    Json(b): Json<AddRadioBody>,
) -> AppResult<(StatusCode, Json<RadioStation>)> {
    let station = s.radio().add(&b.name, &b.url, b.homepage, b.artwork)?;
    Ok((StatusCode::CREATED, Json(station)))
}

/// Update a radio station's display name and artwork.
async fn radio_update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<UpdateRadioBody>,
) -> AppResult<Json<RadioStation>> {
    Ok(Json(s.radio().update(&id, &b.name, b.artwork)?))
}

/// Remove a radio station by id.
async fn radio_delete(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    s.radio().remove(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Replace the play queue with the station's stream and start playing it.
/// Mirrors `clear_play` (clear → add → play) but the station URL is the MPD
/// URI itself, so `resolve_play_uri` is bypassed.
async fn radio_play(State(s): State<AppState>, Path(id): Path<String>) -> AppResult<StatusCode> {
    let station = s

        .radio()
        .get(&id)
        .ok_or_else(|| AppError::NotFound(format!("radio station {id}")))?;
    s.mpd().clear().await?;
    s.mpd().play_uri(&station.url).await?;
    s.mpd().set_active_track(None).await;
    s.broadcast_queue_now().await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::devices::config_fragment::DeviceConfig;
    use crate::dsp::DspManager;
    use crate::library::LibraryDb;
    use crate::mpd::Mpd;
    use crate::state::AppState;
    use crate::types::OutputDevice;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_app() -> axum::Router {
        test_app_with_config(Config::default_config()).await
    }

    async fn test_app_with_config(config: Config) -> axum::Router {
        let db = LibraryDb::open(std::path::Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        let dsp = DspManager::new(
            std::env::temp_dir().join("oxide_test_version_dsp.yaml"),
            None,
            "".to_string(),
            44100,
            false,
            None,
        );
        let mpd = Mpd::with_connection("127.0.0.1", 6600, false, None, None);
        let visualizer = crate::visualizer::VisualizerAnalyzer::new(&config);
        let bt = crate::bluetooth::BluetoothManager::new().await;
        let state = AppState::new(config, db, dsp, mpd, visualizer, bt, None);
        super::router(state).await
    }

    #[test]
    fn library_source_must_be_an_existing_directory() {
        let missing = std::env::temp_dir().join(format!(
            "oxide-library-source-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&missing);
        let error = super::canonical_library_dir(&missing).unwrap_err();
        assert!(matches!(error, crate::error::AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn version_endpoint_returns_single_version() {
        let app = test_app().await;
        let req = Request::builder()

            .uri("/api/version")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
        assert_eq!(
            v["version"],
            env!("CARGO_PKG_VERSION"),
            "version must match CARGO_PKG_VERSION"
        );
    }

    #[tokio::test]
    async fn dsp_import_endpoint_parses_text_settings() {
        let app = test_app().await;
        let request = Request::builder()
            .method("POST")
            .uri("/api/dsp/import")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "text": "Preamp: -0.7 dB\nFilter 1: ON PK Fc 1000 Hz Gain +2 dB Q 1.2\n"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["preamp"], -0.7);
        assert_eq!(parsed["eq_bands"][0]["type"], "peaking");
        assert_eq!(parsed["eq_bands"][0]["freq"], 1000.0);
    }

    #[tokio::test]
    async fn dsp_set_endpoint_persists_profile_for_restart() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = Config::default_config();
        config.data_dir = temp.path().join("data");
        config.camilladsp_config_path = temp.path().join("camilladsp/config.yml");
        let app = test_app_with_config(config).await;
        let request = Request::builder()
            .method("PUT")
            .uri("/api/dsp")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "device": "hw:USB,0",
                    "mode": "bit_perfect",
                    "target_rate": null,
                    "preset": "balanced",
                    "preamp": -6.5,
                    "eq_bands": []
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let saved = std::fs::read_to_string(temp.path().join("data/config.json")).unwrap();
        let saved: Config = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved.default_dsp_profiles.len(), 1);
        assert_eq!(saved.default_dsp_profiles[0].device, "hw:USB,0");
        assert_eq!(saved.default_dsp_profiles[0].preamp, -6.5);
    }

    #[tokio::test]
    async fn dsp_import_endpoint_fetches_url() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).await.unwrap();
            let body = b"Preamp: -2 dB\nFilter 1: ON PK Fc 800 Hz Gain +1 dB Q 1\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let app = test_app().await;
        let request = Request::builder()
            .method("POST")
            .uri("/api/dsp/import")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "url": format!("http://{address}/eq.txt")
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        server.await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["preamp"], -2.0);
        assert_eq!(parsed["eq_bands"][0]["freq"], 800.0);
    }
    #[tokio::test]
    async fn adding_and_removing_source_updates_shared_folders() {
        let root = std::env::temp_dir().join(format!(
            "oxide-library-share-api-{}",
            std::process::id()
        ));
        let first = root.join("first");
        let second = root.join("second");
        let data_dir = root.join("data");
        let static_dir = root.join("static");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::create_dir_all(&static_dir).unwrap();

        let mut config = Config::default_config();
        config.data_dir = data_dir.clone();
        config.static_dir = static_dir;
        config.library_dirs = vec![first.clone()];
        let app = test_app_with_config(config).await;

        let add = Request::builder()
            .method("POST")
            .uri("/api/config/library-dirs")

            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({ "path": second }).to_string()))
            .unwrap();
        let response = app.clone().oneshot(add).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let share_file = data_dir.join("smb-shares.conf");
        let shared = std::fs::read_to_string(&share_file).unwrap();
        assert!(shared.contains(second.to_str().unwrap()));

        let remove = Request::builder()
            .method("DELETE")
            .uri("/api/config/library-dirs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({ "path": second }).to_string()))
            .unwrap();
        let response = app.oneshot(remove).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let shared = std::fs::read_to_string(&share_file).unwrap();
        assert!(!shared.contains(second.to_str().unwrap()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn play_uri_uses_absolute_db_path() {
        let db = LibraryDb::open(std::path::Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        let track_id = db
            .insert_track(
                "Artist/Album.flac",
                "/music/Artist/Album.flac",
                Some("Album"),
                Some("Artist"),
                Some("Album"),
                None,
                None,
                None,
                Some(1),
                Some(180.0),
                Some("flac"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("/music"),
            )
            .unwrap();
        let config = Config::default_config();
        let dsp = DspManager::new(
            std::env::temp_dir().join("oxide_test_play_uri_dsp.yaml"),
            None,
            "".to_string(),
            44100,
            false,
            None,
        );
        let mpd = Mpd::with_connection("127.0.0.1", 6600, false, None, None);
        let visualizer = crate::visualizer::VisualizerAnalyzer::new(&config);
        let bt = crate::bluetooth::BluetoothManager::new().await;
        let state = AppState::new(config, db, dsp, mpd, visualizer, bt, None);

        let (uri, is_cue) = super::resolve_play_uri(
            &state,
            "Artist/Album.flac",
            Some(track_id),
        )
        .await
        .unwrap();

        assert_eq!(uri, "/music/Artist/Album.flac");
        assert!(!is_cue);
    }

    /// Regression: library sources may have unrelated roots. Playback must use
    /// the absolute path stored in the DB instead of deriving a URI from one
    /// assumed MPD music directory.
    #[tokio::test]
    async fn play_uri_preserves_deep_unrelated_library_path() {
        let db = LibraryDb::open(std::path::Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        let track_id = db
            .insert_track(
                "Lady Blackbird/Black Acid Soul/01 Blackbird.m4a",
                "/mnt/music1/Lady Blackbird/Black Acid Soul/01 Blackbird.m4a",
                Some("Blackbird"),
                Some("Lady Blackbird"),
                Some("Black Acid Soul"),
                None,
                None,
                None,
                Some(1),
                Some(180.0),
                Some("m4a"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some("/mnt/music1"),
            )
            .unwrap();
        let config = Config::default_config();
        let dsp = DspManager::new(
            std::env::temp_dir().join("oxide_test_play_uri_absolute_dsp.yaml"),
            None,
            "".to_string(),
            44100,
            false,
            None,
        );
        let mpd = Mpd::with_connection("127.0.0.1", 6600, false, None, None);
        let visualizer = crate::visualizer::VisualizerAnalyzer::new(&config);
        let bt = crate::bluetooth::BluetoothManager::new().await;
        let state = AppState::new(config, db, dsp, mpd, visualizer, bt, None);

        let (uri, is_cue) = super::resolve_play_uri(
            &state,
            "Lady Blackbird/Black Acid Soul/01 Blackbird.m4a",
            Some(track_id),
        )
        .await
        .unwrap();

        assert_eq!(
            uri,
            "/mnt/music1/Lady Blackbird/Black Acid Soul/01 Blackbird.m4a"
        );
        assert!(!is_cue);
    }
    /// Regression: CUE virtual file must be addressed as a relative URI
    /// (`<dir>/<stem>.cue/trackNNNN`) derived from its library source, not as
    /// an absolute filesystem path (MPD would ENotADirectory-fail on
    /// `/.../.cue/trackNNNN`). Reproduces the folder bulk-play report:
    /// `...Trio Toykeat.cue/track0009: Not a directory`.
    #[tokio::test]
    async fn cue_path_resolved_as_relative_mpd_uri() {
        let mut config = Config::default_config();
        config.library_dirs = vec![std::path::PathBuf::from("/mnt/music1")];
        let db = LibraryDb::open(std::path::Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        let track_id = db
            .insert_track(
                "Jazz/Trio Toykeat-One Night In Tampere/Trio Toykeat - One Night In Tampere.flac",
                "/mnt/music1/Jazz/Trio Toykeat-One Night In Tampere/Trio Toykeat - One Night In Tampere.flac",
                Some("One Night In Tampere"),
                Some("Trio Toykeat"),
                Some("One Night In Tampere"),
                None,
                None,
                None,
                Some(9),
                Some(180.0),
                Some("flac"),
                None,
                None,
                None,
                None,
                Some(9),
                Some(100.0),
                Some(120.0),
                None,
                Some("/mnt/music1"),
            )
            .unwrap();
        let dsp = DspManager::new(
            std::env::temp_dir().join("oxide_test_cue_relative.yaml"),
            None,
            "".to_string(),
            44100,
            false,
            None,
        );
        let mpd = Mpd::with_connection("127.0.0.1", 6600, false, None, None);
        let visualizer = crate::visualizer::VisualizerAnalyzer::new(&config);
        let bt = crate::bluetooth::BluetoothManager::new().await;
        let state = AppState::new(config, db, dsp, mpd, visualizer, bt, None);

        let (uri, is_cue) = super::resolve_play_uri(
            &state,
            "Jazz/Trio Toykeat-One Night In Tampere/Trio Toykeat - One Night In Tampere.flac",
            Some(track_id),
        )
        .await
        .unwrap();
        assert!(is_cue);
        assert_eq!(
            uri,
            "Jazz/Trio Toykeat-One Night In Tampere/Trio Toykeat - One Night In Tampere.cue/track0009"
        );
        assert!(
            !uri.starts_with('/'),
            "CUE MPD URI must be relative to music_directory, got {uri:?}"
        );
    }

    #[test]
    fn dsp_profile_target_requires_releasing_direct_usb_output() {
        let output = OutputDevice {
            id: 7,
            name: "USB DAC".to_string(),
            enabled: true,
        };
        let configs = [config("USB DAC", "alsa", Some("hw:USB,0"))];
        let target = super::dsp_profile_target(&[output], &configs, "hw:USB,0")
            .expect("USB profile should resolve to its managed output");
        assert_eq!(target.id, 7);
        assert!(super::should_release_direct_dsp_output(target.enabled, false));
    }

    #[test]
    fn dsp_route_waits_for_direct_output_release() {
        assert_eq!(
            super::direct_dsp_output_release_delay(),
            std::time::Duration::from_millis(250)
        );
    }

    fn config(name: &str, output_type: &str, device: Option<&str>) -> DeviceConfig {
        DeviceConfig {
            name: name.to_string(),
            output_type: output_type.to_string(),
            device: device.map(str::to_string),
            format: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        }
    }
    #[test]
    fn dsp_requires_a_configured_alsa_device() {
        let output = OutputDevice {
            id: 1,
            name: "USB DAC".to_string(),
            enabled: true,
        };
        assert_eq!(
            super::configured_dsp_device(&[config("USB DAC", "alsa", Some("hw:USB,0"))], &output)
                .unwrap(),
            "hw:USB,0"
        );
        assert!(super::configured_dsp_device(&[config("Pipe", "pipe", Some("/tmp/audio"))], &output).is_err());
        assert!(super::configured_dsp_device(&[], &output).is_err());
    }
    #[test]
    fn dsp_route_is_inferred_after_restart_from_saved_profile() {
        assert!(super::dsp_route_is_active(
            true,
            false,
            None,
            "hw:USB,0",
            true,
        ));
        assert!(!super::dsp_route_is_active(
            true,
            false,
            None,
            "hw:USB,0",
            false,
        ));
        assert!(!super::dsp_route_is_active(
            false,
            false,
            None,
            "hw:USB,0",
            true,
        ));
    }

    #[test]
    fn dsp_loopback_cannot_be_selected_as_playback_target() {
        let output = OutputDevice {
            id: 1,
            name: "camilladsp-loopback".to_string(),
            enabled: true,
        };
        assert!(super::configured_dsp_device(&[], &output).is_err());
    }

}
