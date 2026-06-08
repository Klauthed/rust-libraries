#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]
//! Procedural macros for klauthed.
//!
//! Currently: [`macro@DomainError`], a derive that generates the
//! `klauthed_error::DomainError` impl from `#[domain(...)]` attributes, so error
//! types don't hand-write the `category()` / `code()` match arms.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

use expand::expand;

mod expand;
mod from_config;
mod parse;

/// Derive `klauthed_error::DomainError`.
///
/// Annotate variants (or a struct) with `#[domain(...)]`:
///
/// ## Compile-time validation
///
/// Both `code` and `prefix` are validated at macro-expansion time:
/// they must match `[a-z][a-z0-9_]*` (plus dots in `code` for fully-qualified
/// codes). Violations are hard compile errors, not silent runtime bugs.
///
/// ```compile_fail
/// # use klauthed_macros::DomainError;
/// #[derive(Debug, DomainError)]
/// #[domain(prefix = "BadPrefix")]  // uppercase → compile error
/// enum Bad { A }
/// # impl std::fmt::Display for Bad { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }
/// # impl std::error::Error for Bad {}
/// ```
///
/// ```compile_fail
/// # use klauthed_macros::DomainError;
/// #[derive(Debug, DomainError)]
/// #[domain(prefix = "my")]
/// enum Bad {
///     #[domain(code = "bad code with spaces")]  // spaces → compile error
///     A,
/// }
/// # impl std::fmt::Display for Bad { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }
/// # impl std::error::Error for Bad {}
/// ```
///
/// * `category = "internal"` — one of the snake-case `ErrorCategory` names
///   (`bad_request`, `unauthorized`, `forbidden`, `not_found`, `conflict`,
///   `rate_limited`, `timeout`, `unavailable`, `internal`). Defaults to the
///   container's `category`, else `internal`.
/// * `code = "missing"` — the error code. Defaults to the snake-cased variant
///   name. A container `#[domain(prefix = "config")]` prefixes every code, so
///   `MissingRequired` → `config.missing_required`.
/// * `transparent` — delegate `category()` and `code()` to the variant's single
///   field (which must itself be a `DomainError`), for wrapped/`#[from]` errors.
///
/// The type must also implement `std::error::Error` (e.g. via `thiserror`),
/// since `DomainError` requires it.
#[proc_macro_derive(DomainError, attributes(domain))]
pub fn derive_domain_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Derive `klauthed_core::config::FromConfig`.
///
/// Binds a struct to a section of the resolved configuration, generating a
/// `from_config(&Config)` that reads (and deserializes) it — the klauthed analog
/// of Spring's `@ConfigurationProperties`. The type must also implement
/// `serde::Deserialize`.
///
/// ```text
/// #[derive(serde::Deserialize, FromConfig)]
/// #[config(key = "database")]        // defaults to the snake_cased type name
/// struct DatabaseSettings { /* … */ }
///
/// #[derive(Default, serde::Deserialize, FromConfig)]
/// #[config(key = "cache", default)]  // a missing section binds to `Default`
/// struct CacheSettings { /* … */ }
/// ```
#[proc_macro_derive(FromConfig, attributes(config))]
pub fn derive_from_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    from_config::expand(input).unwrap_or_else(syn::Error::into_compile_error).into()
}
