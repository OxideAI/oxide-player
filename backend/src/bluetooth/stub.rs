use crate::bluetooth::input::BluetoothInputManager;
use crate::bluetooth::types::BtDevice;
use anyhow::{bail, Result};
use std::sync::Arc;

/// Stub [`BluetoothManager`] for non‑Linux platforms.
///
/// Every operation returns `"Bluetooth not supported on this platform"`.
#[derive(Clone)]
pub struct BluetoothManager {
    inner: Arc<Inner>,
}

struct Inner {
    input: BluetoothInputManager,
}

impl BluetoothManager {
    /// Create a disabled stub manager.
    pub async fn new() -> Self {
        BluetoothManager {
            inner: Arc::new(Inner {
                input: BluetoothInputManager::new(),
            }),
        }
    }

    // -- discovery --

    /// Not supported on this platform.
    pub async fn start_discovery(&self, _timeout_secs: u32) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// No‑op.
    pub async fn stop_discovery(&self) {}

    /// Always `false`.
    pub async fn is_discovering(&self) -> bool {
        false
    }

    // -- pairing / bonding --

    /// Not supported on this platform.
    pub async fn pair(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    // -- connection --

    /// Not supported on this platform.
    pub async fn connect(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Not supported on this platform.
    pub async fn disconnect(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Not supported on this platform.
    pub async fn forget(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    // -- queries --

    /// Always fails — Bluetooth is not available on this platform.
    pub async fn check_available(&self) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Always returns an empty list.
    pub async fn list_devices(&self) -> Vec<BtDevice> {
        Vec::new()
    }

    /// Not supported on this platform.
    pub async fn set_alias(&self, _address: &str, _name: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Not supported on this platform.
    pub async fn test_connectivity(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Not supported on this platform.
    pub async fn wake_and_connect(&self, _address: &str) -> Result<()> {
        bail!("Bluetooth is not supported on this platform (requires Linux + BlueZ)")
    }

    /// Access the Bluetooth A2DP Sink input manager.
    pub fn input(&self) -> BluetoothInputManager {
        self.inner.input.clone()
    }
}
