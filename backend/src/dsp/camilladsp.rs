use crate::dsp::config::{render_camilladsp_config, CamillaConfig};
use crate::dsp::profile::DspProfile;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;

pub const DEFAULT_CAPTURE_DEVICE: &str = "hw:Loopback,1";
pub const DEFAULT_CAPTURE_RATE: u32 = 48000;

/// Outcome of persisting a DSP profile and attempting to activate its route.
/// Persistence can succeed even when CamillaDSP does not confirm a reload.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DspApplyResult {
    pub device: String,
    pub persisted: bool,
    pub reload_confirmed: bool,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload_error: Option<String>,
}

#[derive(Clone)]
pub struct DspManager {
    inner: Arc<DspInner>,
}

struct DspInner {
    config_path: PathBuf,
    ws_url: Option<String>,
    ws_host: Option<String>,
    ws_port: Option<u16>,
    autostart: bool,
    binary: Option<String>,
    capture_device: String,
    capture_rate: u32,
    profiles: Mutex<HashMap<String, DspProfile>>,
    active_device: Mutex<Option<String>>,
}

impl DspManager {
    pub fn new(
        config_path: PathBuf,
        ws_url: Option<String>,
        capture_device: String,
        capture_rate: u32,
        autostart: bool,
        binary: Option<String>,
    ) -> Self {
        let (ws_host, ws_port) = ws_url
            .as_deref()
            .and_then(parse_ws)
            .map(|(h, p)| (Some(h), Some(p)))
            .unwrap_or((None, None));
        DspManager {
            inner: Arc::new(DspInner {
                config_path,
                ws_url,
                ws_host,
                ws_port,
                autostart,
                binary,
                capture_device,
                capture_rate,
                profiles: Mutex::new(HashMap::new()),
                active_device: Mutex::new(None),
            }),
        }
    }

