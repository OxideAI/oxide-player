use crate::error::{AppError, AppResult};
use crate::library::db::LibraryDb;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::MimeType;
use lofty::tag::{Accessor, ItemKey};
use lofty::read_from_path;
use std::collections::HashMap;
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

fn walk(dir: &Path, files: &mut Vec<PathBuf>, cues: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
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

    let strip_quotes = |s: &str| -> String {
        let s = s.trim();
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    };
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
        let _ = &strip_quotes;
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

    // Parse CUE sheets; the audio files they reference are split into per-track
    // entries, so we skip ingesting them as whole-file tracks.
    let mut parsed_cues: Vec<(PathBuf, CueTrack)> = Vec::new();
    let mut referenced: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
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
        if let Err(e) = ingest(&uri, &path, db, cover_dir) {
            tracing::warn!("skip {}: {e}", path.display());
        } else {
            scanned += 1;
        }
    }

    for (dir, ct) in &parsed_cues {
        if let Err(e) = ingest_cue(ct, dir, db, cover_dir) {
            tracing::warn!("skip cue track {}: {e}", ct.audio_rel.display());
        } else {
            scanned += 1;
        }
    }

    if let Ok(pruned) = db.prune_missing() {
        if pruned > 0 {
            tracing::info!("pruned {pruned} tracks whose files no longer exist");
        }
    }

    Ok(scanned)
}

fn ingest(uri: &str, path: &Path, db: &LibraryDb, cover_dir: &Path) -> AppResult<()> {
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
    )?;

    let has_cover = extract_cover(path, id, cover_dir);
    db.set_cover(id, has_cover)?;
    Ok(())
}

/// Ingest a single track split from a CUE sheet. `audio_rel` is the relative
/// path of the backing audio file; metadata and start/end come from the cue.
fn ingest_cue(ct: &CueTrack, library_dir: &Path, db: &LibraryDb, cover_dir: &Path) -> AppResult<()> {
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
    )?;

    let has_cover = extract_cover(&audio_path, id, cover_dir);
    db.set_cover(id, has_cover)?;
    Ok(())
}

/// Extract a cover for `path` into the cover cache as `{id}.{ext}`.
/// Local image files (`cover.*`, `folder.*`) take precedence over embedded art.
/// Returns true if a cover was written.
pub fn extract_cover(path: &Path, id: i64, cover_dir: &Path) -> bool {
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

    if let Ok(tagged) = read_from_path(path) {
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
        let has = extract_cover(Path::new(&path), id, cover_dir);
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
}
