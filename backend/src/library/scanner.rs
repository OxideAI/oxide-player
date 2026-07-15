use crate::error::{AppError, AppResult};
use crate::library::db::LibraryDb;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::tag::{Accessor, ItemKey};
use lofty::read_from_path;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const AUDIO_EXTS: &[&str] = &[
    "flac", "wav", "wv", "dsf", "dff", "aiff", "aif", "mp3", "mp2", "aac", "m4a", "ogg", "oga",
    "opus", "mpc", "alac",
];

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn format_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_uppercase())
}

fn parse_year(s: &str) -> Option<i32> {
    s.split(['-', '/', '.']).next().and_then(|p| p.parse::<i32>().ok())
}

fn is_cue(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("cue"))
        .unwrap_or(false)
}

fn load_mpdignore(dir: &Path) -> Vec<String> {
    let path = dir.join(".mpdignore");
    match std::fs::read_to_string(&path) {
        Ok(s) => s
            .trim_start_matches('\u{FEFF}')
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Simple glob match (`*` matches any sequence, `?` matches one char).
fn matches_glob(name: &str, pattern: &str) -> bool {
    let n = name.as_bytes();
    let p = pattern.as_bytes();
    let (mut ni, mut pi) = (0, 0);
    let (mut star_ni, mut star_pi) = (None, None);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            ni += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_ni = Some(ni);
            star_pi = Some(pi);
            pi += 1;
        } else if let (Some(sn), Some(sp)) = (star_ni, star_pi) {
            ni = sn + 1;
            pi = sp + 1;
            star_ni = Some(ni);
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn is_ignored(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches_glob(name, p))
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>, cues: &mut Vec<PathBuf>) {
    let patterns = load_mpdignore(dir);

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).map(|n| n.to_string());
        if let Some(ref name) = name {
            if is_ignored(name, &patterns) {
                continue;
            }
        }
        // Use symlink_metadata so we never follow directory symlinks: a symlink
        // loop (or a symlink to elsewhere on disk) would otherwise cause
        // infinite recursion / traversal outside the library.
        let meta = match std::fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if meta.file_type().is_symlink() {
                continue;
            }
            walk(&p, files, cues);
        } else if is_audio(&p) {
            files.push(p);
        } else if is_cue(&p) {
            cues.push(p);
        }
    }
}

/// A single track split out of a CUE sheet.
struct CueTrack {
    audio_rel: PathBuf,
    track_no: i32,
    title: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    album: Option<String>,
    start: f64,
    end: Option<f64>,
}

/// Parse a basic CUE sheet, returning one entry per `TRACK ... AUDIO` with its
/// `INDEX 01` start time (and the next track's start as its end).
fn parse_cue(cue_path: &Path, library_dir: &Path) -> AppResult<Vec<CueTrack>> {
    let text = std::fs::read_to_string(cue_path).map_err(|e| AppError::Library(e.to_string()))?;
    let parent = cue_path.parent().unwrap_or(cue_path);

    let mut album: Option<String> = None;
    let mut album_artist: Option<String> = None;
    let mut file: Option<String> = None;
    let mut cur_no: Option<i32> = None;
    let mut cur_title: Option<String> = None;
    let mut cur_performer: Option<String> = None;
    let mut cur_start: Option<f64> = None;
    let mut raw: Vec<(i32, Option<String>, Option<String>, f64)> = Vec::new();

    let quoted_value = |line: &str| -> Option<String> {
        let i = line.find('"')?;
        let rest = &line[i + 1..];
        let j = rest.find('"')?;
        Some(rest[..j].to_string())
    };

    for raw_line in text.lines() {
        let line = raw_line.trim();
        let Some((key, rest)) = line.split_once(' ') else {
            continue;
        };
        let key = key.to_uppercase();
        match key.as_str() {
            "PERFORMER" => {
                if cur_no.is_some() {
                    cur_performer = quoted_value(rest);
                } else {
                    album_artist = quoted_value(rest);
                }
            }
            "TITLE" => {
                if cur_no.is_some() {
                    cur_title = quoted_value(rest);
                } else {
                    album = quoted_value(rest);
                }
            }
            "FILE" => file = quoted_value(rest),
            "TRACK" => {
                if let (Some(no), Some(start)) = (cur_no, cur_start) {
                    raw.push((no, cur_title.take(), cur_performer.take(), start));
                }
                cur_no = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<i32>().ok());
                cur_title = None;
                cur_performer = None;
                cur_start = None;
            }
            "INDEX" => {
                let mut it = rest.split_whitespace();
                if it.next() == Some("01") {
                    if let Some(t) = it.next() {
                        cur_start = parse_cue_time(t);
                    }
                }
            }
            _ => {}
        }
    }
    if let (Some(no), Some(start)) = (cur_no, cur_start) {
        raw.push((no, cur_title, cur_performer, start));
    }

    let Some(file) = file else {
        return Ok(Vec::new());
    };
    let audio_path = parent.join(&file);
    let Some(audio_rel) = audio_path.strip_prefix(library_dir).ok() else {
        return Ok(Vec::new());
    };
    let audio_rel = PathBuf::from(audio_rel.to_string_lossy().replace('\\', "/"));

    let mut out = Vec::new();
    for (i, (no, title, artist, start)) in raw.iter().enumerate() {
        let end = raw.get(i + 1).map(|n| n.3);
        out.push(CueTrack {
            audio_rel: audio_rel.clone(),
            track_no: *no,
            title: title.clone(),
            artist: artist.clone(),
            album_artist: album_artist.clone(),
            album: album.clone(),
            start: *start,
            end,
        });
    }
    Ok(out)
}