    /// Start CamillaDSP if it isn't already running and autostart is enabled.
    /// Mirrors the MPD autostart flow: best-effort, never fatal. A config file
    /// must already exist (written by a prior apply) for it to be launched.
    pub async fn ensure_running(&self) {
        if !self.inner.autostart {
            return;
        }
        let (host, port) = match (self.inner.ws_host.clone(), self.inner.ws_port) {
            (Some(h), Some(p)) => (h, p),
            _ => return,
        };
        if !is_localhost(&host) {
            return;
        }
        if tcp_reachable(&host, port).await {
            tracing::info!("CamillaDSP reachable at {host}:{port}");
            return;
        }
        if !self.inner.config_path.exists() {
            tracing::debug!("CamillaDSP config not yet written; will start on first DSP apply");
            return;
        }
        tracing::warn!("CamillaDSP not reachable at {host}:{port}; attempting to start it");
        if let Err(e) = self.start_daemon(&host, port).await {
            tracing::warn!("failed to launch camilladsp: {e}");
            return;
        }
        for attempt in 1..=20 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if tcp_reachable(&host, port).await {
                tracing::info!("CamillaDSP started at {host}:{port}");
                return;
            }
            tracing::debug!("CamillaDSP not up yet, retry {attempt}/20");
        }
        tracing::warn!("launched camilladsp but it did not become reachable at {host}:{port}");
    }

    /// Spawn the `camilladsp` binary (detached) with the websocket server bound
    /// to the configured host/port. Returns Ok immediately; the child runs in
    /// the background (CamillaDSP does not daemonize, so we don't await it).
    async fn start_daemon(&self, host: &str, port: u16) -> Result<()> {
        let binary = self.inner.binary.clone().unwrap_or_else(|| "camilladsp".to_string());
        let mut cmd = tokio::process::Command::new(&binary);
        cmd.arg("--address")
            .arg(host)
            .arg("--port")
            .arg(port.to_string())
            .arg(&self.inner.config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.spawn()
            .map_err(|e| anyhow::anyhow!("failed to launch '{binary}': {e}"))?;
        Ok(())
    }

    /// Poll until the started CamillaDSP daemon accepts TCP connections on its
    /// websocket port. Returns as soon as the listener is up (or gives up at a
    /// fixed deadline) so the subsequent reload connects instead of hitting the
    /// startup race that left a successful apply with an unconfirmed route.
    async fn wait_until_listening(&self, host: &str, port: u16) -> bool {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if tcp_reachable(host, port).await {
                return true;
            }
        }
        tracing::warn!("camilladsp did not become reachable at {host}:{port} after launch");
        false
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
    /// Apply the configured profile for an output, falling back to the
    /// conventional `default` profile or a neutral passthrough profile.
    pub async fn apply_profile_for_device(&self, device: &str) -> Result<DspApplyResult> {
        let profiles = self.inner.profiles.lock().await;
        let mut profile = profiles
            .get(device)
            .cloned()
            .or_else(|| profiles.get("default").cloned())
            .unwrap_or_else(|| DspProfile::bit_perfect(device));
        drop(profiles);
        profile.device = device.to_string();
        self.apply_profile(profile).await
    }

    pub async fn active_device(&self) -> Option<String> {
        self.inner.active_device.lock().await.clone()
    }

    /// Whether the CamillaDSP websocket endpoint accepts TCP connections.
    /// Cheap probe used by the status poller to detect a dropped daemon.
    pub async fn reachable(&self) -> bool {
        match (self.inner.ws_host.clone(), self.inner.ws_port) {
            (Some(host), Some(port)) => tcp_reachable(&host, port).await,
            _ => false,
        }
    }

    /// Re-apply the saved profile for the currently active device and confirm
    /// the reload. Used by the status poller to self-heal the DSP route after
    /// CamillaDSP dropped (crash, BT disconnect, bluealsa restart).
    pub async fn restore_active_dsp_route(&self) -> Result<Option<DspApplyResult>> {
        let device = self.inner.active_device.lock().await.clone();
        let Some(device) = device else {
            return Ok(None);
        };
        let result = self.apply_profile_for_device(&device).await?;
        Ok(Some(result))
    }

    pub async fn clear_active_device(&self) {
        *self.inner.active_device.lock().await = None;
    }


    /// Persist the profile as a CamillaDSP config and signal a reload.
    /// Persist the profile as a CamillaDSP config and report whether the
    /// active route was confirmed by a successful reload acknowledgement.
    pub async fn apply_profile(&self, profile: DspProfile) -> Result<DspApplyResult> {
        profile.validate().context("invalid dsp profile")?;
        let effective = profile.effective();
        let cfg = render_camilladsp_config(
            &effective,
            &self.inner.capture_device,
            &effective.device,
            self.inner.capture_rate,
        );
        self.write_config(&cfg).await?;
        self.inner
            .profiles
            .lock()
            .await
            .insert(profile.device.clone(), profile);

        let mut startup_error = None;
        if self.inner.autostart {
            if let (Some(host), Some(port)) = (self.inner.ws_host.clone(), self.inner.ws_port) {
                if is_localhost(&host) && !tcp_reachable(&host, port).await {
                    match self.start_daemon(&host, port).await {
                        Err(error) => {
                            startup_error = Some(error.to_string());
                        }
                        Ok(()) => {
                            if !self.wait_until_listening(&host, port).await {
                                startup_error = Some(format!(
                                    "camilladsp did not become reachable at {host}:{port} after launch"
                                ));
                            }
                        }
                    }
                }
            }
        }

        let (reload_confirmed, reload_error) = if let Some(error) = startup_error {
            (false, Some(error))
        } else if let Some(url) = &self.inner.ws_url {
            match self.send_reload(url).await {
                Ok(confirmed) => (confirmed, None),
                Err(error) => (false, Some(error.to_string())),
            }
        } else {
            (false, Some("CamillaDSP reload is not configured".to_string()))
        };
        let active = reload_confirmed;
        if reload_confirmed {
            *self.inner.active_device.lock().await = Some(effective.device.clone());
        } else {
            let mut active_device = self.inner.active_device.lock().await;
            if active_device.as_deref() == Some(effective.device.as_str()) {
                *active_device = None;
            }
        }
        Ok(DspApplyResult {
            device: effective.device,
            persisted: true,
            reload_confirmed,
            active,
            reload_error,
        })
    }

    async fn write_config(&self, cfg: &CamillaConfig) -> Result<()> {
        if let Some(parent) = self.inner.config_path.parent() {
            std::fs::create_dir_all(parent).context("create camilladsp config dir")?;
        }
        let yaml = serde_yaml::to_string(cfg).context("serialize camilladsp config")?;
        std::fs::write(&self.inner.config_path, yaml).context("write camilladsp config")?;
        Ok(())
    }

    async fn send_reload(&self, url: &str) -> Result<bool> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut last_connect_error: Option<String> = None;
        let ws = loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let detail = last_connect_error
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default();
                anyhow::bail!("camilladsp websocket at {url} was not reachable{detail}");
            }

            match tokio::time::timeout(
                remaining.min(std::time::Duration::from_millis(500)),
                tokio_tungstenite::connect_async(url),
            )
            .await
            {
                Ok(Ok((ws, _))) => break ws,
                Ok(Err(error)) => {
                    last_connect_error = Some(error.to_string());
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(_) => {
                    last_connect_error = Some("connection timed out".to_string());
                }
            }
        };

        let (mut write, mut read) = ws.split();
        write
            .send(Message::Text(serde_json::json!("Reload").to_string().into()))
            .await
            .context("send reload to camilladsp")?;

        let confirmed = match tokio::time::timeout(std::time::Duration::from_secs(5), read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let v: serde_json::Value = serde_json::from_str(&text)
                    .with_context(|| format!("parse camilladsp reload response: {text}"))?;
                let result = v
                    .get("Reload")
                    .and_then(|reload| reload.get("result"));
                if result == Some(&serde_json::Value::String("Ok".to_string())) {
                    true
                } else {
                    anyhow::bail!(
                        "camilladsp rejected config reload: {}",
                        result.unwrap_or(&v)
                    );
                }
            }
            Ok(Some(Ok(message))) => {
                anyhow::bail!("camilladsp returned non-text reload response: {message:?}");
            }
            Ok(Some(Err(error))) => {
                anyhow::bail!("camilladsp reload websocket error: {error}");
            }
            Ok(None) => anyhow::bail!("camilladsp closed websocket before confirming reload"),
            Err(_) => anyhow::bail!("timed out waiting for camilladsp reload confirmation"),
        };
        write.close().await.ok();
        Ok(confirmed)
    }
}

