//! `ClientStore` trait and in-memory implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::error::SecurityError;

use super::client::OAuth2Client;

// ── ClientStore ───────────────────────────────────────────────────────────────

/// Async storage for registered [`OAuth2Client`]s.
///
/// The stable seam between the authorization server logic and whatever backend
/// (SQL, Redis, in-memory) holds the client registrations.
#[async_trait]
pub trait ClientStore: Send + Sync {
    /// Look up a client by its `client_id`. Returns `None` if not registered.
    ///
    /// # Errors
    /// Returns [`SecurityError`] only on backend failure.
    async fn get(&self, client_id: &str) -> Result<Option<OAuth2Client>, SecurityError>;

    /// Persist a newly registered client (or replace an existing one with the
    /// same `client_id`).
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn register(&self, client: OAuth2Client) -> Result<(), SecurityError>;

    /// Remove a client registration. Idempotent — deleting an unknown id is `Ok`.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn delete(&self, client_id: &str) -> Result<(), SecurityError>;
}

// ── InMemoryClientStore ───────────────────────────────────────────────────────

/// A thread-safe, in-memory [`ClientStore`] for testing and development.
///
/// Cloneable handles share the same backing map (`Arc<Mutex<…>>`). Wire a
/// DB-backed store in production.
#[derive(Debug, Default, Clone)]
pub struct InMemoryClientStore {
    clients: Arc<Mutex<HashMap<String, OAuth2Client>>>,
}

impl InMemoryClientStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered clients.
    #[must_use]
    pub fn len(&self) -> usize {
        self.clients.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len()
    }

    /// Whether no clients are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clients.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty()
    }
}

#[async_trait]
impl ClientStore for InMemoryClientStore {
    async fn get(&self, client_id: &str) -> Result<Option<OAuth2Client>, SecurityError> {
        Ok(self
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(client_id)
            .cloned())
    }

    async fn register(&self, client: OAuth2Client) -> Result<(), SecurityError> {
        self.clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(client.client_id.clone(), client);
        Ok(())
    }

    async fn delete(&self, client_id: &str) -> Result<(), SecurityError> {
        self.clients.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(client_id);
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::client::{ClientGrantType, ClientType, TokenEndpointAuthMethod};
    use super::*;
    use klauthed_core::time::Timestamp;

    fn test_client(id: &str) -> OAuth2Client {
        OAuth2Client {
            client_id: id.into(),
            client_type: ClientType::Confidential,
            client_secret_hash: Some("$argon2id$v=19$...".into()),
            redirect_uris: vec!["https://app.example.com/cb".into()],
            allowed_grant_types: [ClientGrantType::AuthorizationCode].into_iter().collect(),
            allowed_scopes: ["openid"].iter().map(|s| s.to_string()).collect(),
            token_endpoint_auth_method: TokenEndpointAuthMethod::ClientSecretBasic,
            client_name: Some("Test".into()),
            created_at: Timestamp::now(),
        }
    }

    #[tokio::test]
    async fn register_then_get_round_trips() {
        let store = InMemoryClientStore::new();
        store.register(test_client("c1")).await.unwrap();
        let got = store.get("c1").await.unwrap().unwrap();
        assert_eq!(got.client_id, "c1");
    }

    #[tokio::test]
    async fn get_unknown_returns_none() {
        assert!(InMemoryClientStore::new().get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let store = InMemoryClientStore::new();
        store.register(test_client("c")).await.unwrap();
        store.delete("c").await.unwrap();
        assert!(store.is_empty());
        store.delete("c").await.unwrap(); // second delete is fine
    }

    #[tokio::test]
    async fn register_replaces_existing() {
        let store = InMemoryClientStore::new();
        store.register(test_client("c")).await.unwrap();
        let mut updated = test_client("c");
        updated.client_name = Some("Updated".into());
        store.register(updated).await.unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.get("c").await.unwrap().unwrap().client_name.as_deref(), Some("Updated"));
    }

    #[tokio::test]
    async fn cloned_handles_share_state() {
        let store = InMemoryClientStore::new();
        let clone = store.clone();
        store.register(test_client("shared")).await.unwrap();
        assert!(clone.get("shared").await.unwrap().is_some());
    }
}
