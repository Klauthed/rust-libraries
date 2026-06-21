//! An in-process publish/subscribe event bus.
//!
//! Decouples "something happened" from the code that reacts to it: a publisher
//! calls [`EventBus::publish`] without knowing the subscribers, and every
//! registered [`EventHandler`] receives the event. Useful for in-process domain
//! event fan-out (e.g. `user.registered` → send welcome email + write audit).
//!
//! ```
//! # async fn run() {
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use klauthed_data::{EventBus, EventHandler, InMemoryEventBus};
//!
//! struct Welcome;
//! #[async_trait]
//! impl EventHandler<String> for Welcome {
//!     async fn handle(&self, user: &String) { let _ = user; /* send email */ }
//! }
//!
//! let bus = InMemoryEventBus::new().subscribe(Arc::new(Welcome));
//! bus.publish(&"alice".to_string()).await;
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;

/// Reacts to events of type `E` delivered by an [`EventBus`].
#[async_trait]
pub trait EventHandler<E>: Send + Sync {
    /// Handle one published event. Fire-and-forget: report failures internally.
    async fn handle(&self, event: &E);
}

/// Publishes events of type `E` to subscribed [`EventHandler`]s.
#[async_trait]
pub trait EventBus<E: Send + Sync>: Send + Sync {
    /// Deliver `event` to every subscribed handler.
    async fn publish(&self, event: &E);
}

/// An in-process [`EventBus`]: every published event is delivered to all
/// subscribed handlers, in subscription order.
pub struct InMemoryEventBus<E> {
    handlers: Vec<Arc<dyn EventHandler<E>>>,
}

impl<E> InMemoryEventBus<E> {
    /// A bus with no subscribers.
    #[must_use]
    pub fn new() -> Self {
        Self { handlers: Vec::new() }
    }

    /// Register `handler` to receive every published event.
    #[must_use]
    pub fn subscribe(mut self, handler: Arc<dyn EventHandler<E>>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// The number of subscribed handlers.
    #[must_use]
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

impl<E> Default for InMemoryEventBus<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<E: Send + Sync> EventBus<E> for InMemoryEventBus<E> {
    async fn publish(&self, event: &E) {
        for handler in &self.handlers {
            handler.handle(event).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        seen: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl EventHandler<String> for Recorder {
        async fn handle(&self, event: &String) {
            self.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(event.clone());
        }
    }

    fn seen(recorder: &Recorder) -> Vec<String> {
        recorder.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    #[tokio::test]
    async fn delivers_every_event_to_all_handlers() {
        let (r1, r2) = (Arc::new(Recorder::default()), Arc::new(Recorder::default()));
        let bus = InMemoryEventBus::new().subscribe(r1.clone()).subscribe(r2.clone());
        assert_eq!(bus.handler_count(), 2);

        bus.publish(&"hello".to_string()).await;
        bus.publish(&"world".to_string()).await;

        assert_eq!(seen(&r1), ["hello", "world"]);
        assert_eq!(seen(&r2), ["hello", "world"]);
    }

    #[tokio::test]
    async fn publish_with_no_handlers_is_a_noop() {
        let bus: InMemoryEventBus<String> = InMemoryEventBus::new();
        bus.publish(&"ignored".to_string()).await;
        assert_eq!(bus.handler_count(), 0);
    }
}
