//! Shared immutable weights with private per-tenant KV sessions.

use std::collections::{HashMap, HashSet};

/// One immutable model resident in memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedModel {
    pub hash: String,
    pub bytes: u64,
    pub tenants: HashSet<String>,
}

/// Tracks shared model weights and private tenant KV bindings.
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: HashMap<String, SharedModel>,
    tenant_kv: HashMap<String, String>,
}

impl ModelRegistry {
    /// Bind a tenant to a model. Returns true when weights were already shared.
    pub fn retain(
        &mut self,
        model_id: &str,
        tenant: &str,
        hash: &str,
        bytes: u64,
    ) -> bool {
        let shared = self.models.contains_key(model_id);
        let entry = self.models.entry(model_id.to_owned()).or_insert_with(|| SharedModel {
            hash: hash.to_owned(),
            bytes,
            tenants: HashSet::new(),
        });
        entry.tenants.insert(tenant.to_owned());
        self.tenant_kv.insert(tenant.to_owned(), model_id.to_owned());
        shared
    }

    /// Remove a tenant binding from a model.
    pub fn release(&mut self, model_id: &str, tenant: &str) -> bool {
        self.tenant_kv.remove(tenant);
        let Some(entry) = self.models.get_mut(model_id) else {
            return false;
        };
        let removed = entry.tenants.remove(tenant);
        if entry.tenants.is_empty() {
            self.models.remove(model_id);
        }
        removed
    }

    /// Tenant KV sessions bound to a model.
    #[must_use]
    pub fn tenants(&self, model_id: &str) -> Vec<String> {
        self.models
            .get(model_id)
            .map(|entry| entry.tenants.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Model bound to one tenant session.
    #[must_use]
    pub fn tenant_model(&self, tenant: &str) -> Option<&str> {
        self.tenant_kv.get(tenant).map(String::as_str)
    }

    /// Total bytes occupied by resident model weights.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.models.values().map(|model| model.bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_tenant_shares_weights() {
        let mut registry = ModelRegistry::default();
        assert!(!registry.retain("m", "a", "hash", 100));
        assert!(registry.retain("m", "b", "hash", 100));
        assert_eq!(registry.tenants("m").len(), 2);
        assert_eq!(registry.resident_bytes(), 100);
    }

    #[test]
    fn tenant_kv_binding_is_private() {
        let mut registry = ModelRegistry::default();
        registry.retain("m", "a", "hash", 100);
        registry.retain("m", "b", "hash", 100);
        assert_eq!(registry.tenant_model("a"), Some("m"));
        assert_eq!(registry.tenant_model("b"), Some("m"));
        assert_ne!(registry.tenants("m")[0], registry.tenants("m")[1]);
    }

    #[test]
    fn releasing_last_tenant_removes_model() {
        let mut registry = ModelRegistry::default();
        registry.retain("m", "a", "hash", 100);
        registry.retain("m", "b", "hash", 100);
        assert!(registry.release("m", "a"));
        assert!(registry.release("m", "b"));
        assert!(registry.tenants("m").is_empty());
        assert_eq!(registry.resident_bytes(), 0);
    }

    #[test]
    fn releasing_unknown_tenant_is_false() {
        let mut registry = ModelRegistry::default();
        assert!(!registry.release("missing", "a"));
    }
}
