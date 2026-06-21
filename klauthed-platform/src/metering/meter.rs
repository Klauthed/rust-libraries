//! The [`Meter`] usage-metering trait.

use async_trait::async_trait;

use crate::tenancy::TenantId;

/// Records and reports per-tenant usage of named metrics — for quotas,
/// usage-based billing, or rate accounting. Amounts accumulate per
/// `(tenant, metric)` until [`reset`](Meter::reset) (e.g. at the end of a billing
/// period).
///
/// Object-safe, so a meter can be shared as `Arc<dyn Meter>`. The in-memory
/// implementation is [`InMemoryMeter`](super::InMemoryMeter); a real deployment
/// might back this with a counter store or time-series database.
#[async_trait]
pub trait Meter: Send + Sync {
    /// Add `amount` units of `metric` to `tenant`'s usage.
    async fn record(&self, tenant: TenantId, metric: &str, amount: u64);

    /// The accumulated usage of `metric` for `tenant` (0 if nothing recorded).
    async fn usage(&self, tenant: TenantId, metric: &str) -> u64;

    /// Reset `tenant`'s usage of `metric` to zero.
    async fn reset(&self, tenant: TenantId, metric: &str);
}
