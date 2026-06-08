#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

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
