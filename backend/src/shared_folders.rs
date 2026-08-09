use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const SHARE_FILE_NAME: &str = "smb-shares.conf";

/// Write the Samba shares managed by the library-source UI.
///
/// The installer includes this file from smb.conf. Keeping the fragment under
/// data_dir lets the unprivileged oxide service update shares without editing
/// the system-owned Samba configuration.
pub fn sync(data_dir: &Path, folders: &[PathBuf]) -> Result<()> {
    let path = data_dir.join(SHARE_FILE_NAME);
    let text = render(folders)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating shared-folder directory {}", parent.display()))?;
    }
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, text)
        .with_context(|| format!("writing shared-folder config {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| {
        format!(
            "renaming shared-folder config {} -> {}",
            tmp.display(),
            path.display()
        )
    })?;

    reload_samba();
    Ok(())
}

fn render(folders: &[PathBuf]) -> Result<String> {
    let mut names = Vec::with_capacity(folders.len());
    let mut text = String::from(
        "# Managed by oxide-player. Changes are replaced when library sources change.\n\n",
    );

    for folder in folders {
        let path = folder.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "shared folder path is not valid UTF-8: {}",
                folder.display()
            )
        })?;
        if path.contains(['\n', '\r']) {
            anyhow::bail!("shared folder path contains a newline: {}", folder.display());
        }

        let base = folder
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Library");
        let stem = sanitize_name(base);
        let mut name = if stem.eq_ignore_ascii_case("music") {
            "Music".to_string()
        } else {
            format!("Oxide-{stem}")
        };
        if names.iter().any(|existing| existing == &name) {
            name = format!("{name}-{:08x}", stable_hash(path));
        }
        names.push(name.clone());

        text.push_str(&format!(
            "[{name}]\npath = \"{}\"\nbrowseable = yes\nread only = no\nguest ok = yes\nforce user = oxide\nforce group = oxide\ncreate mask = 0644\ndirectory mask = 0755\n\n",
            samba_quote(path)
        ));
    }

    Ok(text)
}

fn sanitize_name(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "Library".to_string()
    } else {
        result
    }
}

fn samba_quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn stable_hash(value: &str) -> u32 {
    value.bytes().fold(0x811c9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    })
}

fn reload_samba() {
    match std::process::Command::new("smbcontrol")
        .args(["smbd", "reload-config"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::debug!("smbcontrol reload-config exited with {status}"),
        Err(error) => tracing::debug!("Samba reload unavailable: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{render, sanitize_name};
    use std::path::PathBuf;

    #[test]
    fn renders_one_share_per_folder() {
        let text = render(&[
            PathBuf::from("/mnt/music"),
            PathBuf::from("/srv/jazz collection"),
        ])
        .unwrap();

        assert!(text.contains("[Music]"));
        assert!(text.contains("path = \"/mnt/music\""));
        assert!(text.contains("[Oxide-jazz_collection]"));
        assert!(text.contains("path = \"/srv/jazz collection\""));
    }
    #[test]
    fn sync_removes_deleted_folder_shares() {
        let data_dir = std::env::temp_dir().join(format!(
            "oxide-shared-folders-{}",
            std::process::id()
        ));
        let first = PathBuf::from("/mnt/first");
        let second = PathBuf::from("/mnt/second");

        super::sync(&data_dir, &[first]).unwrap();
        let share_file = data_dir.join(super::SHARE_FILE_NAME);
        let initial = std::fs::read_to_string(&share_file).unwrap();
        assert!(initial.contains("/mnt/first"));

        super::sync(&data_dir, &[second]).unwrap();
        let updated = std::fs::read_to_string(&share_file).unwrap();
        assert!(!updated.contains("/mnt/first"));
        assert!(updated.contains("/mnt/second"));

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn duplicate_folder_names_get_distinct_share_names() {
        let text = render(&[
            PathBuf::from("/mnt/one/music"),
            PathBuf::from("/mnt/two/music"),
        ])
        .unwrap();

        assert_eq!(text.matches("[Music]").count(), 1);
        assert!(text.contains("[Music-"));
    }

    #[test]
    fn share_names_are_safe_for_samba_sections() {
        assert_eq!(sanitize_name("jazz collection/2026"), "jazz_collection_2026");
        assert_eq!(sanitize_name(""), "Library");
    }
}
