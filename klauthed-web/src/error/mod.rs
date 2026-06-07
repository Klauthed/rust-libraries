//! [`AppError`] — the aggregate error HTTP handlers return.
//!
//! Any [`DomainError`](klauthed_error::DomainError) (a `ConfigError`, `DataError`, or a future crate's error)
//! converts into `AppError`, which captures the error's `category`, stable
//! `code`, and message at construction. Its actix `ResponseError` impl then
//! renders a uniform JSON body and the right HTTP status for every error in the
//! system.
//!
//! Security note: client errors (`4xx`) include their message in the response,
//! but server errors (`5xx`) return a generic message to the client while the
//! real one is logged — so internal details never leak to callers.

pub mod app;
pub mod conversions;
pub mod response;

pub use app::{AppError, AppResult};
