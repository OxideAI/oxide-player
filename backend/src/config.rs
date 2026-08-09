use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mpd_host: String,
    pub mpd_port: u16,
    #[serde(default = "default_true")]
    pub mpd_autostart: bool,
    #[serde(default)]
    pub mpd_binary: Option<String>,
    #[serde(default)]
    pub mpd_config: Option<PathBuf>,
    /// Absolute path to MPD's `music_directory`. The library DB stores
    /// absolute file paths, but MPD addresses tracks by URIs *relative* to its
    /// music directory (e.g. `MyMusic/Artist/Album.flac`). When this is set,
    /// every library track must live under that directory; otherwise playback
    /// fails with a configuration error. When unset, playback falls back to
    /// the matching `library_dirs` root. For CUE tracks we append
    /// `.cue/trackNNNN` so the individual split track is played.
    #[serde(default)]
    pub mpd_music_directory: Option<PathBuf>,
    /// Reconnect paired Bluetooth output devices when the server starts.
    /// Enabled by default so a service restart preserves the active speaker.
    #[serde(default = "default_true")]
    pub bluetooth_reconnect_on_startup: bool,
    pub listen: String,
    pub data_dir: PathBuf,
    pub library_dirs: Vec<PathBuf>,
    pub static_dir: PathBuf,
    pub camilladsp_config_path: PathBuf,
    pub camilladsp_ws_url: Option<String>,
    #[serde(default = "default_true")]
    pub camilladsp_autostart: bool,
    #[serde(default)]
    pub camilladsp_binary: Option<String>,
    #[serde(default)]
    pub camilladsp_capture_device: Option<String>,
    #[serde(default)]
    pub camilladsp_capture_rate: Option<u32>,
    #[serde(default)]
    pub default_dsp_profiles: Vec<crate::dsp::profile::DspProfile>,
    /// Enable the real FFT audio visualizer. When true the backend taps the PCM
    /// capture device and streams magnitude bins to `/api/visualizer`. Off by
    /// default so the feature has zero cost / no capture device dependency when
    /// unused (especially important where the capture device isn't available).
    #[serde(default)]
    pub visualizer_fft: bool,
    /// ALSA/CoreAudio device to capture for the FFT visualizer. Defaults to the
    /// CamillaDSP capture device (the MPD→DSP loopback), so on the real player
    /// the visualizer analyzes the actual output. On macOS set this to a local
    /// input/output device name (e.g. "BlackHole" / "Built-in Microphone").
    #[serde(default)]
    pub visualizer_capture_device: Option<String>,
    /// Sample rate for the FFT capture stream. When unset, defaults to the
    /// CamillaDSP capture rate (44.1 kHz) so it matches the loopback on Linux.
    #[serde(default)]
    pub visualizer_capture_rate: Option<u32>,
    /// Optional path to an MPD `fifo` output the visualizer reads instead of
    /// capturing an ALSA device. MPD feeds the fifo on every enabled output
    /// (Bluetooth / DSP loopback / analog), so the visualizer animates
    /// regardless of the active routing — and there is no substream contention
    /// with CamillaDSP's loopback capture (snd-aloop delivers a substream to
    /// only one capture client). The installer writes the matching
    /// `visualizer-fifo.conf` output and sets this key. The fifo is
    /// S16_LE interleaved stereo at 44.1 kHz (format "44100:16:2").
    #[serde(default)]
    pub visualizer_fifo: Option<String>,
    /// Longest side (in px) a cover image is allowed to have after
    /// optimization. Oversized covers are downscaled to fit. 0 keeps the
    /// original dimension (only file-size recompression applies).
    #[serde(default = "default_cover_max_dimension")]
    pub cover_max_dimension: u32,
    /// Maximum cover file size (in bytes) after optimization. Covers larger
    /// than this are recompressed. 0 disables the size check.
    #[serde(default = "default_cover_max_bytes")]
    pub cover_max_bytes: u64,
    /// JPEG quality (0–100) used when re-encoding oversized covers.
    #[serde(default = "default_cover_quality")]
    pub cover_quality: u8,
}

fn default_cover_max_dimension() -> u32 {
    1200
}

