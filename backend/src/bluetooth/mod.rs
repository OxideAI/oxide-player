//! Bluetooth audio support (output + input).
//!
//! ## Architecture
//!
//! On **Linux** the module uses the `bluer` crate to talk to BlueZ over D-Bus,
//! providing device discovery, pairing, connection, and state monitoring.
//! Connected BT speakers are wired into the MPD output config fragment system.
//! BT input (A2DP sink) routes through ALSA loopback → CamillaDSP.
//!
//! On **non‑Linux** (macOS, Windows) the module compiles as a stub whose
//! operations all return `"Bluetooth not available"`.
//!
//! ## Platform gate
//!
//! The implementation module behind the public re‑export is chosen at compile
//! time via `#[cfg(target_os = "linux")]` so the crate compiles everywhere.

pub mod types;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::BluetoothManager;

#[cfg(not(target_os = "linux"))]
mod stub;
#[cfg(not(target_os = "linux"))]
pub use stub::BluetoothManager;

/// MPD output config fragment management for Bluetooth speakers.
pub mod mpd_integration;

/// Bluetooth A2DP Sink input management (ALSA loopback + CamillaDSP).
pub mod input;
