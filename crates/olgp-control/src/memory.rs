//! Opt-in transcript memory store used by the audio-memory API.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Retention configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Settings {
    pub mode: String,
    pub retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { mode: "disabled".into(), retention_days: 30 }
    }
}

/// A transcript episode.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Episode {
    pub id: String,
    pub text: String,
}

/// A long-term explicit note.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Note {
    pub id: String,
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Store {
    settings: Settings,
    episodes: Vec<Episode>,
    notes: Vec<Note>,
}

/// Audio-memory state.
#[derive(Debug)]
pub struct AudioMemory {
    path: PathBuf,
    store: Store,
}

impl AudioMemory {
    /// Load an existing store or start disabled and empty.
    ///
    /// # Errors
    /// Returns an error when an existing store is unreadable.
    pub fn load(path: &Path) -> Result<Self, String> {
        let store = if path.exists() {
            let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|_| "bad audio-memory store".to_owned())?
        } else {
            Store { settings: Settings::default(), episodes: Vec::new(), notes: Vec::new() }
        };
        Ok(Self { path: path.to_owned(), store })
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.store).unwrap())
    }

    /// Current settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.store.settings
    }

    /// Structured readiness for the memory subsystem.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.path.parent().is_some_and(|parent| parent.exists())
    }

    /// Update settings.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn set_settings(&mut self, settings: Settings) -> Result<(), String> {
        self.store.settings = settings;
        self.persist().map_err(|e| e.to_string())
    }

    /// Add an episode when memory mode is active.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add_episode(&mut self, id: String, text: String) -> Result<bool, String> {
        if self.store.settings.mode != "active" {
            return Ok(false);
        }
        self.store.episodes.push(Episode { id, text });
        self.persist().map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// List episodes.
    #[must_use]
    pub fn episodes(&self) -> &[Episode] {
        &self.store.episodes
    }

    /// Add an explicit note.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add_note(&mut self, id: String, text: String, kind: String) -> Result<(), String> {
        self.store.notes.push(Note { id, text, kind });
        self.persist().map_err(|e| e.to_string())
    }

    /// List notes.
    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.store.notes
    }

    /// Query notes by substring.
    #[must_use]
    pub fn query(&self, needle: &str) -> Vec<Note> {
        self.store
            .notes
            .iter()
            .filter(|note| note.text.contains(needle))
            .cloned()
            .collect()
    }

    /// Forget one note by id.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn forget(&mut self, id: &str) -> Result<bool, String> {
        let before = self.store.notes.len();
        self.store.notes.retain(|note| note.id != id);
        let removed = self.store.notes.len() != before;
        if removed {
            self.persist().map_err(|e| e.to_string())?;
        }
        Ok(removed)
    }

    /// Clear notes and episodes.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn clear(&mut self) -> Result<(), String> {
        self.store.notes.clear();
        self.store.episodes.clear();
        self.persist().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "olgp-memory-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn disabled_mode_rejects_episodes() {
        let path = temp_path();
        let mut memory = AudioMemory::load(&path).unwrap();
        assert!(!memory.add_episode("e1".into(), "text".into()).unwrap());
        assert!(memory.episodes().is_empty());
    }

    #[test]
    fn active_mode_stores_episodes_and_notes() {
        let path = temp_path();
        let mut memory = AudioMemory::load(&path).unwrap();
        memory
            .set_settings(Settings { mode: "active".into(), retention_days: 7 })
            .unwrap();
        assert!(memory.add_episode("e1".into(), "remember this".into()).unwrap());
        memory.add_note("n1".into(), "remember this".into(), "fact".into()).unwrap();
        assert_eq!(memory.episodes().len(), 1);
        assert_eq!(memory.query("remember").len(), 1);
        let reloaded = AudioMemory::load(&path).unwrap();
        assert_eq!(reloaded.settings().retention_days, 7);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn forget_and_clear_mutate_store() {
        let path = temp_path();
        let mut memory = AudioMemory::load(&path).unwrap();
        memory.add_note("n1".into(), "alpha".into(), "fact".into()).unwrap();
        assert!(memory.forget("n1").unwrap());
        assert!(!memory.forget("n1").unwrap());
        memory.add_note("n2".into(), "beta".into(), "fact".into()).unwrap();
        memory.clear().unwrap();
        assert!(memory.notes().is_empty());
        fs::remove_file(path).unwrap();
    }
}
