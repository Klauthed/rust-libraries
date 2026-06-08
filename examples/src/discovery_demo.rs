//! Service discovery: register instances, resolve them, round-robin across
//! them, and let a `ServiceAgent` self-register and deregister.

use std::sync::Arc;
use std::time::Duration;

use klauthed_discovery::agent::ServiceAgent;
use klauthed_discovery::{InMemoryRegistry, RoundRobin, ServiceInstance, ServiceRegistry};

/// Run the discovery demo against an in-memory registry.
pub async fn run() {
    let registry = Arc::new(InMemoryRegistry::new());

    // Two instances of the same logical service.
    registry.register(&ServiceInstance::new("auth-api", "10.0.0.1", 8080)).await.unwrap();
    registry.register(&ServiceInstance::new("auth-api", "10.0.0.2", 8080)).await.unwrap();

    let instances = registry.instances("auth-api").await.unwrap();
    println!("  resolved {} instances of auth-api", instances.len());
    assert_eq!(instances.len(), 2);

    // Client-side load balancing rotates through them.
    let lb = RoundRobin::new();
    let first = lb.pick(&instances).unwrap().base_url();
    let second = lb.pick(&instances).unwrap().base_url();
    println!("  round-robin picked: {first}, then {second}");
    assert_ne!(first, second);

    // A ServiceAgent registers on start and deregisters on shutdown.
    let agent = ServiceAgent::start(
        Arc::clone(&registry) as Arc<dyn ServiceRegistry>,
        ServiceInstance::new("billing-api", "10.0.0.9", 9090),
        Duration::from_secs(30),
    )
    .await
    .unwrap();
    println!("  agent registered billing-api ({} instances total)", registry.len());
    assert_eq!(registry.instances("billing-api").await.unwrap().len(), 1);

    agent.shutdown().await.unwrap();
    println!("  agent shut down; billing-api deregistered");
    assert!(registry.instances("billing-api").await.unwrap().is_empty());
}
