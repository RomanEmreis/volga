//! Token store abstraction
//!
//! [`TokenStore`] lets [`OAuthClient`](crate::OAuthClient) persist and
//! reuse tokens across requests: [`token`](crate::OAuthClient::token)
//! reads through it and refreshes expired entries transparently. The key
//! is chosen by the application — typically a user or session identifier,
//! combined with the resource when one client serves several audiences.
//!
//! [`InMemoryTokenStore`] is the built-in process-local implementation;
//! anything durable (database, encrypted file, OS keychain) is one trait
//! impl away. Implementations own their eviction policy.

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use crate::TokenSet;

/// Storage for tokens obtained by [`OAuthClient`](crate::OAuthClient)
pub trait TokenStore: Send + Sync {
    /// Returns the tokens stored under `key`
    fn get(&self, key: &str) -> Option<TokenSet>;

    /// Stores `tokens` under `key`, replacing any previous entry
    fn put(&self, key: &str, tokens: &TokenSet);

    /// Removes the entry stored under `key`
    fn remove(&self, key: &str);
}

/// A process-local [`TokenStore`] backed by a mutex-guarded map
///
/// Suitable for CLIs, tests and single-instance services; tokens do not
/// survive a restart.
#[derive(Debug, Default)]
pub struct InMemoryTokenStore {
    entries: Mutex<HashMap<String, TokenSet>>,
}

impl InMemoryTokenStore {
    /// Creates an empty store
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for InMemoryTokenStore {
    fn get(&self, key: &str) -> Option<TokenSet> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn put(&self, key: &str, tokens: &TokenSet) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), tokens.clone());
    }

    fn remove(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(access_token: &str) -> TokenSet {
        TokenSet {
            access_token: access_token.into(),
            token_type: "Bearer".into(),
            refresh_token: None,
            scope: None,
            id_token: None,
            expires_at: None,
        }
    }

    #[test]
    fn it_stores_replaces_and_removes_entries() {
        let store = InMemoryTokenStore::new();
        assert!(store.get("alice").is_none());

        store.put("alice", &tokens("a1"));
        store.put("bob", &tokens("b1"));
        assert_eq!(store.get("alice").unwrap().access_token, "a1");

        store.put("alice", &tokens("a2"));
        assert_eq!(store.get("alice").unwrap().access_token, "a2");

        store.remove("alice");
        assert!(store.get("alice").is_none());
        assert!(store.get("bob").is_some());
    }
}
