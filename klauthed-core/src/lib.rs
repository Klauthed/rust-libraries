#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! Core primitives shared across the klauthed libraries.
//!
//! * [`config`] — typed, layered configuration (files, env, Vault) with profiles.
//! * [`time`] — an injectable [`Clock`](time::Clock) and the canonical UTC
//!   [`Timestamp`](time::Timestamp) / [`Duration`](time::Duration).
//! * [`context`] — the per-request [`RequestContext`](context::RequestContext).
//! * [`id`] — phantom-typed 128-bit [`Id`](id::Id)s.
//! * [`cqrs`] — command/query/event buses.
//! * [`domain`] — domain-event / aggregate building blocks.
//! * [`validation`] — the [`Validate`](validation::Validate) trait.
//! * [`wiring`] — the [`AppContext`](wiring::AppContext) component registry.
//! * [`error`] — re-exports the error kernel plus `ConfigError`.

// Lets the `#[derive(FromConfig)]` macro's `::klauthed_core::…` paths resolve
// when the derive is used inside this crate (e.g. its own tests).
extern crate self as klauthed_core;

pub mod config;
pub mod context;
pub mod cqrs;
pub mod domain;
pub mod error;
pub mod id;
pub mod time;
pub mod validation;
pub mod wiring;

/// Common imports for a klauthed service: `use klauthed_core::prelude::*;`.
pub mod prelude {
    pub use crate::config::{Config, ConfigBuilder, ConfigProvider, FromConfig, Profile};
    pub use crate::context::RequestContext;
    pub use crate::id::Id;
    pub use crate::time::{Clock, Duration, SystemClock, Timestamp};
    pub use crate::validation::Validate;
    pub use crate::wiring::{AppBuilder, AppContext, Starter};
}

#[cfg(test)]
mod derive_crate_override {
    //! A crate depending only on the `klauthed` umbrella reaches the error trait
    //! through a re-export; `#[domain(crate = "…")]` points the derive at it. Here a
    //! local module stands in for the umbrella's `klauthed::error`.

    mod umbrella {
        pub use klauthed_error as error;
    }

    use klauthed_error::{DomainError as _, ErrorCategory};
    use klauthed_macros::DomainError;

    #[derive(Debug, DomainError)]
    #[domain(crate = "umbrella::error", prefix = "test", category = "not_found")]
    struct OverriddenError;

    impl std::fmt::Display for OverriddenError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("overridden")
        }
    }
    impl std::error::Error for OverriddenError {}

    #[test]
    fn crate_override_resolves_the_trait_via_a_reexport() {
        let err = OverriddenError;
        // Compiling at all proves `crate = "umbrella::error"` resolved the trait;
        // the values confirm the generated bodies are correct.
        assert_eq!(err.category(), ErrorCategory::NotFound);
        assert_eq!(err.code().as_str(), "test.overridden_error");
    }
}
