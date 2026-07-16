use crate::error::{AppError, AppResult};
use crate::types::Track;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const TRACK_COLS: &str = "id, uri, path, title, artist, album, album_artist, genre, year, \
track, duration, format, sample_rate, bit_depth, channels, has_cover, cover_key, cue_index, \
start_time, end_time, file_mtime";
// Qualified form used when joining `tracks_fts` (both tables expose
// title/artist/album and an unqualified reference would be ambiguous).
const TRACK_COLS_Q: &str = "tracks.id, tracks.uri, tracks.path, tracks.title, tracks.artist, \
tracks.album, tracks.album_artist, tracks.genre, tracks.year, tracks.track, tracks.duration, \
tracks.format, tracks.sample_rate, tracks.bit_depth, tracks.channels, tracks.has_cover, \
tracks.cover_key, tracks.cue_index, tracks.start_time, tracks.end_time, tracks.file_mtime";

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
    ) -> AppResult<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO tracks
             (uri, path, title, artist, album, album_artist, genre, year, track, duration, format, sample_rate, bit_depth, channels, has_cover, cover_key, cue_index, start_time, end_time, file_mtime)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,?15,?16,?17,?18,?19)",
            params![
                uri, path, title, artist, album, album_artist, genre, year, track, duration,
                format, sample_rate, bit_depth, channels, cover_key, cue_index, start_time,
                end_time, file_mtime
            ],
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
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
        conn.execute(
            "UPDATE tracks SET has_cover = ?1, cover_key = ?2 WHERE id = ?3",
            params![has_cover as i32, cover_key, id],
        )
        .map_err(|e| AppError::Library(e.to_string()))?;
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
        Ok(n as u64)
    }

    pub fn prune_missing(&self, seen: &std::collections::HashSet<PathBuf>) -> AppResult<u64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare("SELECT id, path FROM tracks")
            .map_err(|e| AppError::Library(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Library(e.to_string()))?;
        let mut ids = Vec::new();
        for row in rows {
            let (id, path) = row.map_err(|e| AppError::Library(e.to_string()))?;
            if !seen.contains(Path::new(&path)) {
                ids.push(id);
            }
        }
        if !ids.is_empty() {
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("DELETE FROM tracks WHERE id IN ({placeholders})");
            conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
                .map_err(|e| AppError::Library(e.to_string()))?;
        }
        Ok(ids.len() as u64)
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


    /// If the track(s) for `uri` point at a file that no longer exists, remove
    /// them. Returns true when something was deleted. Used when MPD reports a
    /// missing file mid-playback so the dead entry leaves the library.
    pub fn delete_by_uri_if_missing(&self, uri: &str) -> AppResult<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
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
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
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
            "SELECT {TRACK_COLS_PLACEHOLDER} FROM tracks",
        );
        sql = sql.replace(
            "{TRACK_COLS_PLACEHOLDER}",
            if q.is_some() { TRACK_COLS_Q } else { TRACK_COLS },
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(q) = q {
            // Prefix FTS query: each whitespace-separated token becomes a bare
            // prefix term ('beat' -> 'beat*') so a partial word matches from
            // its start. The term is deliberately NOT wrapped in double quotes:
            // FTS5 treats a *bound* MATCH parameter as if already quoted, which
            // would turn '"beat"*' into a phrase (no prefix match). Tokens are
            // stripped of non-alphanumeric chars at the edges, so the result is
            // safe to interpolate. Join brings `rank` (bm25 relevance) into
            // scope for ordering, replacing the old `LIKE '%q%'` full scan.
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
                // No searchable tokens (e.g. only punctuation) -> no matches.
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

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
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
            None, None, None, None, Some(1),
        )
        .unwrap();
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
            None, Some(cue_index), Some(start), end, Some(1),
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
            Some(2), None, None, None, None, Some(1),
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
            Some(2), None, None, None, None, Some(1),
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
}
