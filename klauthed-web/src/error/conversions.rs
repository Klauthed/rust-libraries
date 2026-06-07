//! `From` impls converting the common crate errors into [`AppError`], so handler
//! code can use `?` directly.

use super::AppError;

// ── From the common domain errors, for `?` ergonomics ─────────────────────────
//
// (No blanket `impl<E: DomainError> From<E>`, which would collide with the std
// reflexive `From<T> for T`; concrete impls are unambiguous.)

impl From<klauthed_core::validation::ValidationErrors> for AppError {
    fn from(error: klauthed_core::validation::ValidationErrors) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_core::error::ConfigError> for AppError {
    fn from(error: klauthed_core::error::ConfigError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_data::DataError> for AppError {
    fn from(error: klauthed_data::DataError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_security::SecurityError> for AppError {
    fn from(error: klauthed_security::SecurityError) -> Self {
        Self::from_domain(error)
    }
}

impl From<klauthed_platform::PlatformError> for AppError {
    fn from(error: klauthed_platform::PlatformError) -> Self {
        Self::from_domain(error)
    }
}
