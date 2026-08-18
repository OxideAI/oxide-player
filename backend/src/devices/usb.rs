use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// A USB audio playback endpoint exposed by ALSA.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UsbAudioDevice {
    /// Stable selection key for the card/device pair.
    pub id: String,
    pub name: String,
    pub card: u32,
    pub device: u32,
    pub alsa_device: String,
}

/// Enumerate USB playback hardware using ALSA's `aplay -l` output.
pub async fn scan() -> Result<Vec<UsbAudioDevice>, String> {
    let output = tokio::process::Command::new("aplay")
        .arg("-l")
        .output()
        .await
        .map_err(|error| format!("could not run aplay: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("aplay exited with {}", output.status)
        } else {
            format!("aplay -l failed: {detail}")
        });
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let sysfs_usb_cards = usb_cards_from_sysfs();
    Ok(parse_aplay_list(&text)
        .into_iter()
        .filter(|candidate| {
            candidate.name.to_ascii_lowercase().contains("usb")
                || sysfs_usb_cards.contains(&candidate.card)
        })
        .collect())
}

/// Parse one ALSA hardware playback listing.
///
/// `aplay -l` emits one row per playback subdevice, for example:
/// `card 1: DAC [USB Audio DAC], device 0: USB Audio [USB Audio]`.
pub fn parse_aplay_list(text: &str) -> Vec<UsbAudioDevice> {
    text.lines().filter_map(parse_aplay_line).collect()
}

fn parse_aplay_line(line: &str) -> Option<UsbAudioDevice> {
    let rest = line.strip_prefix("card ")?;
    let (card_text, rest) = rest.split_once(':')?;
    let card = card_text.trim().parse::<u32>().ok()?;
    let (card_name, device_part) = rest.split_once(", device ")?;
    let device_part = device_part.trim_start();
    let (device_text, device_name) = device_part.split_once(':')?;
    let device = device_text.trim().parse::<u32>().ok()?;

    let card_name = bracket_label(card_name.trim());
    let device_name = bracket_label(device_name.trim());
    let name = if card_name.is_empty() {
        device_name.clone()
    } else if device_name.is_empty() || card_name == device_name {
        card_name.clone()
    } else {
        format!("{card_name} — {device_name}")
    };

    Some(UsbAudioDevice {
        id: format!("alsa:{card}:{device}"),
        name,
        card,
        device,
        alsa_device: format!("hw:{card},{device}"),
    })
}

fn bracket_label(value: &str) -> String {
    value
        .split_once('[')
        .and_then(|(_, rest)| rest.strip_suffix(']'))
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn usb_cards_from_sysfs() -> HashSet<u32> {
    let mut cards = HashSet::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/sound") else {
        return cards;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(card) = name.strip_prefix("card").and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let id = std::fs::read_to_string(entry.path().join("id")).unwrap_or_default();
        let uevent = std::fs::read_to_string(entry.path().join("device/uevent")).unwrap_or_default();
        if id.to_ascii_lowercase().contains("usb")
            || uevent.to_ascii_lowercase().contains("snd_usb_audio")
            || Path::new(&entry.path().join("device").join("usb")).exists()
        {
            cards.insert(card);
        }
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    const APLAY: &str = "**** List of PLAYBACK Hardware Devices ****\ncard 0: PCH [HDA Intel PCH], device 0: ALC [ALC]\ncard 1: DAC [USB Audio DAC], device 0: USB Audio [USB Audio]\ncard 1: DAC [USB Audio DAC], device 1: USB Audio [USB Audio #1]\n";

    #[test]
    fn parses_usb_card_and_each_playback_device() {
        let devices = parse_aplay_list(APLAY);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].id, "alsa:0:0");
        assert_eq!(devices[0].name, "HDA Intel PCH — ALC");
        assert_eq!(devices[2].alsa_device, "hw:1,1");
    }

    #[test]
    fn ignores_non_hardware_lines() {
        assert!(parse_aplay_list("card Loopback: Loopback, device 0: Loopback").is_empty());
    }
}
