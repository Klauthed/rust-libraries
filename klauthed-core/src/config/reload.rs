//! Hot-reloading configuration (`hot-reload` feature).
//!
//! [`ReloadableConfig`] resolves a [`ConfigBuilder`]'s provider chain once, then
//! re-resolves it on an interval (and on demand). When the resolved tree
//! changes it swaps in the new [`Config`] atomically and notifies subscribers —
//! so a value edited in a file or a central config server takes effect without
//! a restart. Secrets policy is unchanged: the same providers, re-run.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::{Config, ConfigBuilder};
use crate::error::ConfigError;

/// A [`Config`] kept fresh by periodically re-resolving its provider chain.
///
/// Reads are cheap [`Arc`] clones via [`current`](Self::current); changes are
/// observable through [`subscribe`](Self::subscribe). The background refresh
/// task is aborted when the value is dropped.
///
/// ```no_run
/// use std::time::Duration;
/// use klauthed_core::config::{ConfigBuilder, Profile, ReloadableConfig};
///
/// # async fn run() -> Result<(), klauthed_core::error::ConfigError> {
/// let builder = ConfigBuilder::new(Profile::detect());
/// let config = ReloadableConfig::start(builder, Duration::from_secs(30)).await?;
///
/// let snapshot = config.current(); // Arc<Config>, cheap to clone and hold
/// println!("profile: {:?}", snapshot.profile());
/// # Ok(())
/// # }
/// ```
pub struct ReloadableConfig {
    builder: Arc<ConfigBuilder>,
    tx: watch::Sender<Arc<Config>>,
    rx: watch::Receiver<Arc<Config>>,
    task: JoinHandle<()>,
}

impl ReloadableConfig {
    /// Resolve `builder` once, then spawn a task that re-resolves it every
    /// `interval`, swapping in and notifying subscribers whenever the resolved
    /// tree changes.
    ///
    /// The default provider chain is applied if `builder` has none (like
    /// [`ConfigBuilder::build`]). A failed re-resolve is logged and the current
    /// configuration is retained.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the initial resolve fails.
    pub async fn start(builder: ConfigBuilder, interval: Duration) -> Result<Self, ConfigError> {
        let builder = Arc::new(builder.ensure_defaults()?);
        let initial = Arc::new(builder.resolve().await?);
        let (tx, rx) = watch::channel(initial);

        let task = {
            let builder = Arc::clone(&builder);
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                ticker.tick().await; // the immediate first tick fires instantly; skip it
                loop {
                    ticker.tick().await;
                    match builder.resolve().await {
                        Ok(next) => swap_if_changed(&tx, next),
                        Err(error) => {
                            tracing::warn!(%error, "config reload failed; keeping current values");
                        }
                    }
                }
            })
        };

        Ok(Self { builder, tx, rx, task })
    }

    /// The current configuration — a cheap [`Arc`] clone safe to hold across an
    /// await or hand to another task.
    #[must_use]
    pub fn current(&self) -> Arc<Config> {
        self.rx.borrow().clone()
    }

    /// Subscribe to configuration swaps. The receiver holds the current value
    /// immediately and yields each new one via
    /// [`changed`](watch::Receiver::changed).
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<Arc<Config>> {
        self.rx.clone()
    }

    /// Re-resolve the provider chain now (in addition to the periodic refresh),
    /// swapping in and notifying if the tree changed.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the resolve fails; the current configuration is
    /// kept.
    pub async fn reload_now(&self) -> Result<(), ConfigError> {
        let next = self.builder.resolve().await?;
        swap_if_changed(&self.tx, next);
        Ok(())
    }

    /// Like [`start`](Self::start), but also returns a [`RefreshTrigger`] that
    /// **push-refreshes** on demand — re-resolve immediately when an external
    /// change signal arrives, instead of waiting for the next interval. Wire the
    /// trigger to a config-server webhook, a discovery / message-bus event, or an
    /// HTTP `/refresh` endpoint.
    ///
    /// The periodic refresh still runs as a safety net. When the last
    /// `RefreshTrigger` is dropped the task falls back to interval-only.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use klauthed_core::config::{ConfigBuilder, Profile, ReloadableConfig};
    ///
    /// # async fn run() -> Result<(), klauthed_core::error::ConfigError> {
    /// let builder = ConfigBuilder::new(Profile::detect());
    /// let (config, trigger) =
    ///     ReloadableConfig::start_with_refresh(builder, Duration::from_secs(300)).await?;
    ///
    /// // … hand `trigger` to an event source; on a "config changed" event:
    /// trigger.refresh();
    /// # let _ = config;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the initial resolve fails.
    pub async fn start_with_refresh(
        builder: ConfigBuilder,
        interval: Duration,
    ) -> Result<(Self, RefreshTrigger), ConfigError> {
        let builder = Arc::new(builder.ensure_defaults()?);
        let initial = Arc::new(builder.resolve().await?);
        let (tx, rx) = watch::channel(initial);
        // Capacity 1 ⇒ bursts of signals coalesce into at most one pending reload.
        let (trigger_tx, mut trigger_rx) = mpsc::channel::<()>(1);

        let task = {
            let builder = Arc::clone(&builder);
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                ticker.tick().await; // skip the immediate first tick
                let mut triggers_open = true;
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {}
                        signal = trigger_rx.recv(), if triggers_open => {
                            if signal.is_none() {
                                // All triggers dropped; keep the periodic refresh.
                                triggers_open = false;
                                continue;
                            }
                        }
                    }
                    match builder.resolve().await {
                        Ok(next) => swap_if_changed(&tx, next),
                        Err(error) => {
                            tracing::warn!(%error, "config reload failed; keeping current values");
                        }
                    }
                }
            })
        };

        Ok((Self { builder, tx, rx, task }, RefreshTrigger(trigger_tx)))
    }
}

