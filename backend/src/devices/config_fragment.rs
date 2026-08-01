use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A managed MPD output device config fragment.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub name: String,
    pub output_type: String,
    pub device: Option<String>,
    pub format: Option<String>,
    pub mixer_type: Option<String>,
    pub mixer_device: Option<String>,
    pub mixer_control: Option<String>,
    pub dop: bool,
}

/// Field-level validation result.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub errors: HashMap<String, String>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_error_string(self) -> Option<String> {
        if self.errors.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = self
            .errors
            .into_iter()
            .map(|(field, msg)| format!("{field}: {msg}"))
            .collect();
        parts.sort();
        Some(parts.join("; "))
    }
}

/// Sanitize a device name into a safe filename.
/// Lowercased, non-alphanumeric/dot/underscore/hyphen → `-`, collapse runs,
/// trim leading/trailing `-`.
pub fn sanitize_device_name(name: &str) -> String {
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '-' })
        .collect();
    // Collapse runs of '-'
    let collapsed: String = sanitized
        .chars()
        .fold(String::new(), |mut acc, c| {
            if c == '-' && acc.ends_with('-') {
                // skip duplicate
            } else {
                acc.push(c);
            }
            acc
        });
    collapsed.trim_matches('-').to_string()
}

/// Validate device config fields. Returns structured field-level errors.
pub fn validate_config(
    name: &str,
    output_type: &str,
    device: Option<&str>,
    format: Option<&str>,
    mixer_type: Option<&str>,
    mixer_device: Option<&str>,
    dop: bool,
) -> ValidationResult {
    let mut result = ValidationResult::default();

    let name = name.trim();
    if name.is_empty() {
        result.errors.insert("name".to_string(), "must not be empty".to_string());
    } else if name.contains('\n') || name.contains('\0') {
        result.errors.insert("name".to_string(), "must not contain newlines or null bytes".to_string());
    }

    let output_type = output_type.trim();
    if output_type.is_empty() {
        result.errors.insert("type".to_string(), "must not be empty".to_string());
    }
    // Unknown types are accepted for forward-compat, but warn through validation pass-through

    if let Some(d) = device {
        if d.contains('\n') || d.contains('\0') {
            result.errors.insert("device".to_string(), "must not contain newlines or null bytes".to_string());
        }
    }

    if let Some(f) = format {
        let f = f.trim();
        if !f.is_empty() {
            // MPD format: BITS:RATE:CHANNELS (e.g. "44100:16:2")
            let parts: Vec<&str> = f.split(':').collect();
            if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
                result.errors.insert("format".to_string(), "must match BITS:RATE:CHANNELS (e.g. '44100:16:2')".to_string());
            }
        }
    }

    if let Some(mt) = mixer_type {
        let mt = mt.trim();
        if !mt.is_empty() && !matches!(mt, "hardware" | "software" | "none") {
            result.errors.insert("mixer_type".to_string(), "must be 'hardware', 'software', or 'none'".to_string());
        }
    }

    if let Some(md) = mixer_device {
        if md.contains('\n') || md.contains('\0') {
            result.errors.insert("mixer_device".to_string(), "must not contain newlines or null bytes".to_string());
        }
    }

    let _ = dop; // boolean, no extra validation needed
    result
}

/// Manages MPD audio output config fragment files on disk.
#[derive(Debug, Clone)]
pub struct ConfigFragmentManager {
    dir: PathBuf,
}

