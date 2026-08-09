use crate::error::{AppError, AppResult};
use crate::types::Track;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

const TRACK_COLS: &str = "id, uri, path, title, artist, album, album_artist, genre, year, \
track, duration, format, sample_rate, bit_depth, channels, has_cover, cover_key, cue_index, \
start_time, end_time, file_mtime, source";
// Qualified form used when joining `tracks_fts` (both tables expose
// title/artist/album and an unqualified reference would be ambiguous).
const TRACK_COLS_Q: &str = "tracks.id, tracks.uri, tracks.path, tracks.title, tracks.artist, \
tracks.album, tracks.album_artist, tracks.genre, tracks.year, tracks.track, tracks.duration, \
tracks.format, tracks.sample_rate, tracks.bit_depth, tracks.channels, tracks.has_cover, \
tracks.cover_key, tracks.cue_index, tracks.start_time, tracks.end_time, tracks.file_mtime, \
tracks.source";

#[derive(Debug)]
pub enum LibrarySnapshot {
    NotModified { etag: String },
    Fresh { etag: String, tracks: Vec<Track> },
}

#[derive(Clone)]
pub struct LibraryDb {
    conn: Arc<Mutex<Connection>>,
    revision: Arc<AtomicU64>,
    nonce: u64,
}

static NEXT_NONCE: AtomicU64 = AtomicU64::new(1);

fn process_nonce() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    now ^ ((std::process::id() as u64) << 32) ^ NEXT_NONCE.fetch_add(1, Ordering::Relaxed)
}


fn if_none_match_matches(header: &str, current: &str) -> bool {
    let current = current.strip_prefix("W/").unwrap_or(current).trim();
    header.trim() == "*"
        || header.split(',').any(|candidate| {
            let candidate = candidate.trim();
            candidate.strip_prefix("W/").unwrap_or(candidate).trim() == current
        })
}