/// Parse a `ws://host:port` (or `wss://...`) URL into its host and port.
fn parse_ws(url: &str) -> Option<(String, u16)> {
    let without_scheme = url
        .strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))?;
    let without_path = without_scheme.split('/').next().unwrap_or(without_scheme);
    let (host, port) = without_path.rsplit_once(':')?;
    let port = port.parse().ok()?;
    let host = if let Some(h) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        h.to_string()
    } else {
        host.to_string()
    };
    Some((host, port))
}

fn is_localhost(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]" | "0.0.0.0")
}

async fn tcp_reachable(host: &str, port: u16) -> bool {
    // `timeout` returns Result<io::Result<..>, Elapsed>: a refused connection
    // is Ok(Err(..)), so the OUTER is_ok() alone would report it as reachable.
    // Check both layers.
    tokio::time::timeout(Duration::from_secs(2), TcpStream::connect((host, port)))
        .await
        .map(|res| res.is_ok())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::config::PipelineStep;
    use crate::dsp::profile::{DspMode, EqBand, EqBandType, ResamplePreset};

    fn base(device: &str) -> DspProfile {
        DspProfile {
            device: device.to_string(),
            mode: DspMode::BitPerfect,
            target_rate: None,
            preset: ResamplePreset::default(),
            preamp: 0.0,
            eq_bands: vec![],
        }
    }

    fn manager(tmp: &std::path::Path) -> DspManager {
        DspManager::new(
            tmp.join("config.yml"),
            None, // no websocket -> exercises the write-only path
            "hw:Loopback,1".to_string(),
            44100,
            false, // autostart off in tests
            None,
        )
    }

    #[tokio::test]
    async fn apply_writes_resample_config() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(96000);
        p.preset = ResamplePreset::High;
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        m.apply_profile(p.clone()).await.unwrap();

        let yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
        let parsed: CamillaConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.devices.samplerate, 96000);
        assert_eq!(parsed.devices.capture_samplerate, Some(44100));
        // Resample + 1 EQ band = 1 filter pipeline step (on both channels)
        assert_eq!(parsed.pipeline.len(), 1);
        println!("--- resample config ---\n{yaml}");
    }

    #[tokio::test]
    async fn apply_bit_perfect_without_eq_is_passthrough() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(96000);
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::HighShelf,
            freq: 200.0,
            gain: -2.0,
            q: 0.7,
        }];
        m.apply_profile(p).await.unwrap();
        // Switch back to bit-perfect with no EQ -> clean passthrough.
        let mut bp = base("DAC");
        bp.mode = DspMode::BitPerfect;
        m.apply_profile(bp).await.unwrap();

        let yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
        let parsed: CamillaConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(parsed.pipeline.is_empty());
        assert_eq!(parsed.devices.samplerate, 44100);
        assert!(parsed.devices.capture_samplerate.is_none());
        println!("--- bitperfect config ---\n{yaml}");
    }

    #[tokio::test]
    async fn apply_bit_perfect_with_eq_applies_biquads() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        let mut p = base("DAC");
        p.mode = DspMode::BitPerfect;
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::HighShelf,
            freq: 200.0,
            gain: -2.0,
            q: 0.7,
        }];
        m.apply_profile(p).await.unwrap();

        let yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
        let parsed: CamillaConfig = serde_yaml::from_str(&yaml).unwrap();
        // No resampler, 1 EQ band = 1 filter step (applied to both channels)
        assert_eq!(parsed.pipeline.len(), 1);
        match &parsed.pipeline[0] {
            PipelineStep::Filter { channels, names } => {
                assert_eq!(channels, &vec![0u32, 1]);
                assert_eq!(names.len(), 1);
            }
            _ => panic!("expected Filter step"),
        }
        assert_eq!(parsed.devices.samplerate, 44100);
        assert!(parsed.devices.capture_samplerate.is_none());
        println!("--- bitperfect+eq config ---\n{yaml}");
    }

    #[tokio::test]
    async fn apply_rejects_invalid_device() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        for bad in ["", "bad\nname", "bad\0name"] {
            let mut p = base(bad);
            p.mode = DspMode::Resample;
            p.target_rate = Some(48000);
            let r = m.apply_profile(p).await;
            assert!(r.is_err(), "expected rejection for device {bad:?}");
        }
    }

    #[tokio::test]
    async fn apply_resample_without_target_rate_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = None; // user picks Resample + DSP but no rate
        m.apply_profile(p).await.unwrap();

        let yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
        let parsed: CamillaConfig = serde_yaml::from_str(&yaml).unwrap();
        // samplerate falls back to capture_rate; resampler present but target = capture
        assert_eq!(parsed.devices.samplerate, 44100);
        assert_eq!(parsed.devices.capture_samplerate, Some(44100));
        assert!(parsed.devices.resampler.is_some());
        assert!(parsed.pipeline.is_empty()); // no EQ, no pipeline steps
        println!("--- resample no-target config ---\n{yaml}");
    }
    #[tokio::test]
    async fn apply_sends_camilladsp_reload_command() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config_path = tmp.path().join("config.yml");
        let expected = serde_json::json!("Reload").to_string();
        let received = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let message = ws.next().await.unwrap().unwrap().into_text().unwrap().to_string();
            ws.send(Message::Text(r#"{"Reload":{"result":"Ok"}}"#.into()))
                .await
                .unwrap();
            message
        });

        let m = DspManager::new(
            config_path,
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            false,
            None,
        );
        let outcome = m.apply_profile(base("DAC")).await.unwrap();

        assert!(outcome.persisted);
        assert!(outcome.reload_confirmed);
        assert!(outcome.active);
        assert_eq!(received.await.unwrap(), expected);
    }

    #[tokio::test]
    async fn apply_reports_daemon_launch_failure_without_websocket_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let m = DspManager::new(
            tmp.path().join("config.yml"),
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            true,
            Some("/definitely/missing/camilladsp".to_string()),
        );
        let started = tokio::time::Instant::now();
        let outcome = m.apply_profile(base("DAC")).await.unwrap();

        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(outcome.persisted);
        assert!(!outcome.reload_confirmed);
        assert!(outcome.reload_error.as_deref().is_some_and(|error| {
            error.contains("failed to launch")
                && error.contains("definitely/missing/camilladsp")
        }));
    }

    #[tokio::test]
    async fn apply_reports_daemon_exit_before_websocket_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let binary = tmp.path().join("camilladsp");
        std::fs::write(&binary, "#!/bin/sh\nexit 1\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let m = DspManager::new(
            tmp.path().join("config.yml"),
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            true,
            Some(binary.to_string_lossy().into_owned()),
        );
        let started = tokio::time::Instant::now();
        let outcome = m.apply_profile(base("DAC")).await.unwrap();

        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(outcome.persisted);
        assert!(!outcome.reload_confirmed);
        assert!(outcome.reload_error.as_deref().is_some_and(|error| {
            error.contains("did not become reachable")
        }));
    }
    #[tokio::test]
    async fn reload_retries_when_camilladsp_is_starting() {
        let tmp = tempfile::tempdir().unwrap();
        let reserved = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = reserved.local_addr().unwrap();
        drop(reserved);

        let server = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            assert_eq!(
                ws.next().await.unwrap().unwrap().into_text().unwrap(),
                r#""Reload""#
            );
            ws.send(Message::Text(r#"{"Reload":{"result":"Ok"}}"#.into()))
                .await
                .unwrap();
        });

        let m = DspManager::new(
            tmp.path().join("config.yml"),
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            false,
            None,
        );
        assert!(m.send_reload(m.inner.ws_url.as_deref().unwrap()).await.unwrap());
        server.await.unwrap();
    }


    #[tokio::test]
    async fn restore_active_dsp_route_reapplies_active_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = tokio::spawn(async move {
            let mut messages = Vec::new();
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let message = ws.next().await.unwrap().unwrap().into_text().unwrap().to_string();
                ws.send(Message::Text(r#"{"Reload":{"result":"Ok"}}"#.into()))
                    .await
                    .unwrap();
                messages.push(message);
            }
            messages
        });

        let m = DspManager::new(
            tmp.path().join("config.yml"),
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            false,
            None,
        );
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        m.apply_profile(p.clone()).await.unwrap();

        // Active device is set by the successful apply; the restore must
        // re-apply that same device and confirm the reload again.
        let outcome = m.restore_active_dsp_route().await.unwrap().unwrap();
        assert!(outcome.reload_confirmed);
        assert!(outcome.active);
        assert_eq!(m.active_device().await.as_deref(), Some("DAC"));
        let messages = received.await.unwrap();
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|message| message == r#""Reload""#));
    }

    #[tokio::test]
    async fn reload_failure_reports_saved_but_not_active() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            ws.send(Message::Text(
                r#"{"Reload":{"result":{"ConfigValidationError":"invalid config"}}}"#.into(),
            ))
                .await
                .unwrap();
        });
        let m = DspManager::new(
            tmp.path().join("config.yml"),
            Some(format!("ws://{addr}")),
            "hw:Loopback,1".to_string(),
            44100,
            false,
            None,
        );
        *m.inner.active_device.lock().await = Some("DAC".to_string());
        let outcome = m.apply_profile(base("DAC")).await.unwrap();
        assert!(outcome.persisted);
        assert!(outcome.reload_error.as_deref().is_some_and(|error| error.contains("rejected")));
        assert!(!outcome.reload_confirmed);
        assert!(!outcome.active);
        assert_eq!(m.active_device().await, None);
    }
    #[tokio::test]
    async fn apply_profile_for_device_targets_selected_output() {
        let tmp = tempfile::tempdir().unwrap();
        let m = manager(tmp.path());
        m.seed(vec![base("default")]).await;

        let outcome = m.apply_profile_for_device("hw:USB,0").await.unwrap();

        let yaml = std::fs::read_to_string(tmp.path().join("config.yml")).unwrap();
        let parsed: CamillaConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.devices.playback.device, "hw:USB,0");
        assert!(outcome.persisted);
        assert!(!outcome.reload_confirmed);
        assert!(!outcome.active);
        assert_eq!(m.active_device().await, None);
    }

    // Regression: tcp_reachable used `.is_ok()` on the *outer* timeout Result,
    // so a refused connection (Ok(Err(..))) was reported as reachable. The
    // backend then believed CamillaDSP was up when it was down: autostart and
    // on-apply relaunch were skipped, and reloads silently went nowhere —
    // "DSP applied" with no DSP running.
    #[tokio::test]
    async fn tcp_reachable_is_false_when_port_is_closed() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // nothing accepts now -> connection refused
        assert!(!tcp_reachable("127.0.0.1", port).await);
    }

    #[tokio::test]
    async fn tcp_reachable_is_true_when_port_is_open() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(tcp_reachable("127.0.0.1", port).await);
    }
}