impl ConfigFragmentManager {
    /// Create a new manager for the given fragment directory. Creates the
    /// directory if it does not exist.
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// List all managed device config fragments. Reads each `.conf` file in the
    /// fragment directory and extracts the name, type, and device fields.
    pub fn list(&self) -> Vec<DeviceConfig> {
        let mut configs = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return configs,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("conf") {
                continue;
            }
            if let Ok(cfg) = Self::parse_fragment(&path) {
                configs.push(cfg);
            }
        }
        configs
    }

    fn filename_for(name: &str) -> String {
        format!("{}.conf", sanitize_device_name(name))
    }

    /// Generate the `audio_output {}` block content.
    fn render_fragment(config: &DeviceConfig) -> String {
        let mut out = String::from("audio_output {\n");
        out.push_str(&format!("    type        \"{}\"\n", config.output_type));
        out.push_str(&format!("    name        \"{}\"\n", config.name));

        if let Some(device) = &config.device {
            if !device.trim().is_empty() {
                out.push_str(&format!("    device      \"{}\"\n", device));
            }
        }
        if let Some(fmt) = &config.format {
            if !fmt.trim().is_empty() {
                out.push_str(&format!("    format      \"{}\"\n", fmt));
            }
        }
        if let Some(mt) = &config.mixer_type {
            if !mt.trim().is_empty() {
                out.push_str(&format!("    mixer_type  \"{}\"\n", mt));
            }
        }
        if let Some(md) = &config.mixer_device {
            if !md.trim().is_empty() {
                out.push_str(&format!("    mixer_device \"{}\"\n", md));
            }
        }
        if let Some(mc) = &config.mixer_control {
            if !mc.trim().is_empty() {
                out.push_str(&format!("    mixer_control \"{}\"\n", mc));
            }
        }
        if config.dop {
            out.push_str("    dop         \"yes\"\n");
        }

        out.push_str("}\n");
        out
    }

    /// Parse a fragment file and extract the device config fields.
    fn parse_fragment(path: &Path) -> io::Result<DeviceConfig> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse_content(&content))
    }

    fn parse_content(content: &str) -> DeviceConfig {
        let mut name = String::new();
        let mut output_type = String::new();
        let mut device: Option<String> = None;
        let mut format: Option<String> = None;
        let mut mixer_type: Option<String> = None;
        let mut mixer_device: Option<String> = None;
        let mut mixer_control: Option<String> = None;
        let mut dop = false;

        for line in content.lines() {
            let line = line.trim();
            // Skip audio_output { } brackets and comments
            if line.is_empty() || line == "audio_output {" || line == "}" || line.starts_with('#') {
                continue;
            }
            // Split on first whitespace after the key
            if let Some((key, rest)) = line.split_once(char::is_whitespace) {
                let val = rest.trim().trim_matches('"');
                match key {
                    "name" => name = val.to_string(),
                    "type" => output_type = val.to_string(),
                    "device" => device = Some(val.to_string()),
                    "format" => format = Some(val.to_string()),
                    "mixer_type" => mixer_type = Some(val.to_string()),
                    "mixer_device" => mixer_device = Some(val.to_string()),
                    "mixer_control" => mixer_control = Some(val.to_string()),
                    "dop" => dop = val.eq_ignore_ascii_case("yes"),
                    _ => {}
                }
            }
        }

        DeviceConfig {
            name,
            output_type,
            device,
            format,
            mixer_type,
            mixer_device,
            mixer_control,
            dop,
        }
    }

    /// Create a new device config fragment. Validates fields, sanitizes the
    /// name, and writes the file atomically (temp file + rename).
    pub fn create(&self, config: &DeviceConfig) -> io::Result<()> {
        let filename = Self::filename_for(&config.name);
        let path = self.dir.join(&filename);
        let content = Self::render_fragment(config);
        atomic_write(&path, &content)?;
        Ok(())
    }

    /// Update an existing device config fragment. The `old_name` is used to
    /// find the existing file; if the sanitized name changed, the old file is
    /// removed and a new one is written.
    pub fn update(&self, old_name: &str, config: &DeviceConfig) -> io::Result<()> {
        // Remove old file
        let old_filename = Self::filename_for(old_name);
        let old_path = self.dir.join(&old_filename);
        let _ = fs::remove_file(&old_path); // Ignore if already gone

        // Write new file
        let new_filename = Self::filename_for(&config.name);
        let new_path = self.dir.join(&new_filename);
        let content = Self::render_fragment(config);
        atomic_write(&new_path, &content)?;
        Ok(())
    }

    /// Delete a device config fragment by name.
    pub fn delete(&self, name: &str) -> io::Result<()> {
        let filename = Self::filename_for(name);
        let path = self.dir.join(&filename);
        fs::remove_file(&path)?;
        Ok(())
    }

    /// Read a device config fragment by name.
    pub fn get(&self, name: &str) -> io::Result<DeviceConfig> {
        let filename = Self::filename_for(name);
        let path = self.dir.join(&filename);
        Self::parse_fragment(&path)
    }

    /// Check if a fragment file exists for the given name.
    pub fn exists(&self, name: &str) -> bool {
        let filename = Self::filename_for(name);
        self.dir.join(&filename).exists()
    }

    /// Count existing fragment files.
    pub fn count(&self) -> usize {
        self.list().len()
    }
}

