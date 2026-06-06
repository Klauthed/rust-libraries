//! Multi-tenant primitives.
//!
//! A [`Tenant`] is the unit of isolation in the platform. It is identified by a
//! typed [`TenantId`] (an [`Id<Tenant>`](klauthed_core::id::Id)) and addressed
//! operationally by a human-readable [`slug`](Tenant::slug). A [`TenantResolver`]
//! turns an id *or* slug into a [`Tenant`]; [`InMemoryTenantResolver`] is a
//! deterministic implementation for tests and local development.
//!
//! ```
//! use klauthed_platform::tenancy::{Tenant, TenantStatus};
//!
//! let t = Tenant::new("acme").with_name("Acme, Inc.");
//! assert_eq!(t.slug(), "acme");
//! assert_eq!(t.status(), TenantStatus::Active);
//! assert!(t.is_active());
//! ```

pub mod model;
pub mod resolver;

pub use model::{Tenant, TenantId, TenantStatus};
pub use resolver::{InMemoryTenantResolver, TenantResolver, tenant_from_context};

#[cfg(test)]
mod tests;
