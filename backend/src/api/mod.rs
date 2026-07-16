use crate::dsp::profile::DspProfile;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::{PlaybackState, Track};
use axum::extract::{Path, Query, Request, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use tower_http::services::ServeDir;
use serde::Deserialize;

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
        .route("/api/playback/shuffle", post(shuffle_queue))
        .route("/api/playback/jump", post(jump))
        .route("/api/playback/remove", post(remove))
        .route("/api/playback/clear-queue", post(clear_queue))
        .route("/api/devices", get(devices))
        .route("/api/devices/{id}/enable", post(enable_device))
        .route("/api/devices/{id}/disable", post(disable_device))
        .route("/api/dsp", get(dsp_get))
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
        .route("/api/config", get(config_get))
        .route("/api/config", put(config_put))
        .route("/api/config/library-dirs", post(config_add_dir))
        .route("/api/config/library-dirs", delete(config_remove_dir))
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

#[derive(Deserialize)]
struct LibraryQuery {
    q: Option<String>,
    artist: Option<String>,
    album: Option<String>,
}

async fn library_list(
    State(s): State<AppState>,
    Query(q): Query<LibraryQuery>,
) -> AppResult<Json<Vec<Track>>> {
    Ok(Json(s.db().search(q.q.as_deref(), q.artist.as_deref(), q.album.as_deref(), None)?))
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
            return Ok(([(header::CONTENT_TYPE, ct)], bytes).into_response());
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
    let count = tokio::task::spawn_blocking(move || crate::library::scan(&dirs, &db, &cover_dir))
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

/// Serialized view of the current config for the Settings UI.
async fn config_get(State(s): State<AppState>) -> AppResult<Json<crate::config::Config>> {
    Ok(Json(s.config().await))
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

/// Add a music library source folder. Must be an absolute path. Persists and
/// immediately rescans so the new source is picked up without a restart.
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
    let path = std::path::PathBuf::from(&b.path);
    if !path.is_absolute() {
        return Err(AppError::BadRequest(
            "library dir must be an absolute path".to_string(),
        ));
    }
    let mut cfg = s.config().await;
    // Dedupes against existing sources (rejects child-of-existing and exact
    // duplicates; drops child sources when `path` is their parent).
    let subsumed = cfg.add_library_dir(path);
    if subsumed.is_none() {
        return Ok(Json(serde_json::json!({ "scanned": 0, "duplicate": true })));
    }
    let subsumed = subsumed.unwrap();
    let mut removed_tracks = 0u64;
    if !subsumed.is_empty() {
        let db = s.db().clone();
        for d in &subsumed {
            removed_tracks += db.delete_by_source(d).map_err(|e| AppError::Library(e.to_string()))?;
        }
    }
    s.set_config(cfg).await
        .map_err(|e| AppError::Library(e.to_string()))?;
    let count = run_scan(&s, true).await?;
    Ok(Json(serde_json::json!({ "scanned": count, "removed": removed_tracks })))
}

/// Remove a music library source folder by absolute path. Drops every track
/// that came from that source so its albums leave the library (issue #46), then
/// keeps MPD's index in sync.
async fn config_remove_dir(
    State(s): State<AppState>,
    Json(b): Json<DirBody>,
) -> AppResult<StatusCode> {
    let path = std::path::PathBuf::from(&b.path);
    let mut cfg = s.config().await;
    let before = cfg.library_dirs.len();
    cfg.library_dirs.retain(|d| d != &path);
    if cfg.library_dirs.len() == before {
        return Err(AppError::NotFound(format!("library dir {}", path.display())));
    }
    s.set_config(cfg).await
        .map_err(|e| AppError::Library(e.to_string()))?;
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
    let cover_dir = s.config().await.cover_cache_dir();
    std::fs::create_dir_all(&cover_dir).map_err(|e| AppError::Library(e.to_string()))?;
    let with_cover =
        tokio::task::spawn_blocking(move || crate::library::scanner::rescan_art(&db, &cover_dir))
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
            let (mpd_uri, is_cue) = resolve_play_uri(&s, &uri, b.track_id).await;
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

async fn devices(State(s): State<AppState>) -> AppResult<Json<Vec<crate::types::OutputDevice>>> {
    Ok(Json(s.mpd().outputs().await?))
}

async fn enable_device(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    s.mpd().enable_output(id).await?;
    let _ = s.mpd().clear_error().await;
    Ok(StatusCode::OK)
}

async fn disable_device(State(s): State<AppState>, Path(id): Path<u32>) -> AppResult<StatusCode> {
    s.mpd().disable_output(id).await?;
    let _ = s.mpd().clear_error().await;
    Ok(StatusCode::OK)
}

async fn dsp_get(State(s): State<AppState>) -> AppResult<Json<Vec<DspProfile>>> {
    Ok(Json(s.dsp().list_profiles().await))
}

async fn dsp_set(State(s): State<AppState>, Json(p): Json<DspProfile>) -> AppResult<StatusCode> {
    p.validate().map_err(|e| AppError::BadRequest(e.to_string()))?;
    s.dsp()
        .apply_profile(p)
        .await
        .map_err(|e| AppError::Dsp(e.to_string()))?;
    Ok(StatusCode::OK)
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

/// Resolve a track to the URI MPD can actually `add`.
///
/// MPD addresses tracks by a path *relative to its `music_directory`* (e.g.
/// `MyMusic/Artist/Album.flac`), not by absolute OS paths. We convert the
/// library DB's absolute `path` accordingly when `mpd_music_directory` is
/// configured. For CUE-split tracks MPD exposes each split as
/// `<file>.cue/trackNNNN`, so we return that and signal that no start/end
/// offset should be applied (the split already isolates the track).
async fn resolve_play_uri(s: &AppState, uri: &str, track_id: Option<i64>) -> (String, bool) {
    let cfg = s.config().await;
    let music_dir = cfg.mpd_music_directory.as_deref();

    // Prefer the DB row for this exact track so we get its `path` and, for CUE,
    // its `cue_index`.
    let track = track_id.and_then(|id| s.db().track_by_id(id).ok().flatten());
    let (path, cue_index) = match &track {
        Some(t) => (Some(t.path.clone()), t.cue_index),
        None => (s.db().path_for_uri(uri).ok().flatten(), None),
    };

    let abs = match path {
        Some(p) => p,
        None => return (uri.to_string(), false),
    };

    let rel = match music_dir {
        Some(dir) => match std::path::Path::new(&abs).strip_prefix(dir) {
            Ok(rel) => rel.to_string_lossy().to_string(),
            Err(_) => {
                tracing::warn!(
                    "mpd_music_directory ({}) is not a prefix of track path ({}); \
                     passing the absolute path through, MPD add may fail",
                    dir.display(),
                    abs
                );
                abs.clone()
            }
        },
        None => abs.clone(),
    };

    let result = if let Some(cue) = cue_index {
        // MPD represents each CUE split as `<file>.cue/trackNNNN`, where `<file>`
        // is the *audio file's stem* (extension dropped), not the full path.
        // e.g. `Album.flac` -> `Album.cue/track0001`.
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
                (cue_uri, true)
            }
            // No usable stem (e.g. a path that is all extension) — fall back to
            // the plain relative URI rather than building a malformed CUE URI.
            None => (rel, false),
        }
    } else {
        (rel, false)
    };
    result
}

/// Insert `t` immediately after the currently playing song and record it as the
/// active (highlighted) track. Uses the DB track id from the request; never the
/// MPD song id, which is a different namespace (see AGENTS.md).
async fn enqueue(s: &AppState, t: &TrackRef) -> AppResult<()> {
    let (mpd_uri, is_cue) = resolve_play_uri(s, &t.uri, t.track_id).await;
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
    let (mpd_uri, is_cue) = resolve_play_uri(&s, &first.uri, first.track_id).await;
    if is_cue {
        s.mpd().play_uri(&mpd_uri).await?;
    } else {
        s.mpd().play_uri_range(&mpd_uri, first.start.unwrap_or(0.0), first.end).await?;
    }
    s.mpd().set_active_track(first.track_id).await;
    for t in tracks[1..].iter().rev() {
        enqueue(&s, t).await?;
    }
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
        let (mpd_uri, _) = resolve_play_uri(&s, &t.uri, t.track_id).await;
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

/// Position of the current song within `entries`, or `None` when nothing is
/// playing. Uses the cached current song id from the 1s status poller instead
/// of a second MPD round-trip. `QueueEntry.id` and `current_id` are both MPD
/// SongIds (not DB track ids), so the comparison is sound.
async fn current_pos(s: &AppState, entries: &[crate::types::QueueEntry]) -> Option<u32> {
    match s.status_snapshot().await.current_id {
        Some(id) => entries.iter().position(|e| e.id == id).map(|p| p as u32),
        None => None,
    }
}

async fn queue(State(s): State<AppState>) -> AppResult<Json<crate::types::QueueResponse>> {
    let entries = s.mpd().queue().await?;
    let current_pos = current_pos(&s, &entries).await;
    Ok(Json(crate::types::QueueResponse { entries, current: current_pos }))
}

#[derive(Deserialize)]
struct ShuffleBody {
    on: bool,
}

async fn shuffle_queue(State(s): State<AppState>, Json(b): Json<ShuffleBody>) -> AppResult<StatusCode> {
    s.mpd().random(b.on).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct JumpBody {
    pos: u32,
}

async fn jump(State(s): State<AppState>, Json(b): Json<JumpBody>) -> AppResult<StatusCode> {
    s.mpd().play_position(b.pos).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RemoveBody {
    pos: u32,
}

async fn remove(State(s): State<AppState>, Json(b): Json<RemoveBody>) -> AppResult<StatusCode> {
    s.mpd().delete_position(b.pos).await?;
    Ok(StatusCode::OK)
}

async fn clear_queue(State(s): State<AppState>) -> AppResult<StatusCode> {
    let entries = s.mpd().queue().await?;
    // Skip the currently playing/paused song so playback is uninterrupted.
    // When nothing is current, every entry is removed.
    let current_pos = current_pos(&s, &entries).await;
    // Delete from the highest position down so earlier indices stay valid.
    for pos in (0..entries.len() as u32).rev() {
        if Some(pos) == current_pos {
            continue;
        }
        s.mpd().delete_position(pos).await?;
    }
    Ok(StatusCode::OK)
}
