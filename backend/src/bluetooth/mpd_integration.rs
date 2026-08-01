//! MPD output config fragment management for Bluetooth speakers.
//!
//! When a BT speaker is connected via the [`BluetoothManager`], this module
//! creates an MPD `audio_output { type "alsa" device "bluealsa:DEV=<addr>,PROFILE=a2dp" }`
//! fragment in the managed config fragments directory so the speaker appears as
//! an MPD output after an MPD restart.
//!
//! Bluetooth volume is deliberately delegated to the speaker: the PCM disables
//! BlueALSA soft volume and the output uses BlueALSA's per-device hardware mixer.
//! A simple disconnect preserves the fragment so the admin can reconnect later
//! without reconfiguring.

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
    // BlueALSA PCM addressing: disable its software attenuation so MPD's
    // hardware mixer controls the speaker's A2DP volume directly.
    let bt_device = format!(
        "bluealsa:DEV={},PROFILE=a2dp,SOFTVOL=no",
        device.address
    );

    DeviceConfig {
        name: device
            .name
            .clone()
            .unwrap_or_else(|| format!("BT Speaker ({})", device.address)),
        output_type: "alsa".to_string(),
        device: Some(bt_device),
        format: Some("48000:16:2".to_string()),
        mixer_type: Some("hardware".to_string()),
        mixer_device: Some(format!("bluealsa:{}", device.address)),
        mixer_control: Some("A2DP".to_string()),
        dop: false,
    }
}
/// Match both current and legacy BlueALSA PCM strings so reconnecting a
/// speaker upgrades an existing software-volume fragment in place.
fn is_bluetooth_pcm(device: Option<&str>, address: &str) -> bool {
    let Some(device) = device else {
        return false;
    };
    let prefix = format!("bluealsa:DEV={address},PROFILE=a2dp");
    device == prefix || device.starts_with(&format!("{prefix},"))
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

    // The fragment filename comes from the device *name*, which can differ
    // between connections ("BT Speaker (<addr>)" while the name is unknown,
    // then the real name once discovered) and across naming-scheme versions.
    // That left one MPD output per name for the same MAC — all pointing at the
    // same bluealsa PCM, so every output after the first failed to open with
    // "Device or resource busy" and the UI listed the speaker multiple times.
    // Dedupe by MAC: drop any existing fragment that references this address
    // under a different name before writing the canonical one.
    let existing_configs = config_manager.list();
    let existed = existing_configs.iter().any(|c| {
        c.name == cfg.name
            && c.output_type == cfg.output_type
            && c.device == cfg.device
            && c.format == cfg.format
            && c.mixer_type == cfg.mixer_type
            && c.mixer_device == cfg.mixer_device
            && c.mixer_control == cfg.mixer_control
    });
    let mut removed = false;
    for existing in &existing_configs {
        if is_bluetooth_pcm(existing.device.as_deref(), &device.address) && existing.name != cfg.name {
            if config_manager.delete(&existing.name).is_ok() {
                removed = true;
                tracing::info!(
                    "removed stale MPD fragment '{}' for same BT device {}",
                    existing.name,
                    device.address
                );
            }
        }
    }

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

    // Only prompt for an MPD restart when something actually changed (a stale
    // duplicate was removed or the canonical fragment didn't exist yet) — a
    // same-name reconnect must not nag for a pointless restart.
    let changed = removed || !existed;
    set_restart_pending(changed);
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
    // Remove *every* fragment referencing this MAC — not just the one named
    // after the current device name — so a legacy duplicate state (multiple
    // names, one MAC) is fully cleared on forget.
    let matches: Vec<String> = config_manager
        .list()
        .into_iter()
        .filter(|c| is_bluetooth_pcm(c.device.as_deref(), &device.address))
        .map(|c| c.name)
        .collect();
    if matches.is_empty() {
        // Nothing to remove — not an error.
        return Ok(());
    }
    for name in &matches {
        config_manager
            .delete(name)
            .with_context(|| format!("remove MPD config fragment for BT device {}", device.address))?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth::types::BtDevice;
    use crate::devices::config_fragment::ConfigFragmentManager;

    fn mgr() -> (ConfigFragmentManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ConfigFragmentManager::new(dir.path().join("outputs")).unwrap();
        (mgr, dir)
    }

    fn device(name: Option<&str>, address: &str) -> BtDevice {
        BtDevice {
            address: address.to_string(),
            name: name.map(|s| s.to_string()),
            alias: None,
            class: None,
            icon: None,
            rssi: None,
            connected: false,
            paired: false,
            trusted: false,
        }
    }

    /// Repro for the "speaker listed twice / second MPD output fails with
    /// 'Device or resource busy'" bug: the same speaker connected twice —
    /// first with no cached name (fragment becomes "BT Speaker (<addr>)"),
    /// then with its discovered name ("HT-S400") — used to leave two MPD
    /// output fragments for one MAC, both pointing at the same bluealsa PCM.
    #[test]
    fn create_fragment_dedupes_by_mac() {
        let (mgr, _dir) = mgr();
        let addr = "F8:AB:E5:A4:F2:16";

        // First connect: device name not yet resolved -> fallback name.
        create_fragment(&device(None, addr), &mgr, |_| {}).unwrap();

        // Later connect: the real name is known -> different fragment name.
        create_fragment(&device(Some("HT-S400"), addr), &mgr, |_| {}).unwrap();

        let list = mgr.list();
        assert_eq!(list.len(), 1, "one MAC must yield exactly one fragment");
        assert_eq!(list[0].name, "HT-S400");
        assert!(list[0].device.as_deref().unwrap().contains(addr));
    }

    #[test]
    fn create_fragment_uses_hardware_mixer_for_bluetooth_volume() {
        let (mgr, _dir) = mgr();
        let addr = "F8:AB:E5:A4:F2:16";

        create_fragment(&device(Some("HT-S400"), addr), &mgr, |_| {}).unwrap();

        let cfg = mgr.get("HT-S400").unwrap();
        assert_eq!(cfg.mixer_type.as_deref(), Some("hardware"));
        assert_eq!(cfg.mixer_device.as_deref(), Some("bluealsa:F8:AB:E5:A4:F2:16"));
        assert_eq!(cfg.mixer_control.as_deref(), Some("A2DP"));
        assert_eq!(
            cfg.device.as_deref(),
            Some("bluealsa:DEV=F8:AB:E5:A4:F2:16,PROFILE=a2dp,SOFTVOL=no")
        );
    }
    #[test]
    fn create_fragment_upgrades_legacy_software_volume() {
        let (mgr, _dir) = mgr();
        let addr = "F8:AB:E5:A4:F2:16";
        let legacy = DeviceConfig {
            name: "HT-S400".to_string(),
            output_type: "alsa".to_string(),
            device: Some(format!("bluealsa:DEV={addr},PROFILE=a2dp")),
            format: Some("48000:16:2".to_string()),
            mixer_type: Some("software".to_string()),
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&legacy).unwrap();

        let pending = std::sync::atomic::AtomicBool::new(false);
        create_fragment(&device(Some("HT-S400"), addr), &mgr, |changed| {
            pending.store(changed, std::sync::atomic::Ordering::Relaxed)
        })
        .unwrap();

        assert!(pending.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(mgr.get("HT-S400").unwrap().mixer_type.as_deref(), Some("hardware"));
    }


    #[test]
    fn create_fragment_marks_restart_pending_for_new_output() {
        let (mgr, _dir) = mgr();
        let pending = std::sync::atomic::AtomicBool::new(false);

        create_fragment(
            &device(Some("HT-S400"), "F8:AB:E5:A4:F2:16"),
            &mgr,
            |changed| pending.store(changed, std::sync::atomic::Ordering::Relaxed),
        )
        .unwrap();

        assert!(
            pending.load(std::sync::atomic::Ordering::Relaxed),
            "a new MPD output requires a restart"
        );
    }

    /// A device forgotten after a legacy duplicate state (two names, one MAC)
    /// must clear every fragment for that MAC, not just the named one.
    #[test]
    fn remove_fragment_clears_all_for_mac() {
        let (mgr, _dir) = mgr();
        let addr = "AA:BB:CC:DD:EE:FF";
        let mk = |name: &str| crate::devices::config_fragment::DeviceConfig {
            name: name.to_string(),
            output_type: "alsa".to_string(),
            device: Some(format!("bluealsa:DEV={addr},PROFILE=a2dp")),
            format: Some("48000:16:2".to_string()),
            mixer_type: Some("software".to_string()),
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        // Simulate the legacy on-disk state (two names, same MAC) by writing
        // the fragments directly through the manager.
        mgr.create(&mk("BT Speaker (AA:BB:CC:DD:EE:FF)")).unwrap();
        mgr.create(&mk("HT-S400")).unwrap();
        assert_eq!(mgr.list().len(), 2);

        remove_fragment(&device(Some("HT-S400"), addr), &mgr, |_| {}).unwrap();

        assert!(mgr.list().is_empty(), "forget must remove every fragment for the MAC");
    }
}
