#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! Cross-cutting platform concerns for klauthed services.
//!
//! This crate provides the data structures, traits, and in-memory implementations
//! for the platform capabilities, each in its own module:
//!
//! * [`tenancy`] — the [`Tenant`] model, [`TenantStatus`], a [`TenantResolver`]
//!   trait (with an in-memory impl), and a helper to read the tenant from a
//!   [`RequestContext`](klauthed_core::context::RequestContext).
//! * [`featureflag`] — a [`FeatureFlag`] key type, the [`FeatureFlags`] trait, and
//!   an [`InMemoryFeatureFlags`] provider with global defaults plus per-tenant
//!   overrides and multivariate values.
//! * [`audit`] — the [`AuditEvent`] record (with an ergonomic builder), the
//!   [`AuditSink`] trait, and an [`InMemoryAuditSink`] that retains events.
//! * [`jobs`] — background jobs: [`JobStatus`], the [`EnqueuedJob`] record, the
//!   async [`JobQueue`] trait, a clock-driven [`InMemoryJobQueue`], and a
//!   [`JobWorker`] that drains a queue (plus durable SQL and Redis backends behind
//!   the `jobs-sql` / `jobs-redis` features).
//! * [`webhooks`] — [`WebhookEndpoint`]/[`WebhookEvent`] types, HMAC-SHA256
//!   [`sign_payload`]/[`verify_signature`] helpers, the async [`WebhookSender`]
//!   trait, and a [`RecordingWebhookSender`] (signs + records, no network).
//! * [`metering`] — per-tenant usage accounting ([`Meter`] + [`InMemoryMeter`]).
//! * [`notifications`] — user-facing messaging ([`Notifier`] + [`Notification`]).
//!
//! All errors are reported via [`PlatformError`], which implements
//! [`DomainError`](klauthed_error::DomainError) (codes `platform.*`).
//!
//! A real HTTP webhook transport is available behind the `webhook-http` feature;
//! the in-process scheduler behind `scheduler`. Messaging remains out of scope.
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
#[cfg(feature = "scheduler")]
mod cron;
pub mod error;
pub mod featureflag;
pub mod jobs;
pub mod metering;
pub mod notifications;
#[cfg(feature = "scheduler")]
pub mod scheduler;
pub mod tenancy;
pub mod webhooks;

pub use audit::{
    Audit, AuditEvent, AuditEventBuilder, AuditId, AuditOutcome, AuditSink, InMemoryAuditSink,
};
pub use error::PlatformError;
pub use featureflag::{FeatureFlag, FeatureFlags, InMemoryFeatureFlags};
#[cfg(feature = "jobs-redis")]
pub use jobs::RedisJobQueue;
#[cfg(feature = "jobs-sql")]
pub use jobs::SqlJobQueue;
pub use jobs::{
    DEFAULT_MAX_ATTEMPTS, EnqueuedJob, InMemoryJobQueue, Job, JobHandler, JobId, JobQueue,
    JobStatus, JobWorker,
};
pub use metering::{InMemoryMeter, Meter};
pub use notifications::{Channel, Notification, Notifier, RecordingNotifier};
#[cfg(feature = "scheduler")]
pub use scheduler::{Cron, CronError, Scheduler, SchedulerHandle};
pub use tenancy::{
    InMemoryTenantResolver, Tenant, TenantId, TenantResolver, TenantStatus, tenant_from_context,
};
pub use webhooks::{
    RecordingWebhookSender, WebhookDelivery, WebhookEndpoint, WebhookEndpointId, WebhookEvent,
    WebhookEventId, WebhookSender, sign_payload, verify_signature,
};

/// Common imports for the platform services: `use klauthed_platform::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "scheduler")]
    pub use crate::Scheduler;
    pub use crate::{
        Audit, AuditSink, FeatureFlag, FeatureFlags, InMemoryAuditSink, InMemoryFeatureFlags,
        InMemoryJobQueue, InMemoryMeter, InMemoryTenantResolver, Job, JobHandler, JobQueue,
        JobWorker, Meter, Notification, Notifier, PlatformError, Tenant, TenantResolver,
        WebhookSender,
    };
}
