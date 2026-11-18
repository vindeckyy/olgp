//! Small durable household state used by the public timer/memo routes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A timer or reminder record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Timer {
    pub id: String,
    pub kind: String,
    pub due_at_ms: u64,
    pub title: String,
    pub cancelled: bool,
}

/// A memo record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Memo {
    pub id: String,
    pub title: String,
    pub dismissed: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Store {
    timers: Vec<Timer>,
    memos: Vec<Memo>,
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
        fs::write(&self.path, serde_json::to_string_pretty(&self.store).unwrap())
    }

    /// Create a timer.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add_timer(&mut self, timer: Timer) -> Result<(), String> {
        self.store.timers.push(timer);
        self.persist().map_err(|e| e.to_string())
    }

    /// List active timers.
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

    #[test]
    fn timer_lifecycle_persists() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        let timer = Timer {
            id: "t1".into(),
            kind: "timer".into(),
            due_at_ms: 123,
            title: "wake".into(),
            cancelled: false,
        };
        store.add_timer(timer).unwrap();
        assert_eq!(store.timers().len(), 1);
        assert!(store.cancel_timer("t1").unwrap());
        let reloaded = Household::load(&path).unwrap();
        assert!(reloaded.timers()[0].cancelled);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn memo_lifecycle_persists() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let mut store = Household::load(&path).unwrap();
        store.add_memo(Memo { id: "m1".into(), title: "note".into(), dismissed: false }).unwrap();
        assert!(store.dismiss_memo("m1").unwrap());
        assert!(store.memos()[0].dismissed);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_ids_return_false() {
        let path = temp_path();
        let mut store = Household::load(&path).unwrap();
        assert!(!store.cancel_timer("missing").unwrap());
        assert!(!store.dismiss_memo("missing").unwrap());
    }
}