/// A cheap, clonable handle that **push-refreshes** a [`ReloadableConfig`]
/// created with [`start_with_refresh`](ReloadableConfig::start_with_refresh).
///
/// Wire any change signal — a config-server webhook, a discovery / message-bus
/// event, an HTTP `/refresh` endpoint — to [`refresh`](Self::refresh) to
/// re-resolve the provider chain immediately.
#[derive(Clone)]
pub struct RefreshTrigger(mpsc::Sender<()>);

impl RefreshTrigger {
    /// Request a reload as soon as possible. Non-blocking and **coalescing**: if
    /// a refresh is already queued, extra signals are dropped — one reload covers
    /// them all.
    pub fn refresh(&self) {
        let _ = self.0.try_send(());
    }
}

impl Drop for ReloadableConfig {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Swap `next` in and notify subscribers only if its resolved tree differs from
/// the current one.
fn swap_if_changed(tx: &watch::Sender<Arc<Config>>, next: Config) {
    // Bind to a bool so the read borrow is released before `send` takes the
    // write lock (holding both would deadlock the watch channel).
    let changed = tx.borrow().values() != next.values();
    if changed {
        tracing::info!("configuration changed; swapping in new values");
        let _ = tx.send(Arc::new(next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;
    use crate::config::map::ConfigMap;
    use crate::config::provider::{ConfigProvider, MemoryProvider, ProviderKind};
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A provider that returns an incrementing `version` on each load, so every
    /// resolve produces a different tree.
    #[derive(Clone, Default)]
    struct CountingProvider {
        counter: Arc<AtomicU64>,
    }

    #[async_trait]
    impl ConfigProvider for CountingProvider {
        fn name(&self) -> String {
            "counting".to_owned()
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Memory
        }
        async fn load(&self) -> Result<ConfigMap, ConfigError> {
            let version = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(ConfigMap::from_iter([("version".to_string(), json!(version))]))
        }
    }

    #[tokio::test]
    async fn reload_now_swaps_in_changed_values_and_notifies() {
        let builder = ConfigBuilder::new(Profile::Test).with_provider(CountingProvider::default());
        // A long interval so only the explicit reload triggers a change here.
        let config = ReloadableConfig::start(builder, Duration::from_secs(3600)).await.unwrap();

        assert_eq!(config.current().get_raw("version"), Some(&json!(0)));

        let sub = config.subscribe();
        config.reload_now().await.unwrap();

        assert_eq!(config.current().get_raw("version"), Some(&json!(1)));
        assert!(sub.has_changed().unwrap(), "subscriber should see the swap");
    }

    #[tokio::test]
    async fn reload_with_unchanged_values_does_not_notify() {
        let builder =
            ConfigBuilder::new(Profile::Test).with_provider(MemoryProvider::new().set("x", 1));
        let config = ReloadableConfig::start(builder, Duration::from_secs(3600)).await.unwrap();

        let sub = config.subscribe();
        config.reload_now().await.unwrap(); // re-resolves to identical values

        assert!(!sub.has_changed().unwrap(), "no change → no notification");
    }

    #[tokio::test]
    async fn periodic_refresh_picks_up_changes() {
        let builder = ConfigBuilder::new(Profile::Test).with_provider(CountingProvider::default());
        let config = ReloadableConfig::start(builder, Duration::from_millis(20)).await.unwrap();

        let mut sub = config.subscribe();
        // The background task should re-resolve and swap within a few intervals.
        tokio::time::timeout(Duration::from_secs(2), sub.changed()).await.unwrap().unwrap();
        assert!(config.current().get_raw("version").unwrap().as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn refresh_trigger_forces_a_reload() {
        let builder = ConfigBuilder::new(Profile::Test).with_provider(CountingProvider::default());
        // A long interval so only the explicit push-refresh causes a change.
        let (config, trigger) =
            ReloadableConfig::start_with_refresh(builder, Duration::from_secs(3600)).await.unwrap();
        assert_eq!(config.current().get_raw("version"), Some(&json!(0)));

        let mut sub = config.subscribe();
        trigger.refresh();

        tokio::time::timeout(Duration::from_secs(2), sub.changed()).await.unwrap().unwrap();
        assert_eq!(config.current().get_raw("version"), Some(&json!(1)));
    }

    #[tokio::test]
    async fn periodic_refresh_survives_dropping_the_trigger() {
        let builder = ConfigBuilder::new(Profile::Test).with_provider(CountingProvider::default());
        let (config, trigger) =
            ReloadableConfig::start_with_refresh(builder, Duration::from_millis(20)).await.unwrap();
        // Dropping the only trigger must fall back to interval-only, not kill the task.
        drop(trigger);

        let mut sub = config.subscribe();
        tokio::time::timeout(Duration::from_secs(2), sub.changed()).await.unwrap().unwrap();
        assert!(config.current().get_raw("version").unwrap().as_u64().unwrap() >= 1);
    }
}
