use std::collections::HashMap;

use super::extractor::AuthenticatedUser;

/// In-memory store mapping API keys to authenticated users.
///
/// API keys are provided via `X-API-Key` header as an alternative to JWT.
/// Keys are loaded at startup (from config or environment).
#[derive(Debug, Clone)]
pub struct ApiKeyStore {
    keys: HashMap<String, AuthenticatedUser>,
}

impl ApiKeyStore {
    /// Creates an empty API key store.
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    /// Registers an API key mapping to a user identity.
    pub fn insert(&mut self, key: String, user_id: String, role: String) {
        self.keys.insert(
            key,
            AuthenticatedUser {
                sub: user_id,
                role,
                tenant_id: None,
            },
        );
    }

    /// Looks up an API key and returns the associated user, if valid.
    pub fn lookup(&self, key: &str) -> Option<AuthenticatedUser> {
        self.keys.get(key).cloned()
    }

    /// Returns the number of registered API keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns true if no API keys are registered.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl Default for ApiKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup() {
        let mut store = ApiKeyStore::new();
        store.insert(
            "sk-test-123".to_string(),
            "user-1".to_string(),
            "admin".to_string(),
        );
        let user = store.lookup("sk-test-123");
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.sub, "user-1");
        assert_eq!(user.role, "admin");
    }

    #[test]
    fn lookup_missing_returns_none() {
        let store = ApiKeyStore::new();
        assert!(store.lookup("nonexistent").is_none());
    }

    #[test]
    fn len_and_is_empty() {
        let mut store = ApiKeyStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.insert("k1".to_string(), "u1".to_string(), "admin".to_string());
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }
}
