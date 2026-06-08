#![deny(unsafe_code)]
#![deny(missing_docs)]

//! Cross-cutting platform concerns for klauthed services.
//!
//! This crate provides the data structures, traits, and in-memory implementations
//! for three foundational platform capabilities, each in its own module:
//!
//! * [`tenancy`] — the [`Tenant`] model, [`TenantStatus`], a [`TenantResolver`]
//!   trait (with an in-memory impl), and a helper to read the tenant from a
//!   [`RequestContext`](klauthed_core::context::RequestContext).
//! * [`featureflag`] — a [`FeatureFlag`] key type, the [`FeatureFlags`] trait, and
//!   an [`InMemoryFeatureFlags`] provider with global defaults plus per-tenant
//!   overrides and multivariate values.
//! * [`audit`] — the [`AuditEvent`] record (with an ergonomic builder), the
//!   [`AuditSink`] trait, and an [`InMemoryAuditSink`] that retains events.
//! * [`jobs`] — a background-job *store* abstraction: [`JobStatus`], the
//!   [`EnqueuedJob`] record, the async [`JobQueue`] trait, and a clock-driven
//!   [`InMemoryJobQueue`] (queueing only — no worker runtime).
//! * [`webhooks`] — [`WebhookEndpoint`]/[`WebhookEvent`] types, HMAC-SHA256
//!   [`sign_payload`]/[`verify_signature`] helpers, the async [`WebhookSender`]
//!   trait, and a [`RecordingWebhookSender`] (signs + records, no network).
//!
//! All errors are reported via [`PlatformError`], which implements
//! [`DomainError`](klauthed_error::DomainError) (codes `platform.*`).
//!
//! Heavier platform concerns — metering, notifications, and messaging, plus a
//! real HTTP webhook transport and a job worker runtime — are intentionally out
//! of scope for this cut and will land in follow-up modules.
//!
//! ```
//! use klauthed_core::context::RequestContext;
//! use klauthed_platform::audit::{AuditEvent, AuditOutcome};
//! use klauthed_platform::featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
//! use klauthed_platform::tenancy::{Tenant, TenantStatus};
//!
//! // Tenancy.
//! let tenant = Tenant::new("acme").with_name("Acme, Inc.");
//! assert!(tenant.ensure_active().is_ok());
//!
//! // Feature flags, scoped by request context.
//! let beta = FeatureFlag::new("beta_ui");
//! let flags = InMemoryFeatureFlags::new().with_tenant_override("acme", &beta, true);
//! let ctx = RequestContext::new().with_tenant("acme");
//! assert!(flags.is_enabled(&beta, &ctx));
//!
//! // Audit.
//! let event = AuditEvent::builder("tenant.read")
//!     .from_context(&ctx)
//!     .outcome(AuditOutcome::Success)
//!     .build();
//! assert_eq!(event.action(), "tenant.read");
//! ```

pub mod audit;
pub mod error;
pub mod featureflag;
pub mod jobs;
pub mod tenancy;
pub mod webhooks;

pub use audit::{
    Audit, AuditEvent, AuditEventBuilder, AuditId, AuditOutcome, AuditSink, InMemoryAuditSink,
};
pub use error::PlatformError;
pub use featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
pub use jobs::{
    DEFAULT_MAX_ATTEMPTS, EnqueuedJob, InMemoryJobQueue, Job, JobId, JobQueue, JobStatus,
};
pub use tenancy::{
    InMemoryTenantResolver, Tenant, TenantId, TenantResolver, TenantStatus, tenant_from_context,
};
pub use webhooks::{
    RecordingWebhookSender, WebhookDelivery, WebhookEndpoint, WebhookEndpointId, WebhookEvent,
    WebhookEventId, WebhookSender, sign_payload, verify_signature,
};
