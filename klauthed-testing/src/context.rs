//! Deterministic [`RequestContext`] builders for tests.
//!
//! [`RequestContext::new`](klauthed_core::context::RequestContext::new) mints a
//! random request id and stamps the arrival time from the system clock, so two
//! contexts are never equal and timestamps drift run-to-run. These helpers
//! produce a context with a **fixed, seeded** request id and a **pinned**
//! `received_at`, giving reproducible fixtures while still letting you set the
//! fields a test cares about (tenant, locale, …).

use klauthed_core::context::{RequestContext, RequestId};
use klauthed_core::time::Timestamp;

use crate::ids::seeded_id;

/// The default seed for a test context's request id.
const DEFAULT_REQUEST_SEED: u64 = 1;

/// The default arrival instant for a test context (`1_700_000_000_000` ms ≈
/// 2023-11-14T22:13:20Z), chosen as a stable, recognizable point in time.
const DEFAULT_RECEIVED_AT_MILLIS: i64 = 1_700_000_000_000;

/// A deterministic [`RequestContext`] with a seeded request id and a pinned
/// `received_at`.
///
/// Equivalent to `TestContextBuilder::new().build()`. Use the builder when you
/// need to set tenant, locale, or other fields.
///
/// ```
/// use klauthed_testing::context::test_context;
///
/// let a = test_context();
/// let b = test_context();
/// // Reproducible: same request id and arrival time every time.
/// assert_eq!(a.request_id(), b.request_id());
/// assert_eq!(a.received_at(), b.received_at());
/// ```
pub fn test_context() -> RequestContext {
    TestContextBuilder::new().build()
}

/// Builds a deterministic [`RequestContext`] for tests.
///
/// Starts from a seeded request id and a pinned arrival time, then layers on the
/// optional fields a test sets. Construct via [`TestContextBuilder::new`].
///
/// ```
/// use klauthed_testing::context::TestContextBuilder;
///
/// let ctx = TestContextBuilder::new()
///     .seed(42)
///     .tenant("acme")
///     .locale("tr-TR")
///     .correlation_id("trace-1")
///     .metadata("feature_flag", "beta")
///     .build();
///
/// assert_eq!(ctx.tenant(), Some("acme"));
/// assert_eq!(ctx.locale(), Some("tr-TR"));
/// assert_eq!(ctx.correlation_id(), Some("trace-1"));
/// assert_eq!(ctx.metadata_get("feature_flag"), Some("beta"));
/// ```
#[derive(Debug, Clone)]
pub struct TestContextBuilder {
    seed: u64,
    received_at: Timestamp,
    tenant: Option<String>,
    principal: Option<String>,
    locale: Option<String>,
    correlation_id: Option<String>,
    deadline: Option<Timestamp>,
    metadata: Vec<(String, String)>,
}

impl TestContextBuilder {
    /// A builder with default seed and pinned arrival time, and no other fields.
    pub fn new() -> Self {
        Self {
            seed: DEFAULT_REQUEST_SEED,
            received_at: Timestamp::from_unix_millis(DEFAULT_RECEIVED_AT_MILLIS),
            tenant: None,
            principal: None,
            locale: None,
            correlation_id: None,
            deadline: None,
            metadata: Vec::new(),
        }
    }

    /// Set the seed used to derive the (deterministic) request id.
    #[must_use]
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Pin the arrival time (`received_at`).
    #[must_use]
    pub fn received_at(mut self, at: Timestamp) -> Self {
        self.received_at = at;
        self
    }

    /// Set an absolute deadline.
    #[must_use]
    pub fn deadline(mut self, deadline: Timestamp) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Set the tenant identifier.
    #[must_use]
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set the authenticated principal / subject.
    #[must_use]
    pub fn principal(mut self, principal: impl Into<String>) -> Self {
        self.principal = Some(principal.into());
        self
    }

    /// Set the preferred locale (BCP-47, e.g. `en-US`).
    #[must_use]
    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set the inbound correlation / trace id.
    #[must_use]
    pub fn correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Add a metadata entry.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }

    /// The seeded [`RequestId`] this builder will use.
    pub fn request_id(&self) -> RequestId {
        seeded_id(self.seed)
    }

    /// Build the [`RequestContext`].
    pub fn build(self) -> RequestContext {
        let mut ctx = RequestContext::new()
            .with_request_id(seeded_id(self.seed))
            .with_received_at(self.received_at);
        if let Some(tenant) = self.tenant {
            ctx = ctx.with_tenant(tenant);
        }
        if let Some(principal) = self.principal {
            ctx = ctx.with_principal(principal);
        }
        if let Some(locale) = self.locale {
            ctx = ctx.with_locale(locale);
        }
        if let Some(correlation_id) = self.correlation_id {
            ctx = ctx.with_correlation_id(correlation_id);
        }
        if let Some(deadline) = self.deadline {
            ctx = ctx.with_deadline(deadline);
        }
        for (key, value) in self.metadata {
            ctx = ctx.with_metadata(key, value);
        }
        ctx
    }
}

impl Default for TestContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::seeded_id;

    #[test]
    fn test_context_is_deterministic() {
        let a = test_context();
        let b = test_context();
        assert_eq!(a.request_id(), b.request_id());
        assert_eq!(a.received_at(), b.received_at());
        assert_eq!(a.request_id(), seeded_id(DEFAULT_REQUEST_SEED));
    }

    #[test]
    fn builder_sets_fields() {
        let ctx = TestContextBuilder::new()
            .seed(9)
            .tenant("acme")
            .principal("user-1")
            .locale("de-DE")
            .correlation_id("corr-9")
            .metadata("k", "v")
            .build();

        assert_eq!(ctx.request_id(), seeded_id::<_>(9));
        assert_eq!(ctx.tenant(), Some("acme"));
        assert_eq!(ctx.principal(), Some("user-1"));
        assert_eq!(ctx.locale(), Some("de-DE"));
        assert_eq!(ctx.correlation_id(), Some("corr-9"));
        assert_eq!(ctx.metadata_get("k"), Some("v"));
    }

    #[test]
    fn deadline_and_received_at_pinning() {
        let received = Timestamp::from_unix_millis(10_000);
        let deadline = Timestamp::from_unix_millis(15_000);
        let ctx = TestContextBuilder::new().received_at(received).deadline(deadline).build();
        assert_eq!(ctx.received_at(), received);
        assert_eq!(ctx.deadline(), Some(deadline));
    }

    #[test]
    fn request_id_accessor_matches_built_context() {
        let builder = TestContextBuilder::new().seed(123);
        let id = builder.request_id();
        assert_eq!(builder.build().request_id(), id);
    }
}