fn path_tail_matches(left: &str, right: &str) -> bool {
    let left = left.trim_start_matches('/');
    let right = right.trim_start_matches('/');
    left == right || left.ends_with(&format!("/{right}")) || right.ends_with(&format!("/{left}"))
}
impl LibraryDb {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Library(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| AppError::Library(e.to_string()))?;
        Ok(LibraryDb {
            conn: Arc::new(Mutex::new(conn)),
            revision: Arc::new(AtomicU64::new(0)),
            nonce: process_nonce(),
        })
    }

    fn etag(&self, revision: u64) -> String {
        format!("\"oxide-{:#x}-{revision:#x}\"", self.nonce)
    }

    fn bump_revision(&self) {
        self.revision.fetch_add(1, Ordering::Release);
    }

    pub fn unfiltered_snapshot(
        &self,
        if_none_match: Option<&str>,
    ) -> AppResult<LibrarySnapshot> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let revision = self.revision.load(Ordering::Acquire);
        let etag = self.etag(revision);
        if if_none_match.is_some_and(|header| if_none_match_matches(header, &etag)) {
            return Ok(LibrarySnapshot::NotModified { etag });
        }
        let tracks = self.search_with_conn(&conn, None, None, None, None)?;
        Ok(LibrarySnapshot::Fresh { etag, tracks })
    }

    pub fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY,
                uri TEXT NOT NULL,
                path TEXT NOT NULL,
                title TEXT,
                artist TEXT,
                album TEXT,
                album_artist TEXT,
                genre TEXT,
                year INTEGER,
                track INTEGER,
                duration REAL,
                format TEXT,
                sample_rate INTEGER,
                bit_depth INTEGER,
                channels INTEGER,
                has_cover INTEGER NOT NULL DEFAULT 0,
                cover_key TEXT,
                cue_index INTEGER,
                start_time REAL,
                end_time REAL,
                file_mtime INTEGER,
                source TEXT,
                UNIQUE(uri, cue_index)
            );
            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            -- SQLite treats NULLs as distinct in a UNIQUE constraint, so the
            -- (uri, cue_index) key alone let incremental rescans insert
            -- duplicate non-CUE rows. This partial index keeps at most one
            -- non-CUE (cue_index IS NULL) row per uri, so INSERT OR REPLACE
            -- dedupes on rescan while CUE tracks keep their own key.
            CREATE UNIQUE INDEX IF NOT EXISTS idx_tracks_uri_noncue
                ON tracks(uri) WHERE cue_index IS NULL;",
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
        // FTS5 index over the searchable text columns. External-content table
        // backed by `tracks`, kept in sync by triggers below, so every
        // `insert_track`/`prune` automatically updates the index without a
        // separate write path. Prefix queries ('beat*') use the index instead
        // of the previous full-table `LIKE '%q%'` scan.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
                title, artist, album, album_artist,
                content='tracks', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
            );
            CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, title, artist, album, album_artist)
                VALUES (new.id, new.title, new.artist, new.album, new.album_artist);
            END;
            CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist)
                VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist);
            END;
            CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist)
                VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist);
                INSERT INTO tracks_fts(rowid, title, artist, album, album_artist)
                VALUES (new.id, new.title, new.artist, new.album, new.album_artist);
            END;",
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
        // Rebuild the FTS index when it's out of sync with `tracks` (e.g. an
        // existing DB created before the index existed, or a recovery).
        let fts_count: i64 = conn
            .query_row("SELECT count(*) FROM tracks_fts", [], |r| r.get(0))
            .unwrap_or(0);
        let tracks_count: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap_or(0);
        if fts_count != tracks_count {
            conn.execute_batch(
                "INSERT INTO tracks_fts(tracks_fts) VALUES('rebuild');",
            )
            .map_err(|e| AppError::Library(e.to_string()))?;
        }
        // Idempotent schema upgrades (only on pre-existing tables).
        for (col, col_ty) in [
            ("cue_index", "INTEGER"),
            ("start_time", "REAL"),
            ("end_time", "REAL"),
            ("file_mtime", "INTEGER"),
            ("cover_key", "TEXT"),
            ("source", "TEXT"),
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('tracks') WHERE name = ?",
                    [col],
                    |r| r.get(0),
                )
                .map_err(|e| AppError::Library(e.to_string()))?;
            if exists == 0 {
                let _ = conn.execute(
                    &format!("ALTER TABLE tracks ADD COLUMN {col} {col_ty}"),
                    [],
                );
            }
        }
        Ok(())
    }

    pub fn insert_track(
        &self,
        uri: &str,
        path: &str,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        album_artist: Option<&str>,
        genre: Option<&str>,
        year: Option<i32>,
        track: Option<i32>,
        duration: Option<f64>,
        format: Option<&str>,
        sample_rate: Option<u32>,
        bit_depth: Option<u32>,
        channels: Option<u32>,
        cover_key: Option<&str>,
        cue_index: Option<i32>,
        start_time: Option<f64>,
        end_time: Option<f64>,
        file_mtime: Option<i64>,
        source: Option<&str>,
    ) -> AppResult<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO tracks
             (uri, path, title, artist, album, album_artist, genre, year, track, duration, format, sample_rate, bit_depth, channels, has_cover, cover_key, cue_index, start_time, end_time, file_mtime, source)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,?15,?16,?17,?18,?19,?20)",
            params![
                uri, path, title, artist, album, album_artist, genre, year, track, duration,
                format, sample_rate, bit_depth, channels, cover_key, cue_index, start_time,
                end_time, file_mtime, source
            ],
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
        self.bump_revision();
        Ok(conn.last_insert_rowid())
    }

    /// Return the stored `file_mtime` for the track identified by `(uri,
    /// cue_index)`, used to skip re-ingesting files that haven't changed.
    pub fn track_mtime(&self, uri: &str, cue_index: Option<i32>) -> AppResult<Option<i64>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mt: Option<i64> = if let Some(c) = cue_index {
            conn.query_row(
                "SELECT file_mtime FROM tracks WHERE uri = ? AND cue_index = ?",
                params![uri, c],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?
        } else {
            conn.query_row(
                "SELECT file_mtime FROM tracks WHERE uri = ? AND cue_index IS NULL",
                [uri],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?
        };
        Ok(mt)
    }

    pub fn set_cover(&self, id: i64, has_cover: bool, cover_key: Option<&str>) -> AppResult<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let changed = conn
            .execute(
                "UPDATE tracks SET has_cover = ?1, cover_key = ?2 WHERE id = ?3",
                params![has_cover as i32, cover_key, id],
            )
            .map_err(|e| AppError::Library(e.to_string()))?;
        if changed > 0 {
            self.bump_revision();
        }
        Ok(())
    }

    /// Return every track with the fields needed to (re)derive album-keyed
    /// covers: id, path, album, album_artist, and any stored `cover_key`.
    pub fn all_tracks_for_art(
        &self,
    ) -> AppResult<Vec<(i64, String, Option<String>, Option<String>, Option<String>)>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare("SELECT id, path, album, album_artist, cover_key FROM tracks")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| AppError::Library(e.to_string()))?);
        }
        Ok(out)
    }

    /// Return rows that predate the album-keyed cover system (no `cover_key`
    /// yet) so a one-time backfill can derive the key and relocate their cover
    /// file without re-reading every audio file. Each entry carries the id,
    /// path, album and album_artist needed to compute the key.
    pub fn tracks_missing_cover_key(
        &self,
    ) -> AppResult<Vec<(i64, String, Option<String>, Option<String>)>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare("SELECT id, path, album, album_artist FROM tracks WHERE cover_key IS NULL")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| AppError::Library(e.to_string()))?);
        }
        Ok(out)
    }

    /// MPD addresses a CUE-split track as `<dir>/<stem>.cue/trackNNNN` (see
    /// `resolve_play_uri` in api/mod.rs). The library DB instead keys CUE
    /// tracks by the backing *audio file* URI (`<dir>/<stem>.<ext>`) with a
    /// `cue_index`. After a restart MPD resumes at that `.cue/trackNNNN` URI,
    /// which the normal URI lookups can't match, so resolve it here: peel the
    /// `.cue/trackNNNN` suffix to recover the audio-file stem, then match the
    /// track whose URI is that stem plus any extension and whose cue_index
    /// equals the track number.
    pub fn track_by_cue_address(&self, cue_uri: &str) -> AppResult<Option<Track>> {
        let (stem, cue_index) = match parse_cue_address(cue_uri) {
            Some(p) => p,
            None => return Ok(None),
        };
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks WHERE cue_index = ?"
            ))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut rows = stmt
            .query([cue_index])
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut matched = None;
        while let Some(row) = rows
            .next()
            .map_err(|e| AppError::Library(e.to_string()))?
        {
            let track = row_to_track(row);
            let track_stem = track
                .uri
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(&track.uri);
            if path_tail_matches(&stem, track_stem) {
                if matched.is_some() {
                    return Ok(None);
                }
                matched = Some(track);
            }
        }
        Ok(matched)
    }

    pub fn track_by_id(&self, id: i64) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let track = conn
            .query_row(
                &format!("SELECT {TRACK_COLS} FROM tracks WHERE id = ?"),
                [id],
                |r| Ok(row_to_track(r)),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(track)
    }

    /// Look up a CUE-split track by playback position: the track whose
    /// `[start_time, end_time)` window contains `elapsed`. This is the robust
    /// way to identify which CUE track MPD is playing when the file was added
    /// directly (MPD then reports the audio file, not the CUE track number).
    pub fn track_by_uri_and_elapsed(&self, uri: &str, elapsed: f64) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let track = conn
            .query_row(
                &format!(
                    "SELECT {TRACK_COLS} FROM tracks WHERE uri = ? \
                     AND cue_index IS NOT NULL \
                     AND start_time <= ? \
                     AND (end_time IS NULL OR ? < end_time) \
                     ORDER BY start_time DESC LIMIT 1"
                ),
                params![uri, elapsed, elapsed],
                |r| Ok(row_to_track(r)),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(track)
    }

    /// Look up a track by URI, preferring an exact CUE-track match when
    /// `cue_index` is given (so a full-album file split into CUE tracks links
    /// to the specific track MPD is reporting).
    pub fn track_by_uri_cue(&self, uri: &str, cue_index: Option<i32>) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cue_index {
            let exact = conn
                .query_row(
                    &format!("SELECT {TRACK_COLS} FROM tracks WHERE uri = ? AND cue_index = ?"),
                    params![uri, c],
                    |r| Ok(row_to_track(r)),
                )
                .optional()
                .map_err(|e| AppError::Library(e.to_string()))?;
            if exact.is_some() {
                return Ok(exact);
            }
        }
        let track = conn
            .query_row(
                &format!(
                    "SELECT {TRACK_COLS} FROM tracks WHERE uri = ? \
                     ORDER BY cue_index IS NOT NULL, id LIMIT 1"
                ),
                [uri],
                |r| Ok(row_to_track(r)),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(track)
    }

    /// Look up a track whose stored `uri` is a suffix of `mpd_uri`. MPD reports
    /// URIs relative to its `music_directory`, while the DB stores them relative
    /// to the scanned library dir — the two differ by a leading path segment
    /// (e.g. MPD `MyMusic/Artist/Album.flac` vs DB `Artist/Album.flac`). A pure
    /// prefix strip can't always compute that segment, so match by suffix: the
    /// DB uri is the tail of the MPD uri for the same file.
    pub fn track_by_uri_suffix(&self, mpd_uri: &str) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let track = conn
            .query_row(
                &format!(
                    "SELECT {TRACK_COLS} FROM tracks \
                     WHERE ? LIKE '%/' || uri OR ? = uri \
                     ORDER BY length(uri) DESC LIMIT 1"
                ),
                params![mpd_uri, mpd_uri],
                |r| Ok(row_to_track(r)),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(track)
    }

    /// Look up a unique track by its stored absolute backing path. Recovery
    /// refuses ambiguous paths so one decoder error cannot remove a sibling.
    pub fn track_by_path_unique(&self, path: &str) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks WHERE path = ? LIMIT 2"
            ))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut rows = stmt
            .query([path])
            .map_err(|e| AppError::Library(e.to_string()))?;
        let first = rows
            .next()
            .map_err(|e| AppError::Library(e.to_string()))?
            .map(row_to_track);
        if rows
            .next()
            .map_err(|e| AppError::Library(e.to_string()))?
            .is_some()
        {
            return Ok(None);
        }
        Ok(first)
    }

    /// Like `track_by_uri_suffix`, but returns no result when more than one
    /// library row matches. Playback recovery must never pick an arbitrary
    /// sibling from an ambiguous URI.
    pub fn track_by_uri_suffix_unique(&self, mpd_uri: &str) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks \
                 WHERE ? LIKE '%/' || uri OR ? = uri \
                 ORDER BY length(uri) DESC LIMIT 2"
            ))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut rows = stmt
            .query(params![mpd_uri, mpd_uri])
            .map_err(|e| AppError::Library(e.to_string()))?;
        let first = rows
            .next()
            .map_err(|e| AppError::Library(e.to_string()))?
            .map(row_to_track);
        if rows
            .next()
            .map_err(|e| AppError::Library(e.to_string()))?
            .is_some()
        {
            return Ok(None);
        }
        Ok(first)
    }

    /// Delete tracks whose backing file is no longer in the scanned set
    /// (deleted or newly ignored via `.mpdignore`). Returns the number removed.
    /// Delete the non-CUE (whole-file) row for `uri`, if one exists while CUE
    /// tracks for the same uri are present. Used by the scanner so a CUE-backed
    /// file never lingers as a single full-length track.
    pub fn delete_non_cue(&self, uri: &str) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn
            .execute(
                "DELETE FROM tracks WHERE uri = ? AND cue_index IS NULL \
                 AND EXISTS (SELECT 1 FROM tracks t2 WHERE t2.uri = ? AND t2.cue_index IS NOT NULL)",
                params![uri, uri],
            )
            .map_err(|e| AppError::Library(e.to_string()))?;
        if n > 0 {
            self.bump_revision();
        }
        Ok(n as u64)
    }

    /// Delete tracks whose backing file is no longer in the scanned set
    /// (deleted, newly ignored via `.mpdignore`, or belonging to a library
    /// source that has since been removed). Returns the number removed.
    pub fn prune_missing(
        &self,
        seen: &std::collections::HashSet<PathBuf>,
        sources: &[std::path::PathBuf],
    ) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare("SELECT id, path, source FROM tracks")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| AppError::Library(e.to_string()))?;
        // Drop tracks whose source folder is no longer among the active
        // library sources (e.g. the source was removed in settings).
        let active: HashSet<&str> = sources
            .iter()
            .filter_map(|p| p.to_str())
            .collect();
        let mut ids = Vec::new();
        for row in rows {
            let (id, path, source) = row.map_err(|e| AppError::Library(e.to_string()))?;
            let source_gone = match &source {
                Some(s) => !active.contains(s.as_str()),
                None => false,
            };
            if source_gone || !seen.contains(Path::new(&path)) {
                ids.push(id);
            }
        }
        if !ids.is_empty() {
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("DELETE FROM tracks WHERE id IN ({placeholders})");
            conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
                .map_err(|e| AppError::Library(e.to_string()))?;
            self.bump_revision();
        }
        Ok(ids.len() as u64)
    }

    /// Delete every track produced by the given library source folder (absolute
    /// path). Used when a source is removed from settings so its albums leave
    /// the library (issue #46). Returns the number removed.
    pub fn delete_by_source(&self, source: &Path) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let s = source.to_string_lossy().to_string();
        let n = conn
            .execute("DELETE FROM tracks WHERE source = ?", [s])
            .map_err(|e| AppError::Library(e.to_string()))?;
        if n > 0 {
            self.bump_revision();
        }
        Ok(n as u64)
    }

    /// Return the stored absolute filesystem `path` for the whole-file track
    /// identified by `uri` (CUE tracks are keyed by audio file, so we
    /// match the non-CUE row). Used to feed MPD an absolute path that it
    /// can `add` regardless of its `music_directory` (see `api::play`).
    pub fn path_for_uri(&self, uri: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let path: Option<String> = conn
            .query_row(
                "SELECT path FROM tracks WHERE uri = ? AND cue_index IS NULL LIMIT 1",
                [uri],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(path)
    }

    /// Delete exactly one row after confirming its backing path is absent.
    pub fn delete_track_if_missing(&self, id: i64) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let path: Option<String> = conn
            .query_row("SELECT path FROM tracks WHERE id = ?", [id], |r| r.get(0))
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        if path.is_some_and(|p| !Path::new(&p).exists()) {
            let deleted = conn
                .execute("DELETE FROM tracks WHERE id = ?", [id])
                .map_err(|e| AppError::Library(e.to_string()))?;
            if deleted > 0 {
                self.bump_revision();
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Delete exactly one library row without touching its physical file.
    pub fn delete_track(&self, id: i64) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let deleted = conn
            .execute("DELETE FROM tracks WHERE id = ?", [id])
            .map_err(|e| AppError::Library(e.to_string()))?;
        if deleted > 0 {
            self.bump_revision();
        }
        Ok(deleted > 0)
    }


    fn search_with_conn(
        &self,
        conn: &Connection,
        q: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<Vec<Track>> {
        let mut sql = String::from("SELECT {TRACK_COLS_PLACEHOLDER} FROM tracks");
        sql = sql.replace(
            "{TRACK_COLS_PLACEHOLDER}",
            if q.is_some() { TRACK_COLS_Q } else { TRACK_COLS },
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(q) = q {
            let match_expr: String = q
                .split_whitespace()
                .map(|t| {
                    let cleaned = t.trim_matches(|c: char| !c.is_alphanumeric());
                    if cleaned.is_empty() {
                        String::new()
                    } else {
                        format!("{cleaned}*")
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if match_expr.is_empty() {
                return Ok(Vec::new());
            }
            sql.push_str(
                " JOIN tracks_fts ON tracks_fts.rowid = tracks.id \
                 WHERE tracks_fts MATCH ?",
            );
            args.push(Box::new(match_expr));
        } else {
            sql.push_str(" WHERE 1=1");
        }
        if let Some(a) = artist {
            sql.push_str(" AND tracks.artist = ?");
            args.push(Box::new(a.to_string()));
        }
        if let Some(a) = album {
            sql.push_str(" AND tracks.album = ?");
            args.push(Box::new(a.to_string()));
        }
        if q.is_some() {
            sql.push_str(" ORDER BY tracks_fts.rank");
        } else {
            sql.push_str(" ORDER BY album, track, title");
        }
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?");
            args.push(Box::new(limit as i64));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| Ok(row_to_track(r)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        rows.map(|row| row.map_err(|e| AppError::Library(e.to_string())))
            .collect()
    }

    pub fn search(
        &self,
        q: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<Vec<Track>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        self.search_with_conn(&conn, q, artist, album, limit)
    }

    pub fn list_albums(&self) -> AppResult<Vec<String>> {
        self.distinct_column("album")
    }

    /// Return each album paired with the distinct library source folders that
    /// contributed tracks to it. A single album can span more than one source
    /// when parent/child library sources are both configured (issue #46).
    pub fn albums_with_sources(&self) -> AppResult<Vec<(String, Vec<String>)>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(
                "SELECT album, source FROM tracks \
                 WHERE album IS NOT NULL AND source IS NOT NULL \
                 GROUP BY album, source ORDER BY album, source",
            )
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for row in rows {
            let (album, source) = row.map_err(|e| AppError::Library(e.to_string()))?;
            match out.last_mut() {
                Some((last_album, sources)) if *last_album == album => {
                    if !sources.contains(&source) {
                        sources.push(source);
                    }
                }
                _ => out.push((album, vec![source])),
            }
        }
        Ok(out)
    }

    pub fn list_artists(&self) -> AppResult<Vec<String>> {
        self.distinct_column("artist")
    }

    fn distinct_column(&self, col: &str) -> AppResult<Vec<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(&format!(
                "SELECT DISTINCT {col} FROM tracks WHERE {col} IS NOT NULL ORDER BY {col}"
            ))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| AppError::Library(e.to_string()))?);
        }
        Ok(out)
    }
}

/// Parse MPD's CUE address `<dir>/<stem>.cue/trackNNNN` into the audio-file
/// stem (`<dir>/<stem>`) and the 1-based track number. Returns `None` for any
/// URI that isn't a CUE address.
fn parse_cue_address(cue_uri: &str) -> Option<(String, i32)> {
    let marker = ".cue/track";
    let idx = cue_uri.find(marker)?;
    let stem = &cue_uri[..idx];
    let num = cue_uri[idx + marker.len()..].parse::<i32>().ok()?;
    if num <= 0 {
        return None;
    }
    Some((stem.to_string(), num))
}

fn row_to_track(r: &rusqlite::Row) -> Track {
    Track {
        id: r.get(0).unwrap_or(0),
        uri: r.get(1).unwrap_or_default(),
        path: r.get(2).unwrap_or_default(),
        title: r.get(3).ok(),
        artist: r.get(4).ok(),
        album: r.get(5).ok(),
        album_artist: r.get(6).ok(),
        genre: r.get(7).ok(),
        year: r.get(8).ok(),
        track: r.get(9).ok(),
        duration: r.get(10).ok(),
        format: r.get(11).ok(),
        sample_rate: r.get(12).ok(),
        bit_depth: r.get(13).ok(),
        channels: r.get(14).ok(),
        has_cover: r.get::<_, i32>(15).unwrap_or(0) != 0,
        cover_key: r.get(16).ok(),
        cue_index: r.get(17).ok(),
        start_time: r.get(18).ok(),
        end_time: r.get(19).ok(),
        file_mtime: r.get(20).ok(),
        source: r.get(21).ok(),
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    fn fresh() -> LibraryDb {
        let db = LibraryDb::open(Path::new(":memory:")).unwrap();
        db.migrate().unwrap();
        db
    }

    fn put(db: &LibraryDb, path: &str, title: &str, artist: &str, album: &str) {
        db.insert_track(
            path, path, Some(title), Some(artist), Some(album), None, None, None,
            Some(1), Some(180.0), Some("flac"), Some(44100), Some(16), Some(2),
            None, None, None, None, Some(1), None,
        )
        .unwrap();
    }

    #[test]
    fn if_none_match_accepts_weak_lists_and_wildcard() {
        let etag = "\"oxide-1-2\"";
        assert!(if_none_match_matches(etag, etag));
        assert!(if_none_match_matches(" W/\"oxide-1-2\", \"other\" ", etag));
        assert!(if_none_match_matches("*", etag));
        assert!(!if_none_match_matches("\"other\"", etag));
    }

    #[test]
    fn fts_prefix_search_ranks_and_no_midword() {
        let db = fresh();
        put(&db, "/a/1.flac", "Help", "The Beatles", "Help!");
        put(&db, "/a/2.flac", "Yesterday", "The Beatles", "Help!");
        put(&db, "/a/3.flac", "Blackbird", "The Beatles", "White Album");

        let prefix = db.search(Some("beat"), None, None, None).unwrap();
        assert_eq!(prefix.len(), 3, "all Beatles tracks match prefix 'beat*'");

        // Mid-word substring from the old LIKE '%q%' must no longer match.
        assert!(
            db.search(Some("eatl"), None, None, None).unwrap().is_empty(),
            "'eatl' is not a prefix of any token"
        );

        // Title prefix resolves to a single track.
        let t = db.search(Some("yest"), None, None, None).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].title.as_deref(), Some("Yesterday"));
    }

    #[test]
    fn fts_index_stays_in_sync_after_more_inserts() {
        let db = fresh();
        put(&db, "/a/1.flac", "Help", "The Beatles", "Help!");
        assert_eq!(db.search(Some("beat"), None, None, None).unwrap().len(), 1);
        put(&db, "/a/2.flac", "Revolver", "The Beatles", "Revolver");
        assert_eq!(db.search(Some("beat"), None, None, None).unwrap().len(), 2);
    }

    #[test]
    fn fts_filter_by_artist_exact_still_works() {
        let db = fresh();
        put(&db, "/a/1.flac", "Help", "The Beatles", "Help!");
        put(&db, "/a/2.flac", "Hotel California", "Eagles", "Hotel Cal");
        let r = db.search(Some("hotel"), Some("Eagles"), None, None).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].artist.as_deref(), Some("Eagles"));
    }

    /// Insert a CUE-backed track: a split of `uri` at the given cue index with
    /// the `[start, end)` window within the backing file.
    fn put_cue(
        db: &LibraryDb,
        uri: &str,
        cue_index: i32,
        start: f64,
        end: Option<f64>,
        duration: f64,
    ) {
        db.insert_track(
            uri, uri, Some("Cue Track"), Some("Artist"), Some("Album"), None, None, None,
            Some(cue_index), Some(duration), Some("flac"), Some(44100), Some(16), Some(2),
            None, Some(cue_index), Some(start), end, Some(1), None,
        )
        .unwrap();
    }

    #[test]
    fn delete_non_cue_removes_plain_row_only_when_cue_sibling_exists() {
        let db = fresh();
        // A CUE-backed file: one plain (non-CUE) row plus two split rows.
        db.insert_track(
            "Album.flac", "Album.flac", Some("Whole"), Some("Artist"), Some("Album"),
            None, None, None, Some(0), Some(180.0), Some("flac"), Some(44100), Some(16),
            Some(2), None, None, None, None, Some(1), None,
        )
        .unwrap();
        put_cue(&db, "Album.flac", 1, 0.0, Some(100.0), 180.0);
        put_cue(&db, "Album.flac", 2, 100.0, Some(180.0), 180.0);

        let removed = db.delete_non_cue("Album.flac").unwrap();
        assert_eq!(removed, 1, "plain row removed, CUE rows kept");

        // Plain (whole-file) row gone — only the CUE splits remain.
        let leftover_title = db
            .track_by_uri_cue("Album.flac", None)
            .unwrap()
            .and_then(|t| t.title);
        assert_ne!(
            leftover_title.as_deref(),
            Some("Whole"),
            "plain row with title 'Whole' should have been deleted"
        );
        assert!(db.track_by_uri_cue("Album.flac", Some(1)).unwrap().is_some());
        assert!(db.track_by_uri_cue("Album.flac", Some(2)).unwrap().is_some());
    }

    #[test]
    fn delete_non_cue_is_noop_without_cue_sibling() {
        let db = fresh();
        db.insert_track(
            "Solo.flac", "Solo.flac", Some("Solo"), Some("Artist"), Some("Album"),
            None, None, None, Some(0), Some(180.0), Some("flac"), Some(44100), Some(16),
            Some(2), None, None, None, None, Some(1), None,
        )
        .unwrap();
        let removed = db.delete_non_cue("Solo.flac").unwrap();
        assert_eq!(removed, 0, "no CUE sibling -> nothing deleted");
        assert!(db.track_by_uri_cue("Solo.flac", None).unwrap().is_some());
    }

    #[test]
    fn track_by_uri_and_elapsed_picks_cue_split_for_elapsed() {
        let db = fresh();
        put_cue(&db, "Album.flac", 1, 0.0, Some(100.0), 180.0);
        put_cue(&db, "Album.flac", 2, 100.0, Some(200.0), 180.0);

        // 50s in -> first split.
        let t1 = db.track_by_uri_and_elapsed("Album.flac", 50.0).unwrap();
        assert_eq!(t1.and_then(|t| t.cue_index), Some(1));

        // 150s in -> second split.
        let t2 = db.track_by_uri_and_elapsed("Album.flac", 150.0).unwrap();
        assert_eq!(t2.and_then(|t| t.cue_index), Some(2));
    }

    #[test]
    fn track_by_uri_and_elapsed_rolls_to_next_track_at_boundary() {
        let db = fresh();
        put_cue(&db, "Album.flac", 1, 0.0, Some(100.0), 180.0);
        put_cue(&db, "Album.flac", 2, 100.0, None, 180.0);

        // Exactly at the boundary (elapsed == end_time of track 1) selects track 2.
        let t = db.track_by_uri_and_elapsed("Album.flac", 100.0).unwrap();
        assert_eq!(t.and_then(|x| x.cue_index), Some(2));
    }

    #[test]
    fn track_by_uri_and_elapsed_none_outside_all_windows() {
        let db = fresh();
        put_cue(&db, "Album.flac", 1, 10.0, Some(100.0), 180.0);
        // 5s is before the first split's window.
        assert!(db.track_by_uri_and_elapsed("Album.flac", 5.0).unwrap().is_none());
    }

    #[test]
    fn track_by_cue_address_resolves_split_after_restart() {
        let db = fresh();
        let id1 = db
            .insert_track(
                "Music/Album.flac", "Music/Album.flac", Some("Cue Track"), Some("Artist"),
                Some("Album"), None, None, None, Some(1), Some(180.0), Some("flac"),
                Some(44100), Some(16), Some(2), None, Some(1), Some(0.0), Some(100.0), Some(1), None,
            )
            .unwrap();
        let id2 = db
            .insert_track(
                "Music/Album.flac", "Music/Album.flac", Some("Cue Track"), Some("Artist"),
                Some("Album"), None, None, None, Some(2), Some(180.0), Some("flac"),
                Some(44100), Some(16), Some(2), None, Some(2), Some(100.0), Some(180.0), Some(1), None,
            )
            .unwrap();
        db.set_cover(id1, true, Some("al_coverkey")).unwrap();
        db.set_cover(id2, true, Some("al_coverkey")).unwrap();

        // After a restart MPD resumes at the CUE address URI, not the audio
        // file URI. The resolver must map it back to the split track.
        let t = db
            .track_by_cue_address("Music/Album.cue/track0002")
            .unwrap();
        let t = t.expect("CUE address should resolve to a split track");
        assert_eq!(t.cue_index, Some(2));
        assert_eq!(t.title.as_deref(), Some("Cue Track"));
        assert!(t.has_cover, "resolved split keeps its cover metadata");
        assert_eq!(t.cover_key.as_deref(), Some("al_coverkey"));
        let absolute = db
            .track_by_cue_address("/mnt/music/Music/Album.cue/track0002")
            .unwrap()
            .expect("absolute CUE address should resolve by suffix");
        assert_eq!(absolute.id, id2);
    }

    #[test]
    fn track_by_cue_address_none_for_plain_uri() {
        let db = fresh();
        put_cue(&db, "Album.flac", 1, 0.0, Some(100.0), 180.0);
        // A plain (non-CUE) audio file URI must not be misread as a CUE address.
        assert!(db.track_by_cue_address("Album.flac").unwrap().is_none());
        // An out-of-range / malformed track number yields nothing.
        assert!(db.track_by_cue_address("Album.cue/track0000").unwrap().is_none());
    }

    #[test]
    fn track_by_uri_suffix_matches_mpd_prefixed_uri() {
        let db = fresh();
        put(
            &db,
            "Cesaria Evora/09 - Historia De Un Amor.m4a",
            "Historia De Un Amor",
            "Cesaria Evora",
            "Cesaria Evora &",
        );
        let id = db
            .track_by_uri_cue("Cesaria Evora/09 - Historia De Un Amor.m4a", None)
            .unwrap()
            .unwrap()
            .id;
        db.set_cover(id, true, Some("al_coverkey")).unwrap();
        // MPD reports the URI relative to its music_directory, so it carries an
        // extra leading segment the DB doesn't store. Suffix match must land.
        let t = db
            .track_by_uri_suffix("MyMusic/Cesaria Evora/09 - Historia De Un Amor.m4a")
            .unwrap();
        let t = t.expect("suffix match should resolve the track");
        assert_eq!(t.title.as_deref(), Some("Historia De Un Amor"));
        assert!(t.has_cover);
    }

    #[test]
    fn track_by_uri_suffix_none_when_no_overlap() {
        let db = fresh();
        put(&db, "/a/1.flac", "Help", "The Beatles", "Help!");
        assert!(db.track_by_uri_suffix("Totally/Different/Path.flac").unwrap().is_none());
    }

    /// Insert a track tagged with the library `source` that produced it.
    fn put_src(db: &LibraryDb, path: &str, title: &str, artist: &str, album: &str, source: &str) {
        db.insert_track(
            path, path, Some(title), Some(artist), Some(album), None, None, None,
            Some(1), Some(180.0), Some("flac"), Some(44100), Some(16), Some(2),
            None, None, None, None, Some(1), Some(source),
        )
        .unwrap();
    }

    #[test]
    fn delete_by_source_removes_only_that_sources_tracks() {
        // Issue #46: removing a library source must drop its albums, not others.
        let db = fresh();
        put_src(&db, "/music/A/1.flac", "A1", "Artist", "AlbumA", "/music/A");
        put_src(&db, "/music/B/1.flac", "B1", "Artist", "AlbumB", "/music/B");

        let removed = db.delete_by_source(Path::new("/music/A")).unwrap();
        assert_eq!(removed, 1, "only the A-source track is removed");

        // B's track survives and is still searchable.
        let remaining = db.search(Some("Artist"), None, None, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].album.as_deref(), Some("AlbumB"));
        assert_eq!(remaining[0].source.as_deref(), Some("/music/B"));
    }

    #[test]
    fn prune_missing_drops_tracks_of_removed_source() {
        // Issue #46: a scan over the remaining sources must prune tracks whose
        // source folder is no longer configured (even if their files still
        // exist on disk).
        let db = fresh();
        put_src(&db, "/music/A/1.flac", "A1", "Artist", "AlbumA", "/music/A");
        put_src(&db, "/music/B/1.flac", "B1", "Artist", "AlbumB", "/music/B");

        // Only /music/B is still configured; /music/A has been removed.
        let seen: HashSet<PathBuf> =
            [PathBuf::from("/music/B/1.flac")].into_iter().collect();
        let sources = vec![PathBuf::from("/music/B")];
        let pruned = db.prune_missing(&seen, &sources).unwrap();
        assert_eq!(pruned, 1, "A-source track pruned by source, not by file");

        let remaining = db.search(None, None, None, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].source.as_deref(), Some("/music/B"));
    }

    #[test]
    fn albums_with_sources_groups_by_album() {
        // Issue #46: an album can list more than one contributing source.
        let db = fresh();
        put_src(&db, "/music/A/1.flac", "A1", "Artist", "Shared", "/music/A");
        put_src(&db, "/music/B/1.flac", "B1", "Artist", "Shared", "/music/B");
        put_src(&db, "/music/A/2.flac", "A2", "Artist", "Solo", "/music/A");

        let albums = db.albums_with_sources().unwrap();
        let shared = albums
            .iter()
            .find(|(a, _)| a == "Shared")
            .expect("Shared album present");
        assert_eq!(shared.1.len(), 2, "Shared album spans two sources");
        assert!(shared.1.contains(&"/music/A".to_string()));
        assert!(shared.1.contains(&"/music/B".to_string()));

        let solo = albums
            .iter()
            .find(|(a, _)| a == "Solo")
            .expect("Solo album present");
        assert_eq!(solo.1, vec!["/music/A".to_string()]);
    }
}
