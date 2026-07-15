use crate::error::{AppError, AppResult};
use crate::types::Track;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

const TRACK_COLS: &str = "id, uri, path, title, artist, album, album_artist, genre, year, \
track, duration, format, sample_rate, bit_depth, channels, has_cover, cue_index, \
start_time, end_time, file_mtime";

#[derive(Clone)]
pub struct LibraryDb {
    conn: Arc<Mutex<Connection>>,
}

impl LibraryDb {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Library(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| AppError::Library(e.to_string()))?;
        Ok(LibraryDb {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
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
                cue_index INTEGER,
                start_time REAL,
                end_time REAL,
                file_mtime INTEGER,
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
        // Idempotent schema upgrades for CUE support (only on pre-existing tables).
        for col in ["cue_index", "start_time", "end_time", "file_mtime"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('tracks') WHERE name = ?",
                    [col],
                    |r| r.get(0),
                )
                .map_err(|e| AppError::Library(e.to_string()))?;
            if exists == 0 {
                let _ = conn.execute(
                    &format!("ALTER TABLE tracks ADD COLUMN {col} {col_ty}", col_ty = match col {
                        "cue_index" => "INTEGER",
                        _ => "REAL",
                    }),
                    [],
                );
            }
        }
        Ok(())
    }

    pub fn clear(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tracks", [])
            .map_err(|e| AppError::Library(e.to_string()))?;
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
        cue_index: Option<i32>,
        start_time: Option<f64>,
        end_time: Option<f64>,
    ) -> AppResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO tracks
             (uri, path, title, artist, album, album_artist, genre, year, track, duration, format, sample_rate, bit_depth, channels, has_cover, cue_index, start_time, end_time)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,?15,?16,?17)",
            params![
                uri, path, title, artist, album, album_artist, genre, year, track, duration,
                format, sample_rate, bit_depth, channels, cue_index, start_time, end_time
            ],
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn set_cover(&self, id: i64, has_cover: bool) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tracks SET has_cover = ?1 WHERE id = ?2",
            params![has_cover as i32, id],
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(())
    }

    pub fn all_track_paths(&self) -> AppResult<Vec<(i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, path FROM tracks")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| AppError::Library(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn track_by_id(&self, id: i64) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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

    pub fn track_by_uri(&self, uri: &str) -> AppResult<Option<Track>> {
        let conn = self.conn.lock().unwrap();
        let track = conn
            .query_row(
                &format!("SELECT {TRACK_COLS} FROM tracks WHERE uri = ?"),
                [uri],
                |r| Ok(row_to_track(r)),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(track)
    }

    /// Delete every track whose backing file no longer exists on disk
    /// (e.g. the user deleted an album). Returns the number removed.
    pub fn prune_missing(&self) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, path FROM tracks")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut ids = Vec::new();
        for row in rows {
            let (id, path) = row.map_err(|e| AppError::Library(e.to_string()))?;
            if !Path::new(&path).exists() {
                ids.push(id);
            }
        }
        for id in &ids {
            conn.execute("DELETE FROM tracks WHERE id = ?", [*id])
                .map_err(|e| AppError::Library(e.to_string()))?;
        }
        Ok(ids.len() as u64)
    }

    /// If the track(s) for `uri` point at a file that no longer exists, remove
    /// them. Returns true when something was deleted. Used when MPD reports a
    /// missing file mid-playback so the dead entry leaves the library.
    pub fn delete_by_uri_if_missing(&self, uri: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let path: Option<String> = conn
            .query_row(
                "SELECT path FROM tracks WHERE uri = ? LIMIT 1",
                [uri],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        if let Some(p) = path {
            if !Path::new(&p).exists() {
                conn.execute("DELETE FROM tracks WHERE uri = ?", [uri])
                    .map_err(|e| AppError::Library(e.to_string()))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Like [`Self::delete_by_uri_if_missing`] but matched by the absolute
    /// `path` (as reported in MPD's error string). Removes the track(s) only
    /// when the backing file is gone. Returns true when something was deleted.
    pub fn delete_by_path_if_missing(&self, path: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: Option<bool> = conn
            .query_row(
                "SELECT path FROM tracks WHERE path = ? LIMIT 1",
                [path],
                |r| Ok(Path::new(&r.get::<_, String>(0)?).exists()),
            )
            .optional()
            .map_err(|e| AppError::Library(e.to_string()))?;
        if let Some(missing) = exists {
            if missing {
                conn.execute("DELETE FROM tracks WHERE path = ?", [path])
                    .map_err(|e| AppError::Library(e.to_string()))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn search(
        &self,
        q: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<Vec<Track>> {
        let mut sql = String::from(
            "SELECT {TRACK_COLS_PLACEHOLDER} FROM tracks WHERE 1=1",
        );
        sql = sql.replace("{TRACK_COLS_PLACEHOLDER}", TRACK_COLS);
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(q) = q {
            sql.push_str(" AND (title LIKE ? OR artist LIKE ? OR album LIKE ?)");
            let like = format!("%{q}%");
            args.push(Box::new(like.clone()));
            args.push(Box::new(like.clone()));
            args.push(Box::new(like));
        }
        if let Some(a) = artist {
            sql.push_str(" AND artist = ?");
            args.push(Box::new(a.to_string()));
        }
        if let Some(a) = album {
            sql.push_str(" AND album = ?");
            args.push(Box::new(a.to_string()));
        }
        sql.push_str(" ORDER BY album, track, title LIMIT ?");
        args.push(Box::new(limit.unwrap_or(500) as i64));

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| Ok(row_to_track(r)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| AppError::Library(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn list_albums(&self) -> AppResult<Vec<String>> {
        self.distinct_column("album")
    }

    pub fn list_artists(&self) -> AppResult<Vec<String>> {
        self.distinct_column("artist")
    }

    fn distinct_column(&self, col: &str) -> AppResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();
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

    pub fn count(&self) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap();
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0))
            .map_err(|e| AppError::Library(e.to_string()))?;
        Ok(c as u64)
    }
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
        cue_index: r.get(16).ok(),
        start_time: r.get(17).ok(),
        end_time: r.get(18).ok(),
        file_mtime: file_mtime_of(r.get::<_, String>(2).unwrap_or_default().as_str()),
    }
}

/// Last-modified time of `p` in unix seconds, or `None` if unreadable.
fn file_mtime_of(p: &str) -> Option<i64> {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}