/// Write `content` to `path` atomically using a temp file + rename.
fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fragment")
    );
    let tmp_path = dir.join(&tmp_name);

    let mut f = fs::File::create(&tmp_path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?; // flush to disk before rename
    drop(f);

    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(test_name: &str) -> PathBuf {
        let pid = std::process::id();
        let tid = std::thread::current().id();
        std::env::temp_dir().join(format!("oxide_test_fragments_{}_{:?}_{}", pid, tid, test_name))
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    fn run_test<T>(_name: &str, f: impl FnOnce() -> T) -> T {
        f()
    }

    #[test]
    fn test_sanitize_device_name() {
        assert_eq!(sanitize_device_name("My ALSA Device"), "my-alsa-device");
        assert_eq!(sanitize_device_name("  Spaces!  "), "spaces");
        assert_eq!(sanitize_device_name("USB Audio"), "usb-audio");
        assert_eq!(sanitize_device_name(""), "");
        assert_eq!(sanitize_device_name("a.b-c_d"), "a.b-c_d");
        assert_eq!(sanitize_device_name("---"), "");
    }

    #[test]
    fn test_validate_config_valid() {
        let r = validate_config("My Device", "alsa", Some("hw:0,0"), Some("44100:16:2"), Some("hardware"), None, false);
        assert!(r.is_valid(), "valid config should pass: {:?}", r.errors);
    }

    #[test]
    fn test_validate_config_empty_name() {
        let r = validate_config("", "alsa", None, None, None, None, false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("name"));
    }

    #[test]
    fn test_validate_config_empty_type() {
        let r = validate_config("Dev", "", None, None, None, None, false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("type"));
    }

    #[test]
    fn test_validate_config_bad_format() {
        let r = validate_config("Dev", "alsa", None, Some("44100:foo:2"), None, None, false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("format"));
    }

    #[test]
    fn test_validate_config_bad_mixer_type() {
        let r = validate_config("Dev", "alsa", None, None, Some("invalid"), None, false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("mixer_type"));
    }

    #[test]
    fn test_validate_config_newline_in_name() {
        let r = validate_config("Dev\nice", "alsa", None, None, None, None, false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("name"));
    }

    #[test]
    fn test_validate_config_all_optional_omitted() {
        let r = validate_config("Dev", "alsa", None, None, None, None, false);
        assert!(r.is_valid(), "all optionals omitted should pass");
    }

    #[test]
    fn test_validate_config_unknown_type_forward_compat() {
        let r = validate_config("Dev", "custom_output", None, None, None, None, false);
        assert!(r.is_valid(), "unknown type should pass through for forward compat");
    }

    #[test]
    fn test_validate_config_mixer_device_newline() {
        let r = validate_config("Dev", "alsa", None, None, None, Some("bad\n"), false);
        assert!(!r.is_valid());
        assert!(r.errors.contains_key("mixer_device"));
    }

    #[test]
    fn test_create_fragment() {
        let dir = temp_dir("test_create_fragment");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "Test ALSA".to_string(),
            output_type: "alsa".to_string(),
            device: Some("hw:0,0".to_string()),
            format: Some("44100:16:2".to_string()),
            mixer_type: Some("hardware".to_string()),
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&cfg).unwrap();

        let filename = format!("{}.conf", sanitize_device_name("Test ALSA"));
        let path = dir.join(&filename);
        assert!(path.exists(), "fragment file should exist");

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("audio_output {"));
        assert!(content.contains("type        \"alsa\""));
        assert!(content.contains("name        \"Test ALSA\""));
        assert!(content.contains("device      \"hw:0,0\""));
        assert!(content.contains("format      \"44100:16:2\""));
        assert!(content.contains("mixer_type  \"hardware\""));

        cleanup(&dir);
    }

    #[test]
    fn test_create_fragment_with_dop() {
        let dir = temp_dir("test_create_fragment_with_dop");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "DoP Device".to_string(),
            device: None,
            output_type: "alsa".to_string(),
            format: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: true,
        };
        mgr.create(&cfg).unwrap();

        let filename = format!("{}.conf", sanitize_device_name("DoP Device"));
        let path = dir.join(&filename);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("dop         \"yes\""));

        cleanup(&dir);
    }

    #[test]
    fn test_list_fragments() {
        let dir = temp_dir("test_list_fragments");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        // Empty dir → empty list
        assert!(mgr.list().is_empty());

        let cfg1 = DeviceConfig {
            name: "ALSA One".to_string(),
            device: None,
            output_type: "alsa".to_string(),
            format: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        let cfg2 = DeviceConfig {
            name: "Pulse Two".to_string(),
            output_type: "pulse".to_string(),
            format: None,
            device: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&cfg1).unwrap();
        mgr.create(&cfg2).unwrap();

        let list = mgr.list();
        assert_eq!(list.len(), 2);

        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"ALSA One"));
        assert!(names.contains(&"Pulse Two"));

        cleanup(&dir);
    }

    #[test]
    fn test_update_fragment() {
        let dir = temp_dir("test_update_fragment");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "Original".to_string(),
            output_type: "alsa".to_string(),
            format: None,
            device: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&cfg).unwrap();

        let updated = DeviceConfig {
            name: "Renamed".to_string(),
            output_type: "pulse".to_string(),
            format: None,
            device: Some("new-device".to_string()),
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.update("Original", &updated).unwrap();

        // Old file gone
        let old_filename = format!("{}.conf", sanitize_device_name("Original"));
        assert!(!dir.join(&old_filename).exists());

        // New file exists
        let new_filename = format!("{}.conf", sanitize_device_name("Renamed"));
        assert!(dir.join(&new_filename).exists());

        // Parse back
        let list = mgr.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Renamed");

        cleanup(&dir);
    }

    #[test]
    fn test_delete_fragment() {
        let dir = temp_dir("test_delete_fragment");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "ToDelete".to_string(),
            output_type: "alsa".to_string(),
            format: None,
            device: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&cfg).unwrap();
        assert_eq!(mgr.list().len(), 1);

        mgr.delete("ToDelete").unwrap();
        assert!(mgr.list().is_empty());

        cleanup(&dir);
    }

    #[test]
    fn test_delete_non_existent() {
        let dir = temp_dir("test_delete_non_existent");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();
        let result = mgr.delete("NonExistent");
        assert!(result.is_err(), "deleting non-existent should error");
        cleanup(&dir);
    }

    #[test]
    fn test_exists() {
        let dir = temp_dir("test_exists");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "Exists".to_string(),
            output_type: "alsa".to_string(),
            format: None,
            device: None,
            mixer_type: None,
            mixer_device: None,
            mixer_control: None,
            dop: false,
        };
        mgr.create(&cfg).unwrap();

        assert!(mgr.exists("Exists"));
        assert!(!mgr.exists("NonExistent"));

        cleanup(&dir);
    }

    #[test]
    fn test_parse_round_trip() {
        let dir = temp_dir("test_parse_round_trip");
        let mgr = ConfigFragmentManager::new(dir.clone()).unwrap();

        let cfg = DeviceConfig {
            name: "Round Trip".to_string(),
            output_type: "pulse".to_string(),
            format: Some("48000:24:2".to_string()),
            device: Some("alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()),
            mixer_type: Some("software".to_string()),
            mixer_device: Some("Master".to_string()),
            mixer_control: Some("PCM".to_string()),
            dop: true,
        };
        mgr.create(&cfg).unwrap();

        let list = mgr.list();
        assert_eq!(list.len(), 1);
        let parsed = &list[0];
        assert_eq!(parsed.name, "Round Trip");
        assert_eq!(parsed.output_type, "pulse");
        assert_eq!(parsed.device.as_deref(), Some("alsa_output.pci-0000_00_1f.3.analog-stereo"));
        assert_eq!(parsed.format.as_deref(), Some("48000:24:2"));
        assert_eq!(parsed.mixer_type.as_deref(), Some("software"));
        assert_eq!(parsed.mixer_device.as_deref(), Some("Master"));
        assert_eq!(parsed.mixer_control.as_deref(), Some("PCM"));
        assert!(parsed.dop);

        cleanup(&dir);
    }
}
