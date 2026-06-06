//! Audit events and sinks.
//!
//! An [`AuditEvent`] is an immutable record of a security- or compliance-relevant
//! action: who did what, to which resource, with what outcome, and when. Build
//! one with [`AuditEvent::builder`], then hand it to an [`AuditSink`].
//! [`InMemoryAuditSink`] retains events so tests can assert on them.
//!
//! ```
//! use klauthed_platform::audit::{AuditEvent, AuditOutcome};
//!
//! let event = AuditEvent::builder("tenant.suspend")
//!     .actor("admin-1")
//!     .tenant("acme")
//!     .resource("tenant", "acme")
//!     .outcome(AuditOutcome::Success)
//!     .metadata("reason", "non-payment")
//!     .build();
//!
//! assert_eq!(event.action(), "tenant.suspend");
//! assert!(event.outcome().is_success());
//! ```

pub mod model;
pub mod sink;

pub use model::{Audit, AuditEvent, AuditEventBuilder, AuditId, AuditOutcome};
#[cfg(feature = "audit-outbox")]
pub use sink::OutboxAuditSink;
pub use sink::{AuditSink, InMemoryAuditSink};

#[cfg(test)]
mod tests;
