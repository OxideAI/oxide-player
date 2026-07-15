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

impl Config {
    pub fn load(file: Option<&std::path::Path>, cli: &Cli) -> anyhow::Result<Config> {
        let mut config = match file {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?;
                serde_json::from_str(&text).with_context(|| "parsing config json")?
            }
            None => {
                let defaults = Config::default_config();
                let persisted = defaults.data_dir.join("config.json");
                if persisted.exists() {
                    let text = std::fs::read_to_string(&persisted)
                        .with_context(|| format!("reading config {}", persisted.display()))?;
                    serde_json::from_str(&text).with_context(|| "parsing config json")?
                } else {
                    defaults
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
        Ok(config)
    }

    fn default_config() -> Config {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Config {
            mpd_host: "127.0.0.1".to_string(),
            mpd_port: 6600,
            mpd_autostart: true,
            mpd_binary: None,
            mpd_config: None,
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