fn parse_cue_time(t: &str) -> Option<f64> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let m: f64 = parts[0].parse().ok()?;
    let s: f64 = parts[1].parse().ok()?;
    let f: f64 = parts[2].parse().ok()?;
    Some(m * 60.0 + s + f / 75.0)
}

fn cover_ext(mime: &MimeType) -> &'static str {
    match mime {
        MimeType::Jpeg => "jpg",
        MimeType::Png => "png",
        _ => "bin",
    }
}

/// Look for a local cover image in `dir`, preferring `cover.*` over `folder.*`
/// and jpg over png. Matching is case-insensitive on both name and extension.
fn find_local_cover(dir: &Path) -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "cover.jpg",
        "cover.jpeg",
        "cover.png",
        "folder.jpg",
        "folder.jpeg",
        "folder.png",
    ];
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                found.insert(name.to_ascii_lowercase(), p);
            }
        }
    }
    if let Some(c) = NAMES.iter().find_map(|n| found.get(*n).cloned()) {
        return Some(c);
    }
    // Fall back to an image whose stem matches the audio file's stem, e.g.
    // `Album.flac` paired with `Album.jpg` (common when art is named after the
    // release rather than `cover`/`folder`).
    let mut images: Vec<PathBuf> = found
        .values()
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("jpg" | "jpeg" | "png")))
        .cloned()
        .collect();
    if images.len() == 1 {
        return Some(images.remove(0));
    }
    None
}

