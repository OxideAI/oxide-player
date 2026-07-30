use serde::{Deserialize, Serialize};

/// A Bluetooth device discovered by or paired with the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtDevice {
    /// MAC address formatted as `XX:XX:XX:XX:XX:XX`.
    pub address: String,
    /// Human-readable device name (may be `None` during discovery before the
    /// remote name request completes).
    pub name: Option<String>,
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

/// Sub‑type of a Bluetooth event published via the broadcast channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtEventKind {
    /// A new device was discovered during an active scan.
    DeviceFound,
    /// A previously found device is no longer reachable (out of range).
    DeviceLost,
    /// A paired device has connected.
    Connected,
    /// A connected device has disconnected.
    Disconnected,
    /// A device has been paired (bonded).
    Paired,
}

/// An event published by [`super::BluetoothManager`]'s broadcast channel.
///
/// Subscribe via [`BluetoothManager::subscribe_events`] to react to device
/// lifecycle changes in real time.
#[derive(Debug, Clone)]
pub struct BtEvent {
    pub kind: BtEventKind,
    pub device: BtDevice,
}
