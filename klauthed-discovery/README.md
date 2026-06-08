# klauthed-discovery

Service discovery for the [klauthed](https://github.com/klauthed/rust-libraries)
libraries: a small `ServiceRegistry` abstraction so services can register
themselves and resolve their peers, independent of the backing system.

- `ServiceInstance` — where one instance of a service lives, plus metadata.
- `ServiceRegistry` — async `register` / `deregister` / `heartbeat` /
  `instances`. `InMemoryRegistry` backs tests and single-process use; Consul and
  Eureka backends are available behind the `consul` / `eureka` features.
- `RoundRobin` — lock-free client-side load balancing over resolved instances.

```rust
use klauthed_discovery::{InMemoryRegistry, RoundRobin, ServiceInstance, ServiceRegistry};

let registry = InMemoryRegistry::new();
registry.register(&ServiceInstance::new("auth-api", "10.0.0.1", 8080)).await?;

let instances = registry.instances("auth-api").await?;
if let Some(target) = RoundRobin::new().pick(&instances) {
    // call target.base_url()
}
```

Licensed under MIT OR Apache-2.0.
