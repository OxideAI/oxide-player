use anyhow::Context;
use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub mpd_host: String,
    pub mpd_port: u16,
    pub listen: String,
    pub data_dir: PathBuf,
    pub library_dirs: Vec<PathBuf>,
    pub static_dir: PathBuf,
    pub camilladsp_config_path: PathBuf,
    pub camilladsp_ws_url: Option<String>,
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
            None => Config::default_config(),
        };

        if let Some(host) = &cli.mpd_host {
            config.mpd_host = host.clone();
        }
        if let Some(port) = cli.mpd_port {
            config.mpd_port = port;
        }
        if cli.listen != "0.0.0.0:8000" {
            config.listen = cli.listen.clone();
        }
        Ok(config)
    }

    fn default_config() -> Config {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Config {
            mpd_host: "127.0.0.1".to_string(),
            mpd_port: 6600,
            listen: "0.0.0.0:8000".to_string(),
            data_dir: cwd.join("data"),
            library_dirs: vec![cwd.join("music")],
            static_dir: cwd.join("../frontend/dist"),
            camilladsp_config_path: cwd.join("data/camilladsp/config.yml"),
            camilladsp_ws_url: Some("ws://127.0.0.1:1234".to_string()),
            default_dsp_profiles: Vec::new(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("library.db")
    }

    pub fn cover_cache_dir(&self) -> PathBuf {
        self.data_dir.join("covers")
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
    #[arg(long, default_value = "0.0.0.0:8000")]
    pub listen: String,
}
