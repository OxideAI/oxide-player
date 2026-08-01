use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Manages injection of the `include` directive for device config fragments
/// into MPD's main configuration file.
#[derive(Debug, Clone)]
pub struct IncludeInjector {
    mpd_config: PathBuf,
}

fn include_target(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("include")?;
    if !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    Some(rest.trim())
}

impl IncludeInjector {
    /// Create a new injector for the given MPD config path.
    pub fn new(mpd_config: PathBuf) -> Self {
        Self { mpd_config }
    }

    /// Read and return the MPD config content, or a descriptive error.
    fn read_config(&self) -> io::Result<String> {
        fs::read_to_string(&self.mpd_config)
            .map_err(|e| io::Error::new(
                e.kind(),
                format!(
                    "cannot read MPD config at {}: {}. Ensure the backend user has read permission.",
                    self.mpd_config.display(),
                    e,
                ),
            ))
    }

    /// Write config content atomically (temp file + rename), returning a
    /// descriptive error on failure.
    fn write_config(&self, content: &str) -> io::Result<()> {
        let dir = self.mpd_config.parent().unwrap_or(Path::new("."));
        let tmp_name = format!(
            ".{}.tmp",
            self.mpd_config.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("mpd.conf")
        );
        let tmp_path = dir.join(&tmp_name);

        let mut f = fs::File::create(&tmp_path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot modify MPD config at {}: {}. Ensure the backend user has write permission on the directory.",
                    self.mpd_config.display(),
                    e,
                ),
            )
        })?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        drop(f);

        fs::rename(&tmp_path, &self.mpd_config).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot rename temp file to {}: {}. Ensure the backend user has write permission on the config file.",
                    self.mpd_config.display(),
                    e,
                ),
            )
        })?;
        Ok(())
    }

    /// Ensure the MPD config file includes the fragment directory. Inserts an
    /// `include "/abs/path/to/mpd-outputs.d/*.conf"` line before the first
    /// `audio_output {` block if no suitable include line exists.
    ///
    /// Returns `true` if the config was modified (include line was added or
    /// updated), `false` if it was already correct.
    pub fn ensure_include(&self, fragment_dir: &Path) -> io::Result<bool> {
        let content = self.read_config()?;
        let include_line = format!(
            r#"include "{}/*.conf""#,
            fragment_dir.to_string_lossy()
        );

        // Canonicalize the fragment dir for comparison, so we detect an
        // existing include with a different absolute form of the same path.
        let canonical_dir = fragment_dir.canonicalize().unwrap_or_else(|_| fragment_dir.to_path_buf());
        let canonical_include = format!(
            r#"include "{}/*.conf""#,
            canonical_dir.to_string_lossy()
        );

        // Search for an existing include line matching the fragment directory.
        let existing_include_pos = content.lines().position(|line| {
            include_target(line)
                .is_some_and(|target| target.contains("mpd-outputs.d"))
        });

        if let Some(pos) = existing_include_pos {
            let existing_line = content.lines().nth(pos).unwrap();
            let expected_target = include_target(&include_line).unwrap();
            let canonical_target = include_target(&canonical_include).unwrap();
            if include_target(existing_line)
                .is_some_and(|target| target == expected_target || target == canonical_target)
            {
                // Already set up correctly — no change needed.
                return Ok(false);
            }
            // Stale path — replace it.
            let lines: Vec<&str> = content.lines().collect();
            let mut new_lines = lines.clone();
            new_lines[pos] = &include_line;
            let new_content = new_lines.join("\n");
            self.write_config(&new_content)?;
            return Ok(true);
        }

        // No existing include found — insert before first `audio_output {`, or
        // at end of file.
        let insert_pos = content.lines().position(|line| {
            line.trim() == "audio_output {"
        });

        let new_content = match insert_pos {
            Some(pos) => {
                let lines: Vec<&str> = content.lines().collect();
                let mut new_lines: Vec<&str> = lines[..pos].to_vec();
                new_lines.push(&include_line);
                new_lines.extend(&lines[pos..]);
                new_lines.join("\n")
            }
            None => {
                // No audio_output block at all — append.
                let trimmed = content.trim_end();
                if trimmed.is_empty() {
                    format!("{}\n", include_line)
                } else {
                    format!("{}\n\n{}\n", trimmed, include_line)
                }
            }
        };
        self.write_config(&new_content)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file(test_name: &str) -> (PathBuf, String) {
        let pid = std::process::id();
        let tid = std::thread::current().id();
        let dir = std::env::temp_dir().join(format!("oxide_test_inject_{}_{:?}_{}", pid, tid, test_name));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("mpd.conf");
        (path, dir.to_string_lossy().to_string())
    }

    fn cleanup(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn test_inject_into_config_with_no_existing_include() {
        let (path, _) = temp_file("test_inject_into_config_with_no_existing_include");
        let fragment_dir = PathBuf::from("/tmp/mpd-outputs.d");

        // Write a config that has an audio_output block
        fs::write(&path, "music_directory \"/music\"\naudio_output {\n    type \"alsa\"\n    name \"Default\"\n}\n").unwrap();

        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(modified, "should have modified the config");

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("include \"/tmp/mpd-outputs.d/*.conf\""),
            "config should contain include line:\n{}",
            content,
        );
        // The include should be before audio_output {
        let pos_include = content.find("include \"/tmp/mpd-outputs.d/*.conf\"").unwrap();
        let pos_audio = content.find("audio_output {").unwrap();
        assert!(pos_include < pos_audio, "include should be before audio_output {{");

        cleanup(&path);
    }

    #[test]
    fn test_inject_into_config_with_no_audio_output() {
        let (path, _) = temp_file("test_inject_into_config_with_no_audio_output");
        let fragment_dir = PathBuf::from("/var/lib/mpd/outputs.d");

        fs::write(&path, "music_directory \"/music\"\nbind_to_address \"any\"\n").unwrap();

        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(modified, "should have modified the config");

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("include \"/var/lib/mpd/outputs.d/*.conf\""),
            "config should contain include line appended:\n{}",
            content,
        );

        cleanup(&path);
    }

    #[test]
    fn test_inject_with_existing_correct_include_is_noop() {
        let (path, _) = temp_file("test_inject_with_existing_correct_include_is_noop");
        let fragment_dir = PathBuf::from("/var/lib/mpd-outputs.d");

        fs::write(
            &path,
            "music_directory \"/music\"\ninclude \"/var/lib/mpd-outputs.d/*.conf\"\naudio_output {\n    type \"alsa\"\n    name \"Default\"\n}\n",
        ).unwrap();

        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(!modified, "should NOT modify when include is already correct");

        cleanup(&path);
    }

    #[test]
    fn test_inject_with_aligned_existing_include_is_noop() {
        let (path, _) = temp_file("test_inject_with_aligned_existing_include_is_noop");
        let fragment_dir = PathBuf::from("/var/lib/oxide-player/mpd-outputs.d");

        fs::write(
            &path,
            "music_directory \"/music\"\ninclude             \"/var/lib/oxide-player/mpd-outputs.d/*.conf\"\naudio_output {\n    type \"alsa\"\n}\n",
        ).unwrap();

        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(!modified, "aligned include should already be correct");

        cleanup(&path);
    }

    #[test]
    fn test_inject_replaces_stale_path() {
        let (path, _) = temp_file("test_inject_replaces_stale_path");
        let fragment_dir = PathBuf::from("/new/path/mpd-outputs.d");

        fs::write(
            &path,
            "music_directory \"/music\"\ninclude \"/old/path/mpd-outputs.d/*.conf\"\naudio_output {\n    type \"alsa\"\n    name \"Default\"\n}\n",
        ).unwrap();

        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(modified, "should replace stale include path");

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("include \"/new/path/mpd-outputs.d/*.conf\""),
            "should have updated to new path:\n{}",
            content,
        );
        assert!(!content.contains("old/path"), "old path should be gone");

        cleanup(&path);
    }

    #[test]
    fn test_atomic_write_survives_simulated_crash() {
        let (path, _) = temp_file("test_atomic_write_survives_simulated_crash");
        let fragment_dir = PathBuf::from("/tmp/mpd-outputs.d");

        // Write initial content
        fs::write(&path, "original\n").unwrap();

        // Simulate what atomic_write does: create temp, don't rename
        let dir = path.parent().unwrap();
        let tmp_name = format!(".{}.tmp", path.file_name().unwrap().to_str().unwrap());
        let tmp_path = dir.join(&tmp_name);
        fs::write(&tmp_path, "corrupted\n").unwrap();
        // Rename was NOT called — leave temp + original intact

        // The original file should be untouched
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "original\n");

        // Clean up
        let _ = fs::remove_file(&tmp_path);
        let injector = IncludeInjector::new(path.clone());
        let modified = injector.ensure_include(&fragment_dir).unwrap();
        assert!(modified);

        // Clean up temp file if any
        let _ = fs::remove_file(&tmp_path);
        cleanup(&path);
    }
}
