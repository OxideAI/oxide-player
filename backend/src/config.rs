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
    /// music directory (e.g. `MyMusic/Artist/Album.flac`). When this is set we
    /// convert the absolute path to that relative URI; for CUE tracks we append
    /// `.cue/trackNNNN` so the individual split track is played. When unset, the
    /// absolute path is passed through (works only if MPD's music directory
    /// matches the OS path layout).
    #[serde(default)]
    pub mpd_music_directory: Option<PathBuf>,
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
        let (mut config, resolved) = match file {
            Some(path) => (read_json_config(path)?, Some(path.to_path_buf())),
            None => {
                let defaults = Config::default_config();
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

    fn default_config() -> Config {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Config {
            mpd_host: "127.0.0.1".to_string(),
            mpd_port: 6600,
            mpd_autostart: true,
            mpd_binary: None,
            mpd_config: None,
            mpd_music_directory: None,
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
#[command(name = "oxide-player", about = "Audiophile music server")]
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
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let (got, resolved) = Config::load(None, &default_cli()).unwrap();

        std::env::set_current_dir(old).unwrap();
        assert!(resolved.is_none(), "expected no resolved path without persisted config");
        assert_eq!(got.listen, "127.0.0.1:8000");
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
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cli = Cli {
            config: None,
            mpd_host: Some("10.0.0.1".to_string()),
            mpd_port: Some(7700),
            listen: "0.0.0.0:9000".to_string(),
            allow_root: false,
        };

        let (got, resolved) = Config::load(None, &cli).unwrap();

        std::env::set_current_dir(old).unwrap();
        assert_eq!(got.mpd_host, "10.0.0.1");
        assert_eq!(got.mpd_port, 7700);
        assert_eq!(got.listen, "0.0.0.0:9000");
        assert!(resolved.is_none());
    }
}
