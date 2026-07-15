use crate::dsp::config::{render_camilladsp_config, CamillaConfig};
use crate::dsp::profile::DspProfile;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_CAPTURE_DEVICE: &str = "hw:Loopback,1";
const DEFAULT_CAPTURE_RATE: u32 = 44100;

#[derive(Clone)]
pub struct DspManager {
    inner: Arc<DspInner>,
}

struct DspInner {
    config_path: PathBuf,
    ws_url: Option<String>,
    profiles: Mutex<HashMap<String, DspProfile>>,
}

impl DspManager {
    pub fn new(config_path: PathBuf, ws_url: Option<String>) -> Self {
        DspManager {
            inner: Arc::new(DspInner {
                config_path,
                ws_url,
                profiles: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn seed(&self, profiles: Vec<DspProfile>) {
        let mut map = self.inner.profiles.lock().await;
        for p in profiles {
            map.insert(p.device.clone(), p);
        }
    }

    pub async fn get_profile(&self, device: &str) -> Option<DspProfile> {
        self.inner.profiles.lock().await.get(device).cloned()
    }

    pub async fn list_profiles(&self) -> Vec<DspProfile> {
        self.inner.profiles.lock().await.values().cloned().collect()
    }

    /// Persist the profile as a CamillaDSP config and signal a reload.
    pub async fn apply_profile(&self, profile: DspProfile) -> Result<()> {
        // Defense-in-depth: the device string lands verbatim in the CamillaDSP
        // config. Reject empty or control-character-laden values before we write
        // a config that could break (or be abused to influence) audio output.
        if profile.device.trim().is_empty()
            || profile.device.contains('\n')
            || profile.device.contains('\0')
        {
            anyhow::bail!("invalid dsp device name: {:?}", profile.device);
        }
        let effective = profile.effective();
        let cfg = render_camilladsp_config(
            &effective,
            DEFAULT_CAPTURE_DEVICE,
            &effective.device,
            DEFAULT_CAPTURE_RATE,
        );
        self.write_config(&cfg).await?;
        self.inner
            .profiles
            .lock()
            .await
            .insert(effective.device.clone(), effective);
        if let Some(url) = &self.inner.ws_url {
            self.send_reload(url).await?;
        }
        Ok(())
    }

    async fn write_config(&self, cfg: &CamillaConfig) -> Result<()> {
        if let Some(parent) = self.inner.config_path.parent() {
            std::fs::create_dir_all(parent).context("create camilladsp config dir")?;
        }
        let yaml = serde_yaml::to_string(cfg).context("serialize camilladsp config")?;
        std::fs::write(&self.inner.config_path, yaml).context("write camilladsp config")?;
        Ok(())
    }

    async fn send_reload(&self, url: &str) -> Result<()> {
        let (ws, _) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio_tungstenite::connect_async(url),
        )
        .await
        .with_context(|| format!("camilladsp websocket {url} timed out"))?
        .map_err(|e| anyhow::anyhow!("connect camilladsp websocket {url}: {e}"))?;
        let (mut write, _read) = ws.split();
        let path = self.inner.config_path.to_string_lossy().to_string();
        let msg = serde_json::json!({ "Reload": { "config": path } }).to_string();
        write
            .send(Message::Text(msg))
            .await
            .context("send reload to camilladsp")?;
        write.close().await.ok();
        Ok(())
    }
}
