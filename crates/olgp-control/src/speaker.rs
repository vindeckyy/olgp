//! Persisted first-party speaker state.

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use serde::{Deserialize, Serialize};

/// Bluetooth state of a speaker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Bluetooth {
    pub paired: bool,
    pub connecting: bool,
    pub stream_state: String,
}

/// A first-party speaker and its current playback task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Speaker {
    pub device_id: String,
    pub name: String,
    pub online: bool,
    pub volume: u8,
    pub active_task: Option<String>,
    pub capabilities: Vec<String>,
    pub bluetooth: Bluetooth,
}

#[derive(Deserialize, Serialize)]
struct Persisted {
    speakers: Vec<Speaker>,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            speakers: vec![Speaker {
                device_id: "wici-speaker-001".into(),
                name: "OLGP Speaker".into(),
                online: true,
                volume: 50,
                active_task: None,
                capabilities: vec![
                    "play".into(),
                    "stop".into(),
                    "volume".into(),
                    "bluetooth".into(),
                ],
                bluetooth: Bluetooth {
                    paired: false,
                    connecting: false,
                    stream_state: "idle".into(),
                },
            }],
        }
    }
}

/// Speaker registry backed by a JSON file.
#[derive(Debug)]
pub struct SpeakerStore {
    path: PathBuf,
    speakers: Vec<Speaker>,
}

impl SpeakerStore {
    /// Load speaker state, initializing the first-party speaker when absent.
    ///
    /// # Errors
    /// Returns a description when the file cannot be read or parsed, or when
    /// the initialized state cannot be persisted.
    pub fn load(path: &Path) -> Result<Self, String> {
        let persisted = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("speaker state parse: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Persisted::default(),
            Err(e) => return Err(format!("speaker state read: {e}")),
        };
        let store = Self {
            path: path.to_path_buf(),
            speakers: persisted.speakers,
        };
        store.persist()?;
        Ok(store)
    }

    /// All known speakers.
    #[must_use]
    pub fn speakers(&self) -> &[Speaker] {
        &self.speakers
    }

    /// Look up a speaker by device ID.
    #[must_use]
    pub fn speaker(&self, id: &str) -> Option<&Speaker> {
        self.speakers.iter().find(|speaker| speaker.device_id == id)
    }

    /// Set a speaker's volume.
    ///
    /// # Errors
    /// Returns a description when the speaker is unknown, the volume is above
    /// 100, or the updated state cannot be persisted.
    pub fn update_volume(&mut self, id: &str, volume: u8) -> Result<Speaker, String> {
        if volume > 100 {
            return Err("volume must be 0..=100".into());
        }
        let updated = {
            let speaker = self.speaker_mut(id)?;
            speaker.volume = volume;
            speaker.clone()
        };
        self.persist()?;
        Ok(updated)
    }

    /// Start playback. The URL is the deterministic task token.
    ///
    /// # Errors
    /// Returns a description when the speaker is unknown or the updated state
    /// cannot be persisted.
    pub fn play(&mut self, id: &str, url: &str) -> Result<Speaker, String> {
        let updated = {
            let speaker = self.speaker_mut(id)?;
            speaker.active_task = Some(url.to_owned());
            speaker.clone()
        };
        self.persist()?;
        Ok(updated)
    }

    /// Stop playback and clear the active task.
    ///
    /// # Errors
    /// Returns a description when the speaker is unknown or the updated state
    /// cannot be persisted.
    pub fn stop(&mut self, id: &str) -> Result<Speaker, String> {
        let updated = {
            let speaker = self.speaker_mut(id)?;
            speaker.active_task = None;
            speaker.clone()
        };
        self.persist()?;
        Ok(updated)
    }

    /// Read a speaker's Bluetooth state.
    ///
    /// # Errors
    /// Returns a description when the speaker is unknown.
    pub fn bluetooth(&self, id: &str) -> Result<Bluetooth, String> {
        Ok(self.speaker(id).ok_or_else(|| unknown_speaker(id))?.bluetooth.clone())
    }