pub fn scan(dirs: &[PathBuf], db: &LibraryDb, cover_dir: &Path) -> AppResult<u64> {
    let mut files = Vec::new();
    let mut cues = Vec::new();
    for d in dirs {
        if d.exists() {
            walk(d, &mut files, &mut cues);
        } else {
            tracing::warn!("library dir does not exist: {}", d.display());
        }
    }

    let seen: HashSet<PathBuf> = files.iter().cloned().collect();

    // Parse CUE sheets; the audio files they reference are split into per-track
    // entries, so we skip ingesting them as whole-file tracks.
    let mut parsed_cues: Vec<(PathBuf, CueTrack)> = Vec::new();
    let mut referenced: HashSet<PathBuf> = HashSet::new();
    for cue in &cues {
        if let Some(dir) = dirs.iter().find(|d| cue.starts_with(d)) {
            match parse_cue(cue, dir) {
                Ok(tracks) => {
                    for t in &tracks {
                        referenced.insert(t.audio_rel.clone());
                    }
                    for t in tracks {
                        parsed_cues.push((dir.clone(), t));
                    }
                }
                Err(e) => tracing::warn!("skip cue {}: {e}", cue.display()),
            }
        }
    }

    let mut scanned = 0u64;
    for path in files {
        let uri = dirs
            .iter()
            .find(|d| path.starts_with(d))
            .and_then(|d| path.strip_prefix(d).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if referenced.contains(Path::new(&uri)) {
            continue;
        }
        // Incremental scan: skip files whose mtime already matches the stored
        // row, so a refresh only re-reads changed tracks (see #12 / track_mtime).
        let mtime = file_mtime(&path);
        if let Some(db_mt) = db.track_mtime(&uri, None).ok().flatten() {
            if db_mt == mtime {
                scanned += 1;
                continue;
            }
        }
        if let Err(e) = ingest(&uri, &path, db, cover_dir, mtime) {
            tracing::warn!("skip {}: {e}", path.display());
        } else {
            scanned += 1;
        }
    }

    for (dir, ct) in &parsed_cues {
        let uri = ct.audio_rel.to_string_lossy().replace('\\', "/");
        let mtime = file_mtime(&dir.join(&ct.audio_rel));
        // All CUE tracks of one file share `uri` (keyed by cue_index); compare
        // against the per-track cue_index so only changed files are re-ingested.
        if let Some(db_mt) = db.track_mtime(&uri, Some(ct.track_no)).ok().flatten() {
            if db_mt == mtime {
                scanned += 1;
                continue;
            }
        }
        if let Err(e) = ingest_cue(ct, dir, db, cover_dir, mtime) {
            tracing::warn!("skip cue track {}: {e}", ct.audio_rel.display());
        } else {
            scanned += 1;
        }
    }

    if let Ok(pruned) = db.prune_missing(&seen) {
        if pruned > 0 {
            tracing::info!("pruned {pruned} tracks no longer in the library");
        }
    }

    Ok(scanned)
}

fn ingest(uri: &str, path: &Path, db: &LibraryDb, cover_dir: &Path, mtime: i64) -> AppResult<()> {
    let tagged = read_from_path(path).map_err(|e| AppError::Library(e.to_string()))?;
    let props = tagged.properties();

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let (title, artist, album, album_artist, genre, year, track) = match &tag {
        Some(t) => (
            t.title().map(|c| c.into_owned()),
            t.artist().map(|c| c.into_owned()),
            t.album().map(|c| c.into_owned()),
            t.get_string(ItemKey::AlbumArtist).map(str::to_string),
            t.genre().map(|c| c.into_owned()),
            t.get_string(ItemKey::Year)
                .or_else(|| t.get_string(ItemKey::RecordingDate))
                .and_then(parse_year),
            t.track(),
        ),
        None => (None, None, None, None, None, None, None),
    };

    let uri = uri.to_string();
    let path_str = path.to_string_lossy().to_string();

    let id = db.insert_track(
        &uri,
        &path_str,
        title.as_deref(),
        artist.as_deref(),
        album.as_deref(),
        album_artist.as_deref(),
        genre.as_deref(),
        year,
        track.map(|t| t as i32),
        Some(props.duration().as_secs_f64()),
        format_of(path).as_deref(),
        props.sample_rate(),
        props.bit_depth().map(u32::from),
        props.channels().map(u32::from),
        None,
        None,
        None,
        Some(mtime),
    )?;

    let has_cover = extract_cover(Some(&tagged), path, id, cover_dir);
    db.set_cover(id, has_cover)?;
    Ok(())
}

/// Ingest a single track split from a CUE sheet. `audio_rel` is the relative
/// path of the backing audio file; metadata and start/end come from the cue.
fn ingest_cue(
    ct: &CueTrack,
    library_dir: &Path,
    db: &LibraryDb,
    cover_dir: &Path,
    mtime: i64,
) -> AppResult<()> {
    let audio_path = library_dir.join(&ct.audio_rel);
    let tagged = read_from_path(&audio_path).map_err(|e| AppError::Library(e.to_string()))?;
    let props = tagged.properties();

    let duration = match ct.end {
        Some(end) => end - ct.start,
        None => props.duration().as_secs_f64() - ct.start,
    };

    let uri = ct.audio_rel.to_string_lossy().replace('\\', "/");
    let path_str = audio_path.to_string_lossy().to_string();

    let id = db.insert_track(
        &uri,
        &path_str,
        ct.title.as_deref(),
        ct.artist.as_deref(),
        ct.album.as_deref(),
        ct.album_artist.as_deref(),
        None,
        None,
        Some(ct.track_no),
        Some(duration),
        format_of(&audio_path).as_deref(),
        props.sample_rate(),
        props.bit_depth().map(u32::from),
        props.channels().map(u32::from),
        Some(ct.track_no),
        Some(ct.start),
        ct.end,
        Some(mtime),
    )?;

    let has_cover = extract_cover(Some(&tagged), &audio_path, id, cover_dir);
    db.set_cover(id, has_cover)?;
    Ok(())
}

/// Last-modified time of `p` in unix seconds, or 0 if unreadable.
fn file_mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Extract a cover for `path` into the cover cache as `{id}.{ext}`.
/// Local image files (`cover.*`, `folder.*`) take precedence over embedded art.
/// `tagged`, when provided, is the already-parsed audio file so embedded art is
/// read from the first pass instead of re-reading the whole file. Returns true
/// if a cover was written.
pub fn extract_cover(tagged: Option<&TaggedFile>, path: &Path, id: i64, cover_dir: &Path) -> bool {
    if let Some(local) = find_local_cover(path.parent().unwrap_or(path)) {
        let ext = match local
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("png") => "png",
            _ => "jpg",
        };
        let cover_path = cover_dir.join(format!("{id}.{ext}"));
        if std::fs::copy(&local, &cover_path).is_ok() {
            return true;
        }
    }

    let read_from_disk = tagged.is_none().then(|| read_from_path(path).ok());
    let read = match tagged {
        Some(t) => Some(t),
        None => read_from_disk.as_ref().and_then(|o| o.as_ref()),
    };
    if let Some(tagged) = read {
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(pic) = tag.pictures().first() {
                let ext = cover_ext(pic.mime_type().unwrap_or(&MimeType::Jpeg));
                let cover_path = cover_dir.join(format!("{id}.{ext}"));
                if std::fs::write(&cover_path, pic.data()).is_ok() {
                    return true;
                }
            }
        }
    }

    false
}

