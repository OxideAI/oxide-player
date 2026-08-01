//! Bluetooth input (A2DP Sink) management.
//!
//! When enabled, Oxide registers as an A2DP Sink via BlueALSA so phones and
//! tablets can discover and stream audio to it. Incoming audio is routed
//! through an ALSA loopback device (`snd-aloop`) into CamillaDSP, which
//! applies resampling and EQ before sending it to the DAC.
//!
//! ## Pipeline
//!
//! ```text
//! Phone ──A2DP──▶ bluealsa ──PCM──▶ bluealsa-aplay ──▶ oxide_loopback
//!                                                        │
//!                                                  CamillaDSP captures
//!                                                  from hw:Loopback,1
//!                                                        │
//!                                                       DAC
//! ```
//!
//! ## Platform support
//!
//! The real implementation requires Linux with BlueZ, BlueALSA, and the
//! `snd-aloop` kernel module. On other platforms this module compiles as a
//! stub that always returns "not supported".

use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::Context;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "linux")]
use std::process::Stdio;
#[cfg(target_os = "linux")]
use tokio::process::Command;
#[cfg(target_os = "linux")]
use tokio::sync::Mutex;

/// Manages the Bluetooth A2DP Sink input pipeline.
///
/// Uses interior mutability so that `enable`/`disable` take `&self` and
/// the caller need not hold a mutex across `.await`.
#[derive(Clone)]
pub struct BluetoothInputManager {
    enabled: std::sync::Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    child: std::sync::Arc<Mutex<Option<tokio::process::Child>>>,
    #[cfg(target_os = "linux")]
    streaming: std::sync::Arc<AtomicBool>,
}

impl BluetoothInputManager {
    /// Create a new input manager. On non-Linux platforms the manager is always
    /// disabled and all operations return errors.
    pub fn new() -> Self {
        BluetoothInputManager {
            enabled: std::sync::Arc::new(AtomicBool::new(false)),
            #[cfg(target_os = "linux")]
            child: std::sync::Arc::new(Mutex::new(None)),
            #[cfg(target_os = "linux")]
            streaming: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// Enable the A2DP Sink input pipeline.
    ///
    /// The BlueALSA daemon is installed and supervised by the installer. This
    /// process bridges any connected A2DP source into the shared ALSA PCM that
    /// CamillaDSP captures.
    #[cfg(target_os = "linux")]
    pub async fn enable(&self) -> Result<()> {
        let mut slot = self.child.lock().await;
        if let Some(child) = slot.as_mut() {
            match child.try_wait() {
                Ok(None) => {
                    self.enabled.store(true, Ordering::Relaxed);
                    return Ok(());
                }
                Ok(Some(_)) | Err(_) => {
                    slot.take();
                }
            }
        }

        let mut child = Command::new("bluealsa-aplay")
            .args(["--pcm=oxide_loopback", "--single-audio"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start bluealsa-aplay (is bluez-alsa-utils installed?)")?;
        child.kill_on_drop(true);
        *slot = Some(child);
        drop(slot);

        self.enabled.store(true, Ordering::Relaxed);
        self.streaming.store(false, Ordering::Relaxed);
        self.spawn_monitor();
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn spawn_monitor(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                let exited = {
                    let mut slot = manager.child.lock().await;
                    match slot.as_mut() {
                        Some(child) => match child.try_wait() {
                            Ok(Some(_)) => true,
                            Ok(None) => false,
                            Err(error) => {
                                tracing::warn!("bluealsa-aplay status check failed: {error}");
                                true
                            }
                        },
                        None => return,
                    }
                };

                if exited {
                    manager.child.lock().await.take();
                    manager.enabled.store(false, Ordering::Relaxed);
                    manager.streaming.store(false, Ordering::Relaxed);
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    /// Stub for non-Linux.
    #[cfg(not(target_os = "linux"))]
    pub async fn enable(&self) -> Result<()> {
        anyhow::bail!("A2DP sink input is not supported on this platform (requires Linux + BlueZ + BlueALSA)")
    }

    /// Disable the A2DP Sink input pipeline.
    #[cfg(target_os = "linux")]
    pub async fn disable(&self) -> Result<()> {
        let child = self.child.lock().await.take();
        if let Some(mut child) = child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        self.enabled.store(false, Ordering::Relaxed);
        self.streaming.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Stub for non-Linux.
    #[cfg(not(target_os = "linux"))]
    pub async fn disable(&self) -> Result<()> {
        anyhow::bail!("A2DP sink input is not supported on this platform (requires Linux + BlueZ + BlueALSA)")
    }

    /// Returns whether the A2DP Sink input pipeline is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Whether a phone or tablet is currently streaming audio via A2DP.
    ///
    /// BlueALSA's bridge process does not expose a portable stream-state API;
    /// this remains false until a future D-Bus stream monitor is added.
    #[cfg(target_os = "linux")]
    pub fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::Relaxed)
    }

    /// Stub for non-Linux.
    #[cfg(not(target_os = "linux"))]
    pub fn is_streaming(&self) -> bool {
        false
    }
}