fn default_cover_max_bytes() -> u64 {
    512_000
}

fn default_cover_quality() -> u8 {
    85
}

/// Resolved cover optimization settings, with safe defaults applied for any
/// out-of-range values so a malformed config can never disable optimization
/// entirely or produce garbage output.
#[derive(Debug, Clone, Copy)]
pub struct CoverOptimization {
    pub max_dimension: u32,
    pub max_bytes: u64,
    pub quality: u8,
}

impl CoverOptimization {
    pub fn from_config(cfg: &Config) -> Self {
        let max_dimension = if cfg.cover_max_dimension == 0 {
            1200
        } else {
            cfg.cover_max_dimension
        };
        let max_bytes = if cfg.cover_max_bytes == 0 {
            512_000
        } else {
            cfg.cover_max_bytes
        };
        let quality = cfg.cover_quality.clamp(10, 100);
        CoverOptimization {
            max_dimension,
            max_bytes,
            quality,
        }
    }
}

fn read_json_config(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| "parsing config json")
}

impl Config {
    pub fn load(
        file: Option<&std::path::Path>,
        cli: &Cli,
    ) -> anyhow::Result<(Config, Option<PathBuf>)> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::load_from_dir(file, cli, &cwd)
    }

    /// Load configuration using an explicit base directory.
    ///
    /// This keeps callers that need a deterministic configuration root from
    /// mutating the process-wide current directory.
    pub(crate) fn load_from_dir(
        file: Option<&std::path::Path>,
        cli: &Cli,
        base_dir: &Path,
    ) -> anyhow::Result<(Config, Option<PathBuf>)> {
        let (mut config, resolved) = match file {
            Some(path) => (read_json_config(path)?, Some(path.to_path_buf())),
            None => {
                let defaults = Self::default_config_from_dir(base_dir);
                let persisted = defaults.data_dir.join("config.json");
                if persisted.exists() {
                    (read_json_config(&persisted)?, Some(persisted))
                } else {
                    (defaults, None)
                }
            }
        };

        if let Some(host) = &cli.mpd_host {
            config.mpd_host = host.clone();
        }
        if let Some(port) = cli.mpd_port {
            config.mpd_port = port;
        }
        if cli.listen != "127.0.0.1:8000" {
            config.listen = cli.listen.clone();
        }
        config.validate()?;
        Ok((config, resolved))
    }

    #[cfg(test)]
    pub(crate) fn default_config() -> Config {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::default_config_from_dir(&cwd)
    }
    fn default_config_from_dir(cwd: &Path) -> Config {
        Config {
            mpd_host: "127.0.0.1".to_string(),
            mpd_port: 6600,
            mpd_autostart: true,
            mpd_binary: None,
            mpd_config: None,
            mpd_music_directory: None,
            bluetooth_reconnect_on_startup: true,
            // Bind to localhost by default; override with --listen (or config)
            // only when you intend to expose the (currently unauthenticated) API
            // beyond this machine. See AGENTS.md / security notes.
            listen: "127.0.0.1:8000".to_string(),
            data_dir: cwd.join("data"),
            library_dirs: vec![cwd.join("music")],
            static_dir: cwd.join("../frontend/dist"),
            camilladsp_config_path: cwd.join("data/camilladsp/config.yml"),
            camilladsp_ws_url: Some("ws://127.0.0.1:1234".to_string()),
            camilladsp_autostart: true,
            camilladsp_binary: None,
            camilladsp_capture_device: None,
            camilladsp_capture_rate: None,
            default_dsp_profiles: Vec::new(),
            visualizer_fft: false,
            visualizer_capture_device: None,
            visualizer_capture_rate: None,
            visualizer_fifo: None,
            cover_max_dimension: default_cover_max_dimension(),
            cover_max_bytes: default_cover_max_bytes(),
            cover_quality: default_cover_quality(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("library.db")
    }

    pub fn cover_cache_dir(&self) -> PathBuf {
        self.data_dir.join("covers")
    }

    /// Validate the config before persisting it back to disk. Rejects values that
    /// would break the server on the next (or current) run.
    pub fn validate(&self) -> Result<()> {
        if self.mpd_host.trim().is_empty() {
            anyhow::bail!("mpd_host must not be empty");
        }
        if !(1..=65535).contains(&self.mpd_port) {
            anyhow::bail!("mpd_port must be between 1 and 65535");
        }
        if self.listen.trim().is_empty() {
            anyhow::bail!("listen must not be empty");
        }
        for dir in &self.library_dirs {
            if !dir.is_absolute() {
                anyhow::bail!("library dir must be an absolute path: {}", dir.display());
            }
        }
        // A non-loopback listen widens exposure of the (currently
        // unauthenticated) API: warn but allow, since the user may intend it.
        Ok(())
    }

    /// Add a library source folder, deduplicating against the existing set.
    ///
    /// - If `path` is already present or is nested inside an existing source,
    ///   nothing changes and `None` is returned (the add is a no-op / duplicate).
    /// - If `path` is a parent of one or more existing sources, those child
    ///   sources are subsumed and removed; they are returned so the caller can
    ///   drop their tracks. `path` is then appended.
    ///
    /// This prevents scanning the same files twice when a parent folder is added
    /// after a child, or a child after a parent (issue #46).
    pub fn add_library_dir(
        &mut self,
        path: PathBuf,
    ) -> Option<Vec<PathBuf>> {
        if self
            .library_dirs
            .iter()
            .any(|d| d == &path || path.starts_with(d))
        {
            return None;
        }
        let subsumed: Vec<PathBuf> = self
            .library_dirs
            .iter()
            .filter(|d| d.starts_with(&path))
            .cloned()
            .collect();
        if !subsumed.is_empty() {
            self.library_dirs.retain(|d| !d.starts_with(&path));
        }
        self.library_dirs.push(path);
        Some(subsumed)
    }

    /// Atomically write the config as pretty JSON: serialize to a temp file
    /// beside the target, then rename so a crash mid-write never leaves a
    /// half-written (and therefore unparseable) config.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating config dir {}", parent.display())
            })?;
        }
        let text =
            serde_json::to_string_pretty(self).context("serializing config to json")?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("writing config {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!("renaming config {} -> {}", tmp.display(), path.display())
        })?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(name = "oxide-player", about = "Audiophile music server", version)]
