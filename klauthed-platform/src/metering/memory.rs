//! The in-memory [`Meter`].

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::Meter;
use crate::tenancy::TenantId;

/// A thread-safe, in-memory [`Meter`] for single-process use and tests.
///
/// Not durable: usage lives only for the process lifetime.
#[derive(Default)]
pub struct InMemoryMeter {
    usage: Mutex<HashMap<(TenantId, String), u64>>,
}

impl InMemoryMeter {
    /// An empty meter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(TenantId, String), u64>> {
        self.usage.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl Meter for InMemoryMeter {
    async fn record(&self, tenant: TenantId, metric: &str, amount: u64) {
        *self.lock().entry((tenant, metric.to_owned())).or_insert(0) += amount;
    }

    async fn usage(&self, tenant: TenantId, metric: &str) -> u64 {
        self.lock().get(&(tenant, metric.to_owned())).copied().unwrap_or(0)
    }

    async fn reset(&self, tenant: TenantId, metric: &str) {
        self.lock().remove(&(tenant, metric.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn accumulates_usage_per_tenant_and_metric() {
        let meter = InMemoryMeter::new();
        let (a, b) = (TenantId::new(), TenantId::new());

        meter.record(a, "api_calls", 3).await;
        meter.record(a, "api_calls", 2).await;
        meter.record(b, "api_calls", 10).await;

        assert_eq!(meter.usage(a, "api_calls").await, 5);
        assert_eq!(meter.usage(b, "api_calls").await, 10, "tenants are isolated");
        assert_eq!(meter.usage(a, "storage_mb").await, 0, "unrecorded metric is zero");
    }

    #[tokio::test]
    async fn reset_zeroes_a_single_metric() {
        let meter = InMemoryMeter::new();
        let tenant = TenantId::new();

        meter.record(tenant, "api_calls", 5).await;
        meter.record(tenant, "storage_mb", 9).await;
        meter.reset(tenant, "api_calls").await;

        assert_eq!(meter.usage(tenant, "api_calls").await, 0);
        assert_eq!(meter.usage(tenant, "storage_mb").await, 9, "other metrics untouched");
    }
}
