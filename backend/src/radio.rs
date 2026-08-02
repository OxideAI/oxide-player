//! User-managed internet radio stations, persisted to
//! `<data_dir>/radio_stations.json`.
//!
//! Mirrors the `VizParams` persistence pattern: best-effort load at startup,
//! atomic temp+rename writes. A missing file is seeded with the shipped
//! stations; once the file exists it is the single source of truth, so a user
//! who deletes a seed keeps it gone.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

const STATIONS_FILE: &str = "radio_stations.json";

/// The stations shipped on a fresh install.
fn seed_stations() -> Vec<RadioStation> {
    vec![
        RadioStation {
            id: uuid::Uuid::new_v4().to_string(),
            name: "JFK Ibiza".to_string(),
            url: "https://stream.aiir.com/7dsjltmny8cvv".to_string(),
            homepage: Some("https://jfkibiza.es/".to_string()),
            artwork: None,
        },
        RadioStation {
            id: uuid::Uuid::new_v4().to_string(),
            name: "100% ACID JAZZ".to_string(),
            url: "https://mpc1.mediacp.eu:8356/stream".to_string(),
            homepage: Some("https://www.internet-radio.com/station/100acidjazz/".to_string()),
            artwork: None,
        },
        RadioStation {
            id: uuid::Uuid::new_v4().to_string(),
            name: "The Loft".to_string(),
            url: "https://usa17.fastcast4u.com/proxy/fbpxpddt?mp=/1".to_string(),
            homepage: Some("https://www.jazzandvocalloft.org/".to_string()),
            artwork: None,
        },
    ]
}

/// A single internet radio station.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    pub url: String,
    pub homepage: Option<String>,
    /// Optional remote artwork shown for the station in the radio view.
    #[serde(default)]
    pub artwork: Option<String>,
}

fn normalize_artwork(artwork: Option<String>) -> AppResult<Option<String>> {
    let artwork = artwork
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = &artwork {
        if !(value.starts_with("http://") || value.starts_with("https://")) {
            return Err(AppError::BadRequest(
                "station artwork must start with http:// or https://".into(),
            ));
        }
    }
    Ok(artwork)
}

/// Thread-safe store of user radio stations. Every mutation persists to disk
/// synchronously so a crash never loses a station.
pub struct RadioManager {
    path: PathBuf,
    stations: RwLock<Vec<RadioStation>>,
}

