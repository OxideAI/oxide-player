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

pub const DEFAULT_CAPTURE_DEVICE: &str = "hw:Loopback,1";
pub const DEFAULT_CAPTURE_RATE: u32 = 44100;

#[derive(Clone)]
pub struct DspManager {
    inner: Arc<DspInner>,
}

struct DspInner {
    config_path: PathBuf,
    ws_url: Option<String>,
    capture_device: String,
    capture_rate: u32,
    profiles: Mutex<HashMap<String, DspProfile>>,
}

impl DspManager {
    pub fn new(
        config_path: PathBuf,
        ws_url: Option<String>,
        capture_device: String,
        capture_rate: u32,
    ) -> Self {
        DspManager {
            inner: Arc::new(DspInner {
                config_path,
                ws_url,
                capture_device,
                capture_rate,
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
            &self.inner.capture_device,
            &effective.device,
            self.inner.capture_rate,
        );
        self.write_config(&cfg).await?;
        // Store the full profile (with its EQ bands) so toggling back from
        // bit-perfect restores the user's DSP instead of a stripped copy.
        self.inner
            .profiles
            .lock()
            .await
            .insert(profile.device.clone(), profile);
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
        let (mut write, mut read) = ws.split();
        let path = self.inner.config_path.to_string_lossy().to_string();
        let msg = serde_json::json!({ "Reload": { "config": path } }).to_string();
        write
            .send(Message::Text(msg.into()))
            .await
            .context("send reload to camilladsp")?;

        // CamillaDSP replies with either a success or {"Error": ...} message.
        // Wait briefly for that reply so a rejected reload surfaces as a real
        // error instead of a silent success. Some builds/versions stay silent
        // on success, so a missing reply, a close, or a non-text frame is
        // treated as accepted -- only an explicit {"Error": ...} fails the apply.
        match tokio::time::timeout(std::time::Duration::from_secs(5), read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let v: serde_json::Value = serde_json::from_str(&text)
                    .with_context(|| format!("parse camilladsp reload response: {text}"))?;
                if let Some(err) = v.get("Error") {
                    anyhow::bail!("camilladsp rejected config reload: {err}");
                }
            }
            _ => {}
        }
        write.close().await.ok();
        Ok(())
    }
}
