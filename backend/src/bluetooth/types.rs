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

impl BtDevice {
    /// Returns the display name: alias > name > address.
    pub fn display_name(&self) -> String {
        self.alias
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| self.address.clone())
    }

    /// Returns true if this device is an audio output device (speaker/headphone).
    /// Checks Bluetooth Class of Device: Major class 0x04 (Audio/Video) with
    /// minor classes for audio output devices.
    pub fn is_audio_output(&self) -> bool {
        let Some(class) = self.class else {
            return false;
        };
        // Major device class bits 8-12 (shifted 8)
        let major = (class >> 8) & 0x1F;
        // Minor device class bits 2-7 (shifted 2)
        let minor = (class >> 2) & 0x3F;
        // Major class 0x04 = Audio/Video
        if major != 0x04 {
            return false;
        }
        // Minor classes for audio output:
        // 0x04 = Headset, 0x08 = Hands-free, 0x10 = Microphone,
        // 0x14 = Loudspeaker, 0x18 = Headphones, 0x1C = Portable Audio,
        // 0x20 = Car Audio, 0x28 = HiFi Audio
        matches!(minor, 0x04 | 0x08 | 0x14 | 0x18 | 0x1C | 0x20 | 0x28)
    }

    /// Returns a human-readable device type description based on class.
    pub fn device_type(&self) -> Option<&'static str> {
        let class = self.class?;
        let major = (class >> 8) & 0x1F;
        let minor = (class >> 2) & 0x3F;
        if major != 0x04 {
            return None;
        }
        Some(match minor {
            0x04 => "Headset",
            0x08 => "Hands-free",
            0x10 => "Microphone",
            0x14 => "Speaker",
            0x18 => "Headphones",
            0x1C => "Portable Audio",
            0x20 => "Car Audio",
            0x28 => "HiFi Audio",
            _ => "Audio Device",
        })
    }
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