impl RadioManager {
    /// Load stations from `<data_dir>/radio_stations.json`. Missing or
    /// unparseable files fall back to the shipped stations, which are persisted
    /// so they exist on disk from first run on.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(STATIONS_FILE);
        let (stations, seeded) = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(stations) => (stations, false),
                Err(e) => {
                    tracing::warn!("radio_stations.json unparseable, reseeding: {e}");
                    (seed_stations(), true)
                }
            },
            Err(_) => (seed_stations(), true),
        };
        let manager = RadioManager {
            path,
            stations: RwLock::new(stations),
        };
        if seeded {
            if let Err(e) = manager.save() {
                tracing::warn!("failed to persist radio station seeds: {e}");
            }
        }
        manager
    }

    pub fn list(&self) -> Vec<RadioStation> {
        self.stations.read().expect("radio lock poisoned").clone()
    }

    pub fn get(&self, id: &str) -> Option<RadioStation> {
        self.stations
            .read()
            .expect("radio lock poisoned")
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// Exact URL match (the queue stores the URL verbatim, so MPD-reported
    /// URIs compare equal).
    pub fn by_url(&self, url: &str) -> Option<RadioStation> {
        self.stations
            .read()
            .expect("radio lock poisoned")
            .iter()
            .find(|s| s.url == url)
            .cloned()
    }

    /// Add a station. Trims inputs; rejects empty names, non-http(s) URLs, and
    /// duplicate URLs. Artwork is an optional http(s) image URL.

    pub fn add(
        &self,
        name: &str,
        url: &str,
        homepage: Option<String>,
        artwork: Option<String>,
    ) -> AppResult<RadioStation> {
        let name = name.trim().to_string();
        let url = url.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest("station name is empty".into()));
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(AppError::BadRequest(
                "station url must start with http:// or https://".into(),
            ));
        }
        let artwork = normalize_artwork(artwork)?;
        {
            let stations = self.stations.read().expect("radio lock poisoned");
            if stations.iter().any(|s| s.url == url) {
                return Err(AppError::BadRequest("station url already exists".into()));
            }
        }
        let station = RadioStation {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            url,
            homepage: homepage
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty()),
            artwork,
        };
        self.stations
            .write()
            .expect("radio lock poisoned")
            .push(station.clone());
        self.save().map_err(|e| AppError::Library(e.to_string()))?;
        Ok(station)
    }

    /// Update a station's display name and artwork without changing its stream.
    pub fn update(
        &self,
        id: &str,
        name: &str,
        artwork: Option<String>,
    ) -> AppResult<RadioStation> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::BadRequest("station name is empty".into()));
        }
        let artwork = normalize_artwork(artwork)?;
        let updated = {
            let mut stations = self.stations.write().expect("radio lock poisoned");
            let station = stations
                .iter_mut()
                .find(|station| station.id == id)
                .ok_or_else(|| AppError::NotFound(format!("radio station {id}")))?;
            station.name = name;
            station.artwork = artwork;
            station.clone()
        };
        self.save().map_err(|e| AppError::Library(e.to_string()))?;
        Ok(updated)
    }

    /// Remove a station by id. NotFound when the id is unknown.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        let removed = {
            let mut stations = self.stations.write().expect("radio lock poisoned");
            let before = stations.len();
            stations.retain(|s| s.id != id);
            stations.len() != before
        };
        if !removed {
            return Err(AppError::NotFound(format!("radio station {id}")));
        }
        self.save().map_err(|e| AppError::Library(e.to_string()))?;
        Ok(())
    }

    /// Atomic temp+rename write, same as `VizParams::save`.
    fn save(&self) -> Result<(), anyhow::Error> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| anyhow::anyhow!("create data dir: {e}"))?;
        }
        let text = serde_json::to_string_pretty(&*self.stations.read().expect("radio lock poisoned"))
            .map_err(|e| anyhow::anyhow!("serialize radio stations: {e}"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| anyhow::anyhow!("write radio stations: {e}"))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| anyhow::anyhow!("rename radio stations: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn missing_file_seeds_shipped_stations_and_persists() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());

        let stations = manager.list();
        assert_eq!(stations.len(), 3);
        let jfk = stations.iter().find(|s| s.name == "JFK Ibiza").unwrap();
        assert_eq!(jfk.url, "https://stream.aiir.com/7dsjltmny8cvv");
        assert_eq!(jfk.homepage.as_deref(), Some("https://jfkibiza.es/"));
        let acid_jazz = stations.iter().find(|s| s.name == "100% ACID JAZZ").unwrap();
        assert_eq!(acid_jazz.url, "https://mpc1.mediacp.eu:8356/stream");
        assert_eq!(
            acid_jazz.homepage.as_deref(),
            Some("https://www.internet-radio.com/station/100acidjazz/")
        );
        let loft = stations.iter().find(|s| s.name == "The Loft").unwrap();
        assert_eq!(
            loft.url,
            "https://usa17.fastcast4u.com/proxy/fbpxpddt?mp=/1"
        );
        assert_eq!(
            loft.homepage.as_deref(),
            Some("https://www.jazzandvocalloft.org/")
        );

        // Seeds must be on disk so a reload (restart) reads the same stations.
        let reloaded = RadioManager::load(dir.path());
        assert_eq!(reloaded.list(), stations);
    }

    #[test]
    fn legacy_station_file_defaults_artwork_to_none() {
        let dir = temp_dir();
        std::fs::write(
            dir.path().join(STATIONS_FILE),
            r#"[{"id":"legacy","name":"Legacy FM","url":"https://example.com/stream","homepage":null}]"#,
        )
        .unwrap();

        let station = RadioManager::load(dir.path()).list().pop().unwrap();
        assert_eq!(station.id, "legacy");
        assert_eq!(station.artwork, None);
    }

    #[test]
    fn add_roundtrips_through_disk() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());

        let added = manager
            .add("My Station", " https://example.com/stream.mp3 ", None, None)
            .expect("add");
        assert_eq!(added.name, "My Station");
        assert_eq!(added.url, "https://example.com/stream.mp3");
        assert_eq!(added.homepage, None);
        assert_eq!(added.artwork, None);
        assert!(!added.id.is_empty());
        let reloaded = RadioManager::load(dir.path());
        assert_eq!(reloaded.list().len(), 4);
        assert!(reloaded.get(&added.id).is_some());
    }

    #[test]
    fn update_name_and_artwork_roundtrip() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());
        let original = manager.list().into_iter().next().unwrap();

        let updated = manager
            .update(
                &original.id,
                "  Late Night FM  ",
                Some(" https://example.com/art.jpg ".to_string()),
            )
            .expect("update");
        assert_eq!(updated.name, "Late Night FM");
        assert_eq!(updated.url, original.url);
        assert_eq!(updated.artwork.as_deref(), Some("https://example.com/art.jpg"));

        let reloaded = RadioManager::load(dir.path());
        assert_eq!(reloaded.get(&original.id), Some(updated));
    }

    #[test]
    fn update_rejects_empty_name_and_bad_artwork() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());
        let id = manager.list().into_iter().next().unwrap().id;

        assert!(matches!(
            manager.update(&id, " ", None),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            manager.update(&id, "Station", Some("ftp://example.com/art.jpg".to_string())),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn add_rejects_bad_inputs() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());

        let empty_name = manager.add("   ", "https://x.example/", None, None);
        assert!(matches!(empty_name, Err(AppError::BadRequest(_))));

        let bad_scheme = manager.add("X", "ftp://x.example/", None, None);
        assert!(matches!(bad_scheme, Err(AppError::BadRequest(_))));

        let missing_scheme = manager.add("X", "example.com/stream", None, None);
        assert!(matches!(missing_scheme, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn add_rejects_duplicate_url() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());
        manager
            .add("A", "https://example.com/stream.mp3", None, None)
            .unwrap();
        let dup = manager.add("B", "https://example.com/stream.mp3", None, None);
        assert!(matches!(dup, Err(AppError::BadRequest(_))));
        assert_eq!(manager.list().len(), 4, "three seeds + one addition");
    }

    #[test]
    fn remove_unknown_id_is_not_found() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());
        assert!(matches!(
            manager.remove("nope"),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn remove_persists_and_by_url_tracks_changes() {
        let dir = temp_dir();
        let manager = RadioManager::load(dir.path());
        let seeds = manager.list();

        for seed in &seeds {
            assert!(manager.by_url(&seed.url).is_some());
            manager.remove(&seed.id).expect("remove seed");
        }
        assert!(manager.list().is_empty());

        let reloaded = RadioManager::load(dir.path());
        assert!(reloaded.list().is_empty(), "deleted seeds stay deleted");
    }

    #[test]
    fn unparseable_file_reseeds() {
        let dir = temp_dir();
        std::fs::write(dir.path().join(STATIONS_FILE), "not json").unwrap();
        let manager = RadioManager::load(dir.path());
        assert_eq!(manager.list().len(), 3);
        assert!(manager.list().iter().any(|s| s.name == "JFK Ibiza"));
        assert!(manager.list().iter().any(|s| s.name == "100% ACID JAZZ"));
        assert!(manager.list().iter().any(|s| s.name == "The Loft"));
    }
}