    /// Begin Bluetooth pairing for a speaker.
    ///
    /// # Errors
    /// Returns a description when the speaker is unknown or the updated state
    /// cannot be persisted.
    pub fn start_bluetooth_pairing(&mut self, id: &str) -> Result<Bluetooth, String> {
        let updated = {
            let speaker = self.speaker_mut(id)?;
            speaker.bluetooth.connecting = true;
            speaker.bluetooth.clone()
        };
        self.persist()?;
        Ok(updated)
    }

    fn speaker_mut(&mut self, id: &str) -> Result<&mut Speaker, String> {
        self.speakers
            .iter_mut()
            .find(|speaker| speaker.device_id == id)
            .ok_or_else(|| unknown_speaker(id))
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| format!("speaker state dir: {e}"))?;
            }
        }
        let state = Persisted {
            speakers: self.speakers.clone(),
        };
        let text = serde_json::to_string_pretty(&state).map_err(|e| format!("speaker state encode: {e}"))?;
        fs::write(&self.path, text).map_err(|e| format!("speaker state write: {e}"))?;
        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("speaker state permissions: {e}"))?;
        Ok(())
    }
}

fn unknown_speaker(id: &str) -> String {
    format!("unknown speaker: {id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "wici-speaker-001";

    fn test_path(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("olgp-speaker-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root.join("speakers.json")
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn load_initializes_and_persists_default_speaker() {
        let path = test_path("default");
        let store = SpeakerStore::load(&path).unwrap();
        assert_eq!(store.speakers().len(), 1);
        assert_eq!(store.speaker(ID).unwrap().name, "OLGP Speaker");
        assert_eq!(store.speaker(ID).unwrap().active_task, None);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(ID));
        let reloaded = SpeakerStore::load(&path).unwrap();
        assert_eq!(reloaded.speakers(), store.speakers());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        cleanup(&path);
    }

    #[test]
    fn volume_is_validated_and_persisted() {
        let path = test_path("volume");
        let mut store = SpeakerStore::load(&path).unwrap();
        assert_eq!(store.update_volume(ID, 100).unwrap().volume, 100);
        assert_eq!(SpeakerStore::load(&path).unwrap().speaker(ID).unwrap().volume, 100);
        assert!(store.update_volume(ID, 101).is_err());
        assert_eq!(store.speaker(ID).unwrap().volume, 100);
        assert_eq!(store.update_volume(ID, 0).unwrap().volume, 0);
        cleanup(&path);
    }

    #[test]
    fn play_and_stop_persist_task_token() {
        let path = test_path("task");
        let mut store = SpeakerStore::load(&path).unwrap();
        let url = "https://example.test/audio.mp3";
        assert_eq!(store.play(ID, url).unwrap().active_task.as_deref(), Some(url));
        assert_eq!(
            SpeakerStore::load(&path).unwrap().speaker(ID).unwrap().active_task.as_deref(),
            Some(url)
        );
        assert_eq!(store.stop(ID).unwrap().active_task, None);
        assert_eq!(SpeakerStore::load(&path).unwrap().speaker(ID).unwrap().active_task, None);
        cleanup(&path);
    }

    #[test]
    fn bluetooth_pairing_persists_connecting() {
        let path = test_path("bluetooth");
        let mut store = SpeakerStore::load(&path).unwrap();
        let before = store.bluetooth(ID).unwrap();
        assert!(!before.paired);
        assert!(!before.connecting);
        let after = store.start_bluetooth_pairing(ID).unwrap();
        assert!(after.connecting);
        assert!(!after.paired);
        assert!(SpeakerStore::load(&path).unwrap().speaker(ID).unwrap().bluetooth.connecting);
        cleanup(&path);
    }

    #[test]
    fn unknown_speaker_is_rejected() {
        let path = test_path("unknown");
        let mut store = SpeakerStore::load(&path).unwrap();
        assert!(store.speaker("missing").is_none());
        assert!(store.update_volume("missing", 50).is_err());
        assert!(store.play("missing", "https://example.test/a.mp3").is_err());
        assert!(store.stop("missing").is_err());
        assert!(store.bluetooth("missing").is_err());
        assert!(store.start_bluetooth_pairing("missing").is_err());
        cleanup(&path);
    }
}
