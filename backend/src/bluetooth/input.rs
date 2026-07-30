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
//! Phone ──A2DP──▶ bluealsad ──PCM──▶ bluealsa-aplay ──▶ hw:Loopback,0,0
//!                                                           │
//!                                                    CamillaDSP captures
//!                                                    from hw:Loopback,0,1
//!                                                           │
//!                                                         DAC
//! ```
//!
//! ## Platform support
//!
//! The real implementation requires Linux with BlueZ, BlueALSA, and the
//! `snd-aloop` kernel module. On other platforms this module compiles as a
//! stub that always returns "not supported".

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};

/// Manages the Bluetooth A2DP Sink input pipeline.
///
/// Uses interior mutability so that `enable`/`disable` take `&self` and
/// the caller need not hold a mutex across `.await`.
#[derive(Clone)]
pub struct BluetoothInputManager {
    enabled: std::sync::Arc<AtomicBool>,
}

impl BluetoothInputManager {
    /// Create a new input manager. On non‑Linux platforms the manager is always
    /// disabled and all operations return errors.
    pub fn new() -> Self {
        BluetoothInputManager {
            enabled: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }

    /// Enable the A2DP Sink input pipeline.
    ///
    /// On Linux this starts `bluealsa-aplay` pointed at the ALSA loopback and
    /// reconfigures CamillaDSP's capture device. Returns an error when the
    /// pipeline cannot be started (e.g. no phone connected, `snd-aloop` not
    /// loaded).
    /// On other platforms this always returns an error.
    #[cfg(target_os = "linux")]
    pub async fn enable(&self) -> Result<()> {
        // TODO(U4): Linux implementation
        anyhow::bail!("A2DP sink input is not yet implemented on Linux (U4)")
    }

    /// Stub for non‑Linux.
    #[cfg(not(target_os = "linux"))]
    pub async fn enable(&self) -> Result<()> {
        anyhow::bail!("A2DP sink input is not supported on this platform (requires Linux + BlueZ + BlueALSA)")
    }

    /// Disable the A2DP Sink input pipeline.
    ///
    /// Stops `bluealsa-aplay` and restores CamillaDSP's original capture device.
    #[cfg(target_os = "linux")]
    pub async fn disable(&self) -> Result<()> {
        // TODO(U4): Linux implementation
        anyhow::bail!("A2DP sink input is not yet implemented on Linux (U4)")
    }

    /// Stub for non‑Linux.
    #[cfg(not(target_os = "linux"))]
    pub async fn disable(&self) -> Result<()> {
        anyhow::bail!("A2DP sink input is not supported on this platform (requires Linux + BlueZ + BlueALSA)")
    }

    /// Returns whether the A2DP Sink input pipeline is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Whether a phone or tablet is currently streaming audio via A2DP.
    #[cfg(target_os = "linux")]
    pub fn is_streaming(&self) -> bool {
        // TODO(U4): check bluealsa-aplay process state
        false
    }

    /// Stub for non‑Linux.
    #[cfg(not(target_os = "linux"))]
    pub fn is_streaming(&self) -> bool {
        false
    }
}