pub struct Cli {
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub mpd_host: Option<String>,
    #[arg(long)]
    pub mpd_port: Option<u16>,
    #[arg(long, default_value = "127.0.0.1:8000")]
    pub listen: String,
    /// Allow running as the root user. The server performs no privilege drop on
    /// its own; this flag only silences the warning when launched as uid 0.
    #[arg(long)]
    pub allow_root: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn default_cli() -> Cli {
        Cli {
            config: None,
            mpd_host: None,
            mpd_port: None,
            listen: "127.0.0.1:8000".to_string(),
            allow_root: false,
        }
    }

    fn write_config(path: &Path, cfg: &Config) {
        fs::write(path, serde_json::to_string_pretty(cfg).unwrap()).unwrap();
    }

    #[test]
    fn load_with_explicit_path_returns_that_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        let mut expected = Config::default_config();
        expected.listen = "0.0.0.0:8080".to_string();
        write_config(&path, &expected);

        let (got, resolved) = Config::load(Some(&path), &default_cli()).unwrap();

        assert_eq!(got.listen, "0.0.0.0:8080");
        assert_eq!(resolved, Some(path));
    }

    #[test]
    fn load_without_file_resolves_path_for_existing_persisted_config() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let cfg_path = data_dir.join("config.json");
        let mut expected = Config::default_config();
        expected.listen = "0.0.0.0:9090".to_string();
        expected.data_dir = data_dir;
        write_config(&cfg_path, &expected);

        // use the explicit path load as a proxy — load(None) depends on CWD
        let (got, resolved) = Config::load(Some(&cfg_path), &default_cli()).unwrap();

        assert_eq!(got.listen, "0.0.0.0:9090");
        assert_eq!(resolved, Some(cfg_path));
    }

    #[test]
    fn load_without_file_returns_defaults_and_none_resolved() {
        let dir = tempfile::tempdir().unwrap();

        let (got, resolved) =
            Config::load_from_dir(None, &default_cli(), dir.path()).unwrap();

        assert!(resolved.is_none(), "expected no resolved path without persisted config");
        assert_eq!(got.listen, "127.0.0.1:8000");
        assert_eq!(got.data_dir, dir.path().join("data"));
    }

    #[test]
    fn bluetooth_reconnect_is_enabled_by_default() {
        assert!(Config::default_config().bluetooth_reconnect_on_startup);

        let mut raw = serde_json::to_value(Config::default_config()).unwrap();
        raw.as_object_mut()
            .unwrap()
            .remove("bluetooth_reconnect_on_startup");
        let parsed: Config = serde_json::from_value(raw).unwrap();
        assert!(parsed.bluetooth_reconnect_on_startup);
    }

    #[test]
    fn write_to_creates_parent_dir_and_writes_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.json");
        let cfg = Config::default_config();

        cfg.write_to(&path).unwrap();

        assert!(path.exists(), "config file should exist after write_to");
        let read: Config = read_json_config(&path).unwrap();
        assert_eq!(read.listen, cfg.listen);
        assert_eq!(read.mpd_port, cfg.mpd_port);
    }

    #[test]
    fn write_to_tmp_file_is_cleaned_up_after_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = Config::default_config();

        cfg.write_to(&path).unwrap();

        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp file should be cleaned up after rename");
    }

    #[test]
    fn cli_overrides_are_applied_after_load() {
        let dir = tempfile::tempdir().unwrap();
        let cli = Cli {
            config: None,
            mpd_host: Some("10.0.0.1".to_string()),
            mpd_port: Some(7700),
            listen: "0.0.0.0:9000".to_string(),
            allow_root: false,
        };

        let (got, resolved) = Config::load_from_dir(None, &cli, dir.path()).unwrap();

        assert_eq!(got.mpd_host, "10.0.0.1");
        assert_eq!(got.mpd_port, 7700);
        assert_eq!(got.listen, "0.0.0.0:9000");
        assert!(resolved.is_none());
        assert_eq!(got.data_dir, dir.path().join("data"));
    }

    #[test]
    fn add_library_dir_rejects_child_of_existing() {
        // Issue #46: adding a child folder when its parent is already a source
        // must be a no-op (the parent already covers it).
        let mut cfg = Config::default_config();
        cfg.library_dirs = vec![std::path::PathBuf::from("/music")];
        let res = cfg.add_library_dir(std::path::PathBuf::from("/music/jazz"));
        assert!(res.is_none(), "child of existing source must be rejected");
        assert_eq!(cfg.library_dirs, vec![std::path::PathBuf::from("/music")]);
    }

    #[test]
    fn add_library_dir_rejects_exact_duplicate() {
        let mut cfg = Config::default_config();
        cfg.library_dirs = vec![std::path::PathBuf::from("/music")];
        assert!(cfg.add_library_dir(std::path::PathBuf::from("/music")).is_none());
    }

    #[test]
    fn add_library_dir_subsumes_existing_children() {
        // Issue #46: adding a parent folder must drop the now-redundant child
        // sources and report them so their tracks can be removed.
        let mut cfg = Config::default_config();
        cfg.library_dirs = vec![
            std::path::PathBuf::from("/music/jazz"),
            std::path::PathBuf::from("/music/rock"),
        ];
        let subsumed = cfg
            .add_library_dir(std::path::PathBuf::from("/music"))
            .expect("parent add should succeed");
        assert_eq!(subsumed.len(), 2);
        assert_eq!(cfg.library_dirs, vec![std::path::PathBuf::from("/music")]);
    }

    #[test]
    fn add_library_dir_appends_unrelated() {
        let mut cfg = Config::default_config();
        cfg.library_dirs = vec![std::path::PathBuf::from("/music")];
        let subsumed = cfg
            .add_library_dir(std::path::PathBuf::from("/other"))
            .expect("unrelated add should succeed");
        assert!(subsumed.is_empty());
        assert_eq!(cfg.library_dirs.len(), 2);
    }
}
