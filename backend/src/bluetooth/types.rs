use serde::{Deserialize, Serialize};

/// A Bluetooth device discovered by or paired with the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtDevice {
    /// MAC address formatted as `XX:XX:XX:XX:XX:XX`.
    pub address: String,
    /// Human-readable device name (may be `None` during discovery before the
    /// remote name request completes).
    pub name: Option<String>,
    /// User-settable alias (friendly name). Takes precedence over `name` for display.
    pub alias: Option<String>,
    /// Bluetooth class of device (24-bit CoD). Used to filter audio devices.
    pub class: Option<u32>,
    /// Proposed icon name per freedesktop.org icon naming specification.
    pub icon: Option<String>,
    /// Received signal strength indicator in dBm (negative values, higher =
    /// closer).
    pub rssi: Option<i16>,
    /// Whether the device is currently connected.
    pub connected: bool,
    /// Whether the device has been paired with this adapter.
    pub paired: bool,
    /// Whether the device is trusted (auto‑connect enabled).
    pub trusted: bool,
}

