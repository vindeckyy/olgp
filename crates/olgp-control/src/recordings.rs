//! Durable audio recordings. No transcript is synthesized here.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

/// Recording metadata. `segments` stays empty until a real ASR provider fills it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Recording {
    pub id: String,
    pub state: String,
    pub retain_audio: bool,
    pub segments: Vec<String>,
    pub chunk_count: u32,
}

/// Recording metadata and PCM chunks stored below a caller-supplied root.
pub struct RecordingStore {
    root: PathBuf,
    recordings: Vec<Recording>,
    next_id: u64,
}

impl RecordingStore {
    /// Open or initialize a store under `root`.
    ///
    /// # Errors
    /// Returns a description when the root cannot be created or the index cannot be read or written.
    pub fn load(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| format!("create recordings root: {e}"))?;
        let index = root.join("recordings.json");
        let recordings: Vec<Recording> = if index.exists() {
            let text = fs::read_to_string(&index).map_err(|e| format!("read recordings index: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("parse recordings index: {e}"))?
        } else {
            Vec::new()
        };
        let mut next_id = 1;
        for recording in &recordings {
            if let Some(suffix) = recording.id.strip_prefix("rec-") {
                if let Ok(value) = u64::from_str_radix(suffix, 16) {
                    next_id = next_id.max(value.saturating_add(1));
                }
            }
        }
        let store = Self {
            root: root.to_path_buf(),
            recordings,
            next_id,
        };
        store.persist()?;
        Ok(store)
    }

    /// Start a recording with no segments.
    ///
    /// # Errors
    /// Returns a description when the chunk directory or index cannot be written.
    pub fn create(&mut self, retain_audio: bool) -> Result<Recording, String> {
        let recording = Recording {
            id: format!("rec-{:08x}", self.next_id),
            state: "recording".to_owned(),
            retain_audio,
            segments: Vec::new(),
            chunk_count: 0,
        };
        self.next_id += 1;
        fs::create_dir_all(self.chunk_dir(&recording.id)?)
            .map_err(|e| format!("create recording directory: {e}"))?;
        self.recordings.push(recording.clone());
        if let Err(e) = self.persist() {
            self.recordings.pop();
            return Err(e);
        }
        Ok(recording)
    }

    /// All recordings in creation order.
    #[must_use]
    pub fn list(&self) -> &[Recording] {
        &self.recordings
    }

    /// Look up a recording by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Recording> {
        self.recordings.iter().find(|recording| recording.id == id)
    }

