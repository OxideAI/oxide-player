//! MPD output config fragment management for Bluetooth speakers.
//!
//! When a BT speaker is connected via the [`BluetoothManager`], this module
//! creates an MPD `audio_output { type "alsa" device "bluealsa:DEV=<addr>,PROFILE=a2dp" }`
//! fragment in the managed config fragments directory so the speaker appears as
//! an MPD output after an MPD restart.
//!
//! The fragment is created on connect and removed on forget (unpair). A simple
//! disconnect preserves the fragment so the admin can reconnect later without
//! reconfiguring.

use crate::bluetooth::types::BtDevice;
use crate::devices::config_fragment::{ConfigFragmentManager, DeviceConfig};
use anyhow::{Context, Result};
use std::path::Path;

/// Sanitised name for the MPD output config fragment. Given the user‑visible
/// device name (e.g. `"Sony WH-1000XM4"`), produce a filesystem‑safe name.
fn fragment_name(device: &BtDevice) -> String {
    let base = device
        .name
        .as_deref()
        .unwrap_or("bluetooth-speaker")
        .trim();
    format!("bluetooth-{}", base)
}

/// Build the device config struct for the MPD fragment.
fn build_device_config(device: &BtDevice) -> DeviceConfig {
    // BlueALSA PCM addressing: `bluealsa:DEV=<addr>,PROFILE=a2dp`.
    let bt_device = format!("bluealsa:DEV={},PROFILE=a2dp", device.address);

    DeviceConfig {
        name: device
            .name
            .clone()
            .unwrap_or_else(|| format!("BT Speaker ({})", device.address)),
        output_type: "alsa".to_string(),
        device: Some(bt_device),
        format: Some("48000:16:2".to_string()),
        mixer_type: Some("software".to_string()),
        mixer_device: None,
        dop: false,
    }
}

/// Create an MPD output config fragment for the given Bluetooth device.
///
/// The fragment is written to the managed config fragments directory and the
/// restart‑pending flag is set so the UI prompts the user to restart MPD.
///
/// If a fragment for this device already exists (e.g. after a reconnect) it
/// is silently overwritten — the content is identical.
pub fn create_fragment(
    device: &BtDevice,
    config_manager: &ConfigFragmentManager,
    set_restart_pending: impl Fn(bool),
) -> Result<()> {
    let cfg = build_device_config(device);
    let name = fragment_name(device);

    config_manager
        .create(&cfg)
        .or_else(|e| {
            // If the file already exists (reconnect), update it instead.
            if config_manager.exists(&name) {
                config_manager.update(&name, &cfg)
            } else {
                Err(e)
            }
        })
        .with_context(|| format!("write MPD config fragment for BT device {}", device.address))?;

    set_restart_pending(true);
    tracing::info!(
        "Created MPD output config fragment for BT device '{}' ({})",
        device.name.as_deref().unwrap_or("unknown"),
        device.address
    );
    Ok(())
}

/// Remove the MPD output config fragment for the given Bluetooth device.
///
/// Called when the device is unpaired (forgotten). The restart‑pending flag
/// is set so the UI prompts the user to restart MPD.
pub fn remove_fragment(
    device: &BtDevice,
    config_manager: &ConfigFragmentManager,
    set_restart_pending: impl Fn(bool),
) -> Result<()> {
    let name = fragment_name(device);
    if !config_manager.exists(&name) {
        // Nothing to remove — not an error.
        return Ok(());
    }
    config_manager
        .delete(&name)
        .with_context(|| format!("remove MPD config fragment for BT device {}", device.address))?;

    set_restart_pending(true);
    tracing::info!(
        "Removed MPD output config fragment for BT device '{}' ({})",
        device.name.as_deref().unwrap_or("unknown"),
        device.address
    );
    Ok(())
}

/// Get the fragment config for a connected device, if one exists.
#[allow(dead_code)]
pub fn get_fragment(
    device: &BtDevice,
    config_manager: &ConfigFragmentManager,
) -> Option<DeviceConfig> {
    let name = fragment_name(device);
    config_manager.get(&name).ok()
}

/// Check whether a fragment already exists for this device.
#[allow(dead_code)]
pub fn fragment_exists(device: &BtDevice, config_manager: &ConfigFragmentManager) -> bool {
    let name = fragment_name(device);
    config_manager.exists(&name)
}

/// The directory where BT speaker fragments are stored, nested under the
/// global MPD outputs directory so the IncludeInjector picks them up.
#[allow(dead_code)]
pub fn fragment_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("mpd-outputs.d")
}
