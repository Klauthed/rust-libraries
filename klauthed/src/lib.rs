#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! # klauthed
//!
//! Umbrella "starter" crate for the klauthed framework. Depend on this one crate
//! and turn on the pieces you need with features; each enabled library is
//! re-exported as a module (`klauthed::core`, `klauthed::web`, …), and the most
//! common items are available from [`prelude`].
//!
//! ```toml
//! # A typical actix-web service:
//! klauthed = { version = "0.1", features = ["web", "data", "observability", "security", "postgres"] }
//! ```
//!
//! ## Feature map
//!
//! | Feature         | Re-exports / enables                                  |
//! |-----------------|-------------------------------------------------------|
//! | `core`          | `klauthed::core` (config, id, time, context, domain, cqrs, validation). Implies `error`. |
//! | `error`         | `klauthed::error` (the `DomainError` kernel)          |
//! | `macros`        | `klauthed::macros` (`#[derive(DomainError)]`)         |
//! | `data`          | `klauthed::data` (db/cache/messaging/storage + outbox/idempotency/locks) |
//! | `discovery`     | `klauthed::discovery` (service registry: in-memory, Consul, Eureka) |
//! | `web`           | `klauthed::web` (actix `AppError`, context middleware, health) |
//! | `observability` | `klauthed::observability` (logging/metrics/otel)      |
//! | `i18n`          | `klauthed::i18n` (message catalogs)                   |
//! | `security`      | `klauthed::security` (password hashing, JWT, tokens)  |
//! | `platform`      | `klauthed::platform` (tenancy, feature flags, audit)  |
//! | `protocol`      | `klauthed::protocol` (OIDC, SCIM)                     |
//! | `full`          | all of the above (each with its own defaults)         |
//!
//! Pass-through features forward to a sub-crate's own feature and pull in the
//! owning crate: `vault`/`config-server`/`hot-reload`/`task-local`/`tz` (core),
//! `sealed`/`webauthn` (security), `context-scope` (web),
//! `scheduler` (platform),
//! `metrics`/`otel` (observability), `consul`/`eureka`/`agent` (discovery),
//! `postgres`/`mysql`/`sqlite`, `redis`/`cache-memory`,
//! `nats`/`rabbitmq`/`kafka`, `storage`/`storage-s3`/`storage-gcs`/`storage-azure`.

#[cfg(feature = "core")]
pub use klauthed_core as core;
#[cfg(feature = "data")]
pub use klauthed_data as data;
#[cfg(feature = "discovery")]
pub use klauthed_discovery as discovery;
#[cfg(feature = "error")]
pub use klauthed_error as error;
#[cfg(feature = "i18n")]
pub use klauthed_i18n as i18n;
#[cfg(feature = "macros")]
pub use klauthed_macros as macros;
#[cfg(feature = "observability")]
pub use klauthed_observability as observability;
#[cfg(feature = "platform")]
pub use klauthed_platform as platform;
#[cfg(feature = "protocol")]
pub use klauthed_protocol as protocol;
#[cfg(feature = "security")]
pub use klauthed_security as security;
#[cfg(feature = "testing")]
pub use klauthed_testing as testing;
#[cfg(feature = "web")]
pub use klauthed_web as web;

/// The things a klauthed service reaches for most often, gated by the features
/// you enabled.
///
/// ```
/// # #[cfg(all(feature = "core", feature = "error"))]
/// use klauthed::prelude::*;
/// ```
pub mod prelude {
    // Error kernel: the `DomainError` trait, its derive macro (same name, macro
    // namespace), and the category/code types.
    #[cfg(feature = "error")]
    pub use klauthed_error::{DomainError, ErrorCategory, ErrorCode};
    #[cfg(feature = "macros")]
    pub use klauthed_macros::DomainError;

    // Core building blocks.
    #[cfg(feature = "core")]
    pub use klauthed_core::config::{Config, Profile};
    #[cfg(feature = "core")]
    pub use klauthed_core::context::RequestContext;
    #[cfg(feature = "core")]
    pub use klauthed_core::id::Id;
    #[cfg(feature = "core")]
    pub use klauthed_core::time::{Clock, SystemClock, Timestamp};
    #[cfg(feature = "core")]
    pub use klauthed_core::validation::{Validate, ValidationErrors};

    // Service discovery.
    #[cfg(feature = "discovery")]
    pub use klauthed_discovery::{ServiceInstance, ServiceRegistry};

    // HTTP layer.
    #[cfg(feature = "web")]
    pub use klauthed_web::{AppError, AppResult};

    // Internationalization.
    #[cfg(feature = "i18n")]
    pub use klauthed_i18n::{Args, I18n, Locale};
}

#[cfg(test)]
mod tests {
    /// Smoke test: with the default (`core`) feature, the re-export and prelude
    /// resolve to the real crate items.
    #[test]
    #[cfg(feature = "core")]
    fn core_reexport_and_prelude_resolve() {
        use crate::prelude::*;

        // Re-exported module path works.
        let _profile = crate::core::config::Profile::default();
        // Prelude brings the types into scope.
        let _profile2: Profile = Profile::Local;
        let _id = Id::<()>::nil();
    }

    /// The `discovery` feature re-exports the crate and surfaces its types in the
    /// prelude.
    #[test]
    #[cfg(feature = "discovery")]
    fn discovery_reexport_and_prelude_resolve() {
        use crate::prelude::*;

        let _instance: ServiceInstance = crate::discovery::ServiceInstance::new("svc", "h", 1);
        // `ServiceRegistry` is in scope from the prelude (used as a trait bound).
        fn _needs_registry<R: ServiceRegistry>() {}
    }
}
