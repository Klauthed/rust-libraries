//! The [`ServiceRegistry`] trait and the in-memory [`InMemoryRegistry`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::error::DiscoveryError;
use crate::instance::ServiceInstance;

/// Registration and lookup against a service registry.
///
/// The stable seam between a service and whatever registry backs discovery
/// ([`InMemoryRegistry`] for tests, Consul or Eureka in production). Methods are
/// async because real backends perform network I/O.
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// Register (or re-register) `instance`, replacing any existing entry with
    /// the same `instance_id`.
    ///
    /// # Errors
    /// Returns [`DiscoveryError`] if the backend rejects or cannot be reached.
    async fn register(&self, instance: &ServiceInstance) -> Result<(), DiscoveryError>;

    /// Remove the instance `instance_id` of `service_name`. Idempotent —
    /// deregistering an unknown instance is `Ok`.
    ///
    /// # Errors
    /// Returns [`DiscoveryError`] if the backend cannot be reached.
    async fn deregister(&self, service_name: &str, instance_id: &str)
    -> Result<(), DiscoveryError>;

    /// Renew the lease for an instance (backends that expire registrations).
    ///
    /// # Errors
    /// Returns [`DiscoveryError`] if the backend cannot be reached.
    async fn heartbeat(&self, service_name: &str, instance_id: &str) -> Result<(), DiscoveryError>;

    /// All currently-registered instances of `service_name` (empty if none).
    ///
    /// # Errors
    /// Returns [`DiscoveryError`] if the backend cannot be reached.
    async fn instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, DiscoveryError>;
}

/// A thread-safe, in-memory [`ServiceRegistry`] for tests and single-process use.
///
/// Cloneable handles share one backing map (`Arc<Mutex<…>>`). Heartbeats are
/// no-ops (in-memory registrations never expire); wire Consul or Eureka in a
/// distributed deployment.
#[derive(Debug, Default, Clone)]
pub struct InMemoryRegistry {
    by_service: Arc<Mutex<HashMap<String, Vec<ServiceInstance>>>>,
}

impl InMemoryRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of registered instances across all services.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_service
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Whether no instances are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl ServiceRegistry for InMemoryRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), DiscoveryError> {
        let mut guard = self.by_service.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let list = guard.entry(instance.service_name.clone()).or_default();
        match list.iter_mut().find(|existing| existing.instance_id == instance.instance_id) {
            Some(existing) => *existing = instance.clone(),
            None => list.push(instance.clone()),
        }
        Ok(())
    }

    async fn deregister(
        &self,
        service_name: &str,
        instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        let mut guard = self.by_service.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(list) = guard.get_mut(service_name) {
            list.retain(|instance| instance.instance_id != instance_id);
            if list.is_empty() {
                guard.remove(service_name);
            }
        }
        Ok(())
    }

    async fn heartbeat(
        &self,
        _service_name: &str,
        _instance_id: &str,
    ) -> Result<(), DiscoveryError> {
        // In-memory registrations never expire, so a heartbeat is a no-op.
        Ok(())
    }

    async fn instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>, DiscoveryError> {
        Ok(self
            .by_service
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(service_name)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(service: &str, id: &str, port: u16) -> ServiceInstance {
        ServiceInstance::new(service, "host", port).with_instance_id(id)
    }

    #[tokio::test]
    async fn register_list_and_deregister() {
        let reg = InMemoryRegistry::new();
        reg.register(&instance("auth", "a1", 1)).await.unwrap();
        reg.register(&instance("auth", "a2", 2)).await.unwrap();
        reg.register(&instance("billing", "b1", 3)).await.unwrap();

        assert_eq!(reg.instances("auth").await.unwrap().len(), 2);
        assert_eq!(reg.len(), 3);

        reg.deregister("auth", "a1").await.unwrap();
        assert_eq!(reg.instances("auth").await.unwrap().len(), 1);

        // Deregistering an unknown instance is a no-op.
        reg.deregister("auth", "missing").await.unwrap();
        assert_eq!(reg.instances("auth").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn register_replaces_same_instance_id() {
        let reg = InMemoryRegistry::new();
        reg.register(&instance("auth", "a1", 1)).await.unwrap();
        reg.register(&instance("auth", "a1", 9)).await.unwrap(); // same id, new port
        let list = reg.instances("auth").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].port, 9);
    }

    #[tokio::test]
    async fn unknown_service_lists_empty() {
        let reg = InMemoryRegistry::new();
        assert!(reg.instances("nope").await.unwrap().is_empty());
        assert!(reg.is_empty());
        reg.heartbeat("nope", "x").await.unwrap(); // no-op, always Ok
    }
}