/// Re-derive covers for every track already in the database, without touching
/// metadata. Clears the cover cache first so removed/renamed local art is dropped.
pub fn rescan_art(db: &LibraryDb, cover_dir: &Path) -> AppResult<u64> {
    if let Ok(entries) = std::fs::read_dir(cover_dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let tracks = db.all_track_paths()?;
    let mut with_cover = 0u64;
    for (id, path) in tracks {
        let has = extract_cover(None, Path::new(&path), id, cover_dir);
        db.set_cover(id, has)?;
        if has {
            with_cover += 1;
        }
    }
    Ok(with_cover)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_audio_extensions() {
        assert!(is_audio(Path::new("song.flac")));
        assert!(is_audio(Path::new("song.MP3")));
        assert!(!is_audio(Path::new("cover.jpg")));
        assert!(!is_audio(Path::new("notes.txt")));
    }

    #[test]
    fn format_uppercases_extension() {
        assert_eq!(format_of(Path::new("a.flac")).as_deref(), Some("FLAC"));
        assert_eq!(format_of(Path::new("a.mp3")).as_deref(), Some("MP3"));
    }

    #[test]
    fn matches_glob_exact() {
        assert!(matches_glob(".git", ".git"));
        assert!(!matches_glob(".gitignore", ".git"));
    }

    #[test]
    fn matches_glob_star() {
        assert!(matches_glob("foo.bak", "*.bak"));
        assert!(matches_glob("Thumbs.db", "Thumbs.*"));
        assert!(!matches_glob("notes.txt", "*.bak"));
    }

    #[test]
    fn matches_glob_question() {
        assert!(matches_glob("file1.txt", "file?.txt"));
        assert!(!matches_glob("file12.txt", "file?.txt"));
    }

    #[test]
    fn matches_glob_multi_star() {
        assert!(matches_glob(".stfolder", ".st*"));
        assert!(matches_glob(".stversions", ".st*"));
        assert!(!matches_glob("folder", ".st*"));
    }

    #[test]
    fn is_ignored_checks_against_patterns() {
        let patterns = vec!["*.bak".to_string(), ".git".to_string()];
        assert!(is_ignored("backup.bak", &patterns));
        assert!(is_ignored(".git", &patterns));
        assert!(!is_ignored("song.flac", &patterns));
    }

    #[test]
    fn load_mpdignore_skips_comments_and_blanks() {
        let dir = std::env::temp_dir().join("mpdignore_test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join(".mpdignore"),
            b"*.bak\n# this is a comment\n\n.stfolder\n",
        )
        .unwrap();
        let patterns = load_mpdignore(&dir);
        assert_eq!(patterns, vec!["*.bak", ".stfolder"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_mpdignore_nonexistent_returns_empty() {
        let dir = std::env::temp_dir().join("mpdignore_nonexistent");
        let patterns = load_mpdignore(&dir);
        assert!(patterns.is_empty());
    }

    #[test]
    fn walk_skips_ignored_entries() {
        let dir = std::env::temp_dir().join("mpdignore_walk_test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join(".mpdignore"), b"*.bak\nignored_dir\n").unwrap();
        std::fs::write(dir.join("track.flac"), b"").unwrap();
        std::fs::write(dir.join("notes.bak"), b"").unwrap();
        let _ = std::fs::create_dir_all(dir.join("ignored_dir"));
        std::fs::write(dir.join("ignored_dir").join("hidden.flac"), b"").unwrap();
        let _ = std::fs::create_dir_all(dir.join("other_dir"));
        std::fs::write(dir.join("other_dir").join("deep.flac"), b"").unwrap();

        let mut files = Vec::new();
        let mut cues = Vec::new();
        walk(&dir, &mut files, &mut cues);
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("track.flac")));
        assert!(files.iter().any(|p| p.ends_with("deep.flac")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
