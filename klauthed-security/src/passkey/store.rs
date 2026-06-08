//! [`PasskeyStore`] — async storage for users' registered passkeys.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use webauthn_rs::prelude::{Passkey, Uuid};

use crate::error::SecurityError;

/// Async storage for the passkeys registered to each user.
///
/// A user may enroll several passkeys (phone, laptop, security key), so the
/// store maps a user's WebAuthn handle ([`Uuid`]) to a list of [`Passkey`]s. The
/// trait is the stable seam between the ceremony logic
/// ([`PasskeyAuthenticator`](super::PasskeyAuthenticator)) and whatever backend
/// (SQL, Redis, in-memory) actually persists credentials.
#[async_trait]
pub trait PasskeyStore: Send + Sync {
    /// All passkeys registered for `user_id` (empty if the user has none).
    ///
    /// # Errors
    /// Returns [`SecurityError`] only on backend failure.
    async fn list(&self, user_id: Uuid) -> Result<Vec<Passkey>, SecurityError>;

    /// Register an additional `passkey` for `user_id`.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn add(&self, user_id: Uuid, passkey: Passkey) -> Result<(), SecurityError>;

    /// Replace the stored credential whose id matches `passkey` for `user_id` —
    /// e.g. to persist the updated signature counter after a successful
    /// authentication (see [`Passkey::update_credential`]). Returns `true` if a
    /// matching credential was found and replaced.
    ///
    /// # Errors
    /// Returns [`SecurityError`] on backend failure.
    async fn update(&self, user_id: Uuid, passkey: &Passkey) -> Result<bool, SecurityError>;
}

/// A thread-safe, in-memory [`PasskeyStore`] for testing and development.
///
/// Cloneable handles share one backing map (`Arc<Mutex<…>>`); wire a DB-backed
/// store in production.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPasskeyStore {
    by_user: Arc<Mutex<HashMap<Uuid, Vec<Passkey>>>>,
}

impl InMemoryPasskeyStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of passkeys stored across all users.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_user
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Whether no passkeys are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl PasskeyStore for InMemoryPasskeyStore {
    async fn list(&self, user_id: Uuid) -> Result<Vec<Passkey>, SecurityError> {
        Ok(self
            .by_user
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&user_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn add(&self, user_id: Uuid, passkey: Passkey) -> Result<(), SecurityError> {
        self.by_user
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(user_id)
            .or_default()
            .push(passkey);
        Ok(())
    }

    async fn update(&self, user_id: Uuid, passkey: &Passkey) -> Result<bool, SecurityError> {
        let mut guard = self.by_user.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(list) = guard.get_mut(&user_id) else {
            return Ok(false);
        };
        match list.iter_mut().find(|stored| stored.cred_id() == passkey.cred_id()) {
            Some(stored) => {
                *stored = passkey.clone();
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_is_empty_for_unknown_user() {
        let store = InMemoryPasskeyStore::new();
        assert!(store.list(Uuid::new_v4()).await.unwrap().is_empty());
        assert!(store.is_empty());
    }

    // Round-trip behavior with real `Passkey`s is covered end-to-end (against a
    // software authenticator) in `tests/passkey.rs`.
}