    /// Remove a recording and its chunks. Returns `false` when the id is unknown.
    ///
    /// # Errors
    /// Returns a description when the chunk directory or index cannot be updated.
    pub fn delete(&mut self, id: &str) -> Result<bool, String> {
        let Some(index) = self.recordings.iter().position(|recording| recording.id == id) else {
            return Ok(false);
        };
        let dir = self.chunk_dir(id)?;
        match fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove recording chunks: {e}")),
        }
        let removed = self.recordings.remove(index);
        if let Err(e) = self.persist() {
            self.recordings.insert(index, removed);
            return Err(e);
        }
        Ok(true)
    }

    /// Write the next ordered PCM chunk to `chunks/<id>/<seq>.pcm`.
    ///
    /// # Errors
    /// Returns a description for an unknown id, an inactive recording, an out-of-order
    /// sequence number, or a failed chunk or index write.
    pub fn append_audio(&mut self, id: &str, seq: u32, bytes: &[u8]) -> Result<(), String> {
        let index = self
            .recordings
            .iter()
            .position(|recording| recording.id == id)
            .ok_or_else(|| format!("unknown recording {id}"))?;
        let recording = &self.recordings[index];
        if recording.state != "recording" {
            return Err("recording not active".to_owned());
        }
        if seq != recording.chunk_count {
            return Err("out of order chunk".to_owned());
        }
        let dir = self.chunk_dir(&recording.id)?;
        fs::create_dir_all(&dir).map_err(|e| format!("create chunk directory: {e}"))?;
        write_private(&dir.join(format!("{seq}.pcm")), bytes)?;
        self.recordings[index].chunk_count += 1;
        if let Err(e) = self.persist() {
            self.recordings[index].chunk_count -= 1;
            return Err(e);
        }
        Ok(())
    }

    /// Mark a recording finished. ASR is unavailable, so this always reports that error.
    ///
    /// # Errors
    /// Returns `asr_unavailable` for an existing id, `unknown recording` for a missing id,
    /// or a persistence error when the state cannot be saved.
    pub fn finish(&mut self, id: &str) -> Result<Recording, String> {
        let index = self
            .recordings
            .iter()
            .position(|recording| recording.id == id)
            .ok_or_else(|| format!("unknown recording {id}"))?;
        self.recordings[index].state = "transcription_unavailable".to_owned();
        self.persist()?;
        Err("asr_unavailable".to_owned())
    }

    /// Retry transcription for retained audio. ASR is still unavailable.
    ///
    /// # Errors
    /// Returns `audio_not_retained` when the audio was discarded, `asr_unavailable` when
    /// reprocessing is possible, or a persistence error when the state cannot be saved.
    pub fn reprocess(&mut self, id: &str) -> Result<Recording, String> {
        let index = self
            .recordings
            .iter()
            .position(|recording| recording.id == id)
            .ok_or_else(|| format!("unknown recording {id}"))?;
        if !self.recordings[index].retain_audio {
            return Err("audio_not_retained".to_owned());
        }
        self.recordings[index].state = "transcription_unavailable".to_owned();
        self.persist()?;
        Err("asr_unavailable".to_owned())
    }

    fn chunk_dir(&self, id: &str) -> Result<PathBuf, String> {
        let suffix = id
            .strip_prefix("rec-")
            .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| "invalid recording id".to_owned())?;
        u64::from_str_radix(suffix, 16).map_err(|_| "invalid recording id".to_owned())?;
        Ok(self.root.join("chunks").join(id))
    }

    fn persist(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&self.recordings)
            .map_err(|e| format!("encode recordings index: {e}"))?;
        write_private(&self.root.join("recordings.json"), &json)
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "missing file parent".to_owned())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid file name".to_owned())?;
    let tmp = parent.join(format!(".{name}.tmp"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&tmp)
        .map_err(|e| format!("create {}: {e}", tmp.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    file.sync_all()
        .map_err(|e| format!("sync {}: {e}", tmp.display()))?;
    drop(file);
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("set permissions on {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("replace {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "olgp-recordings-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn ordered_chunks_survive_reload() {
        let root = temp_root();
        let mut store = RecordingStore::load(&root).unwrap();
        let recording = store.create(false).unwrap();

        store.append_audio(&recording.id, 0, b"a").unwrap();
        store.append_audio(&recording.id, 1, b"bc").unwrap();
        assert!(store.append_audio(&recording.id, 3, b"x").is_err());
        assert_eq!(store.get(&recording.id).unwrap().chunk_count, 2);

        let mut reloaded = RecordingStore::load(&root).unwrap();
        assert_eq!(reloaded.get(&recording.id).unwrap().chunk_count, 2);
        let next = reloaded.create(true).unwrap();
        assert_ne!(next.id, recording.id);
        let dir = root.join("chunks").join(&recording.id);
        assert_eq!(fs::read(dir.join("0.pcm")).unwrap(), b"a".to_vec());
        assert_eq!(fs::read(dir.join("1.pcm")).unwrap(), b"bc".to_vec());
        assert!(!dir.join("3.pcm").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_removes_chunks_and_metadata() {
        let root = temp_root();
        let mut store = RecordingStore::load(&root).unwrap();
        let removed = store.create(false).unwrap();
        let kept = store.create(false).unwrap();
        store.append_audio(&removed.id, 0, b"p").unwrap();

        assert!(store.delete(&removed.id).unwrap());
        assert!(!root.join("chunks").join(&removed.id).exists());
        assert!(store.get(&removed.id).is_none());
        assert!(!store.delete(&removed.id).unwrap());

        let reloaded = RecordingStore::load(&root).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert!(reloaded.get(&kept.id).is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_reports_missing_asr_without_transcript() {
        let root = temp_root();
        let mut store = RecordingStore::load(&root).unwrap();
        let recording = store.create(false).unwrap();

        assert_eq!(store.finish(&recording.id).unwrap_err(), "asr_unavailable");
        let saved = store.get(&recording.id).unwrap();
        assert_eq!(saved.state, "transcription_unavailable");
        assert!(saved.segments.is_empty());
        assert!(store.append_audio(&recording.id, 0, b"x").is_err());

        let reloaded = RecordingStore::load(&root).unwrap();
        assert_eq!(reloaded.get(&recording.id).unwrap().state, "transcription_unavailable");
        assert!(reloaded.get(&recording.id).unwrap().segments.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reprocess_requires_retained_audio() {
        let root = temp_root();
        let mut store = RecordingStore::load(&root).unwrap();
        let discarded = store.create(false).unwrap();
        assert_eq!(store.reprocess(&discarded.id).unwrap_err(), "audio_not_retained");
        assert_eq!(store.get(&discarded.id).unwrap().state, "recording");

        let retained = store.create(true).unwrap();
        assert_eq!(store.reprocess(&retained.id).unwrap_err(), "asr_unavailable");
        let saved = store.get(&retained.id).unwrap();
        assert_eq!(saved.state, "transcription_unavailable");
        assert!(saved.segments.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
