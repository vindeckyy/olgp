//! Trusted SDK credentials issued through the pairing window.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pairing::Credential;

/// One persisted client credential.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrustedClient {
    pub token: String,
    pub client_public: String,
    pub paired_at: String,
}

/// Trusted client store loaded by the gateway.
#[derive(Debug)]
pub struct TrustedClients {
    path: PathBuf,
    clients: Vec<TrustedClient>,
}

impl TrustedClients {
    /// Open the store, starting empty when the file does not exist.
    ///
    /// # Errors
    /// Returns a description when an existing store is unreadable.
    pub fn load(path: &Path) -> Result<Self, String> {
        let clients = if path.exists() {
            let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        Ok(Self { path: path.to_owned(), clients })
    }


    /// Whether any client has been paired.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
    /// Check a bearer token.
    #[must_use]
    pub fn is_valid(&self, token: &str) -> bool {
        self.clients.iter().any(|client| client.token == token)
    }

    /// Add or replace a paired credential.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn add(&mut self, credential: &Credential) -> io::Result<()> {
        self.clients.retain(|client| client.client_public != credential.host_key);
        self.clients.push(TrustedClient {
            token: credential.token.clone(),
            client_public: credential.host_key.clone(),
            paired_at: credential.paired_at.clone(),
        });
        self.persist()
    }

    /// Revoke a client by token.
    ///
    /// # Errors
    /// Returns an IO error when the store cannot be persisted.
    pub fn revoke(&mut self, token: &str) -> io::Result<bool> {
        let before = self.clients.len();
        self.clients.retain(|client| client.token != token);
        let removed = self.clients.len() != before;
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.clients).unwrap())?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(token: &str) -> Credential {
        Credential {
            box_id: "a".repeat(32),
            token: token.into(),
            host_key: "b".repeat(64),
            host_signature: "c".repeat(128),
            box_spki_sha256: "d".repeat(64),
            capabilities: vec!["cuda".into()],
            paired_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn issued_tokens_validate_and_revoke() {
        let path = std::env::temp_dir().join(format!("olgp-auth-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut store = TrustedClients::load(&path).unwrap();
        assert!(!store.is_valid("token"));
        assert!(store.is_empty());
        store.add(&credential("token")).unwrap();
        assert!(store.is_valid("token"));
        assert!(!store.is_empty());
        assert!(store.revoke("token").unwrap());
        assert!(!store.is_valid("token"));
        fs::remove_file(path).unwrap();
    }
}
