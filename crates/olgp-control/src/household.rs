//! Small durable household state used by the public timer/memo routes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const TIMER_KINDS: [&str; 3] = ["timer", "alarm", "reminder"];

/// A timer or reminder record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Timer {
    pub id: String,
    pub kind: String,
    pub due_at_ms: u64,
    pub title: String,
    #[serde(default)]
    pub device_id: Option<String>,
    pub cancelled: bool,
}

/// A memo record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Memo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub device_id: Option<String>,
    pub dismissed: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Store {
    #[serde(default)]
    timers: Vec<Timer>,
    #[serde(default)]
    memos: Vec<Memo>,
    #[serde(default)]
    rings: Vec<String>,
}

/// Durable household store.
#[derive(Debug)]
pub struct Household {
    path: PathBuf,
    store: Store,
}

impl Household {
    /// Load an existing store, or start empty.
    ///
    /// # Errors
    /// Returns an IO or JSON error when an existing file is unreadable.
    pub fn load(path: &Path) -> Result<Self, String> {
        let store = if path.exists() {
            let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| e.to_string())?
        } else {
            Store::default()
        };
        Ok(Self { path: path.to_owned(), store })
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_string_pretty(&self.store).map_err(io::Error::other)?;
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&self.path)?;
            file.write_all(bytes.as_bytes())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            fs::write(&self.path, bytes)
        }
    }

    /// Create a timer.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add_timer(&mut self, timer: Timer) -> Result<(), String> {
        if !TIMER_KINDS.contains(&timer.kind.as_str()) {
            return Err(format!("invalid_kind: {}", timer.kind));
        }
        self.store.timers.push(timer);
        self.persist().map_err(|e| e.to_string())
    }

    /// List timers.
    #[must_use]
    pub fn timers(&self) -> &[Timer] {
        &self.store.timers
    }

    /// Cancel a timer by id.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn cancel_timer(&mut self, id: &str) -> Result<bool, String> {
        let Some(timer) = self.store.timers.iter_mut().find(|t| t.id == id) else {
            return Ok(false);
        };
        timer.cancelled = true;
        self.persist().map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Move a timer's due time forward by `delay_ms`.
    ///
    /// Returns `false` when the id is unknown or the timer is cancelled.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn snooze_timer(&mut self, id: &str, delay_ms: u64) -> Result<bool, String> {
        let Some(timer) = self.store.timers.iter_mut().find(|t| t.id == id && !t.cancelled) else {
            return Ok(false);
        };
        timer.due_at_ms = timer.due_at_ms.saturating_add(delay_ms);
        self.persist().map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Create a memo.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add_memo(&mut self, memo: Memo) -> Result<(), String> {
        self.store.memos.push(memo);
        self.persist().map_err(|e| e.to_string())
    }

    /// List memos.
    #[must_use]
    pub fn memos(&self) -> &[Memo] {
        &self.store.memos
    }

    /// Dismiss a memo by id.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn dismiss_memo(&mut self, id: &str) -> Result<bool, String> {
        let Some(memo) = self.store.memos.iter_mut().find(|m| m.id == id) else {
            return Ok(false);
        };
        memo.dismissed = true;
        self.persist().map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// Ring a device for `duration_sec` seconds and record the ring.
    ///
    /// Returns the persisted record. Duration must be in `1..=60`.
    ///
    /// # Errors
    /// Returns an error for an out-of-range duration or when the store cannot be persisted.
    pub fn ring_phone(&mut self, id: &str, duration_sec: u32) -> Result<Option<String>, String> {
        if !(1..=60).contains(&duration_sec) {
            return Err("ring duration must be between 1 and 60 seconds".into());
        }
        let record = format!("{id}:{duration_sec}");
        self.store.rings.push(record.clone());
        self.persist().map_err(|e| e.to_string())?;
        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_path() -> PathBuf {
        let unique = format!(
            "olgp-household-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        std::env::temp_dir().join(unique)
    }

    fn timer(id: &str) -> Timer {
        Timer {
            id: id.into(),
            kind: "timer".into(),
            due_at_ms: 1_000,
            title: "wake".into(),
            device_id: Some("phone-1".into()),
            cancelled: false,
        }
    }

    #[test]
    fn timer_kind_is_validated() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        let mut bad = timer("t1");
        bad.kind = "unknown".into();
        assert!(store.add_timer(bad).is_err());
        for kind in TIMER_KINDS {
            let mut valid = timer(kind);
            valid.id = kind.to_owned();
            store.add_timer(valid).unwrap();
        }
        assert_eq!(store.timers().len(), TIMER_KINDS.len());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn timer_lifecycle_persists() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        store.add_timer(timer("t1")).unwrap();
        assert_eq!(store.timers().len(), 1);
        assert!(store.cancel_timer("t1").unwrap());
        let reloaded = Household::load(&path).unwrap();
        assert!(reloaded.timers()[0].cancelled);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn snooze_moves_due_time_and_persists() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        store.add_timer(timer("t1")).unwrap();
        assert!(store.snooze_timer("t1", 5_000).unwrap());
        assert_eq!(store.timers()[0].due_at_ms, 6_000);
        assert_eq!(Household::load(&path).unwrap().timers()[0].due_at_ms, 6_000);
        assert!(!store.snooze_timer("missing", 5_000).unwrap());
        assert!(store.cancel_timer("t1").unwrap());
        assert!(!store.snooze_timer("t1", 5_000).unwrap());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn memo_lifecycle_and_device_id_persist() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        store
            .add_memo(Memo {
                id: "m1".into(),
                title: "note".into(),
                device_id: Some("phone-2".into()),
                dismissed: false,
            })
            .unwrap();
        assert_eq!(store.memos()[0].device_id.as_deref(), Some("phone-2"));
        assert!(store.dismiss_memo("m1").unwrap());
        assert!(store.memos()[0].dismissed);
        let reloaded = Household::load(&path).unwrap();
        assert_eq!(reloaded.memos()[0].device_id.as_deref(), Some("phone-2"));
        assert!(reloaded.memos()[0].dismissed);
        assert!(!store.dismiss_memo("missing").unwrap());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_state_loads_with_default_new_fields() {
        let path = temp_path();
        fs::write(
            &path,
            r#"{"timers":[{"id":"t1","kind":"timer","due_at_ms":1,"title":"old","cancelled":false}],"memos":[{"id":"m1","title":"old","dismissed":false}]}"#,
        )
        .unwrap();
        let store = Household::load(&path).unwrap();
        assert_eq!(store.timers()[0].device_id, None);
        assert_eq!(store.memos()[0].device_id, None);
        assert!(store.store.rings.is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn ring_duration_bounds_and_history() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        assert!(store.ring_phone("phone-1", 0).is_err());
        assert!(store.ring_phone("phone-1", 61).is_err());
        assert!(store.store.rings.is_empty());
        assert_eq!(store.ring_phone("phone-1", 1).unwrap().as_deref(), Some("phone-1:1"));
        assert_eq!(store.ring_phone("phone-1", 60).unwrap().as_deref(), Some("phone-1:60"));
        let reloaded = Household::load(&path).unwrap();
        assert_eq!(reloaded.store.rings, vec!["phone-1:1", "phone-1:60"]);
        fs::remove_file(path).unwrap();
    }
}
