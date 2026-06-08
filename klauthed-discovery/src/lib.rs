#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Service discovery for klauthed.
//!
//! A small abstraction over a service registry so klauthed services can register
//! themselves and resolve their peers, independent of the backing system.
//!
//! * [`ServiceInstance`] — where one instance of a service lives, plus metadata.
//! * [`ServiceRegistry`] — the async `register` / `deregister` / `heartbeat` /
//!   `instances` trait. [`InMemoryRegistry`] backs tests and single-process use;
//!   `ConsulRegistry` (feature `consul`) talks to a Consul agent and
//!   `EurekaRegistry` (feature `eureka`) talks to a Netflix Eureka server.
//! * [`RoundRobin`] — lock-free client-side load balancing over resolved
//!   instances.
//! * `ServiceAgent` (feature `agent`) — registers on start, heartbeats in the
//!   background, and deregisters on shutdown.
//!
//! ```
//! use klauthed_discovery::{InMemoryRegistry, RoundRobin, ServiceInstance, ServiceRegistry};
//!
//! # async fn run() -> Result<(), klauthed_discovery::DiscoveryError> {
//! let registry = InMemoryRegistry::new();
//! registry.register(&ServiceInstance::new("auth-api", "10.0.0.1", 8080)).await?;
//! registry.register(&ServiceInstance::new("auth-api", "10.0.0.2", 8080)).await?;
//!
//! let instances = registry.instances("auth-api").await?;
//! let lb = RoundRobin::new();
//! let chosen = lb.pick(&instances).expect("at least one instance");
//! println!("calling {}", chosen.base_url());
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "agent")]
pub mod agent;
#[cfg(feature = "consul")]
pub mod consul;
pub mod error;
#[cfg(feature = "eureka")]
pub mod eureka;
pub mod instance;
pub mod picker;
pub mod registry;

#[cfg(feature = "agent")]
pub use agent::ServiceAgent;
#[cfg(feature = "consul")]
pub use consul::ConsulRegistry;
pub use error::DiscoveryError;
#[cfg(feature = "eureka")]
pub use eureka::EurekaRegistry;
pub use instance::ServiceInstance;
pub use picker::RoundRobin;
pub use registry::{InMemoryRegistry, ServiceRegistry};
