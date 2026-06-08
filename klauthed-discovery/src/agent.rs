//! [`ServiceAgent`] — register on start, heartbeat in the background, deregister
//! on shutdown (`feature = "agent"`).

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::error::DiscoveryError;
use crate::instance::ServiceInstance;
use crate::registry::ServiceRegistry;

/// A running registration: it registers an instance with a [`ServiceRegistry`],
/// renews the lease on an interval, and deregisters when it is shut down or
/// dropped.
///
/// Hold it for as long as the service should appear in discovery. Prefer
/// [`shutdown`](Self::shutdown) for a deterministic, awaited deregister; if the
/// agent is merely dropped, it deregisters on a best-effort detached task
/// (requires a Tokio runtime to still be running).
///
/// ```no_run
/// use std::sync::Arc;
/// use std::time::Duration;
/// use klauthed_discovery::{InMemoryRegistry, ServiceInstance};
/// use klauthed_discovery::agent::ServiceAgent;
///
/// # async fn run() -> Result<(), klauthed_discovery::DiscoveryError> {
/// let registry = Arc::new(InMemoryRegistry::new());
/// let instance = ServiceInstance::new("auth-api", "10.0.0.1", 8080);
///
/// let agent = ServiceAgent::start(registry, instance, Duration::from_secs(10)).await?;
/// // ... serve requests; the agent heartbeats in the background ...
/// agent.shutdown().await?;
/// # Ok(())
/// # }
/// ```
pub struct ServiceAgent {
    registry: Arc<dyn ServiceRegistry>,
    service_name: String,
    instance_id: String,
    heartbeat: JoinHandle<()>,
    active: bool,
}

impl ServiceAgent {
    /// Register `instance` and start heartbeating every `interval`.
    ///
    /// The first heartbeat fires one `interval` after registration. Heartbeat
    /// failures are logged and retried on the next tick rather than aborting the
    /// agent.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] if the initial registration fails.
    pub async fn start(
        registry: Arc<dyn ServiceRegistry>,
        instance: ServiceInstance,
        interval: Duration,
    ) -> Result<Self, DiscoveryError> {
        registry.register(&instance).await?;

        let service_name = instance.service_name;
        let instance_id = instance.instance_id;

        let hb_registry = Arc::clone(&registry);
        let hb_service = service_name.clone();
        let hb_instance = instance_id.clone();
        let heartbeat = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // The immediate first tick completes instantly; skip it so the first
            // real heartbeat is one interval out.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(error) = hb_registry.heartbeat(&hb_service, &hb_instance).await {
                    tracing::warn!(%error, service = %hb_service, instance = %hb_instance, "discovery heartbeat failed");
                }
            }
        });

        Ok(Self { registry, service_name, instance_id, heartbeat, active: true })
    }

    /// Stop heartbeating and deregister, awaiting the result.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] if the deregister call fails.
    pub async fn shutdown(mut self) -> Result<(), DiscoveryError> {
        self.heartbeat.abort();
        self.active = false; // suppress the best-effort deregister in `Drop`
        self.registry.deregister(&self.service_name, &self.instance_id).await
    }
}

impl Drop for ServiceAgent {
    fn drop(&mut self) {
        self.heartbeat.abort();
        if !self.active {
            return; // already deregistered via `shutdown`
        }
        // Best-effort: Drop can't be async, so detach a deregister task if a
        // runtime is still available (it usually is during graceful shutdown).
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let registry = Arc::clone(&self.registry);
            let service_name = self.service_name.clone();
            let instance_id = self.instance_id.clone();
            handle.spawn(async move {
                if let Err(error) = registry.deregister(&service_name, &instance_id).await {
                    tracing::warn!(%error, "discovery deregister on drop failed");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::InMemoryRegistry;

    #[tokio::test]
    async fn registers_on_start_and_deregisters_on_shutdown() {
        let registry = Arc::new(InMemoryRegistry::new());
        let instance = ServiceInstance::new("auth", "10.0.0.1", 8080);

        let agent = ServiceAgent::start(
            Arc::clone(&registry) as Arc<dyn ServiceRegistry>,
            instance,
            Duration::from_secs(30),
        )
        .await
        .expect("start");

        assert_eq!(registry.instances("auth").await.unwrap().len(), 1);

        agent.shutdown().await.expect("shutdown");
        assert_eq!(registry.instances("auth").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn dropping_deregisters_best_effort() {
        let registry = Arc::new(InMemoryRegistry::new());
        {
            let _agent = ServiceAgent::start(
                Arc::clone(&registry) as Arc<dyn ServiceRegistry>,
                ServiceInstance::new("auth", "10.0.0.1", 8080),
                Duration::from_secs(30),
            )
            .await
            .expect("start");
            assert_eq!(registry.instances("auth").await.unwrap().len(), 1);
        } // dropped here → detached deregister task

        // Let the detached deregister task run.
        for _ in 0..50 {
            if registry.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(registry.is_empty(), "drop should have deregistered the instance");
    }
}
