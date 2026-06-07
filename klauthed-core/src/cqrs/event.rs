//! The event (pub/sub) side: [`Event`], [`EventHandler`], and the fan-out
//! [`EventBus`].

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use klauthed_error::DomainError;

use super::CqrsError;

/// A fact other parts of the system can react to (a marker for event payloads).
pub trait Event: Send + Sync + 'static {}

/// Reacts to one [`Event`] type. Several handlers may subscribe to the same event.
#[async_trait::async_trait]
pub trait EventHandler<E: Event>: Send + Sync {
    /// The error this handler may return.
    type Error: DomainError + Send + Sync + 'static;

    /// React to the event.
    async fn handle(&self, event: &E) -> Result<(), Self::Error>;
}

#[async_trait::async_trait]
trait ErasedEventHandler<E: Event>: Send + Sync {
    async fn handle_erased(&self, event: &E) -> Result<(), CqrsError>;
}

#[async_trait::async_trait]
impl<E: Event, H: EventHandler<E>> ErasedEventHandler<E> for H {
    async fn handle_erased(&self, event: &E) -> Result<(), CqrsError> {
        self.handle(event).await.map_err(CqrsError::handler)
    }
}

/// Fans an [`Event`] out to every subscribed [`EventHandler`] of its type.
#[derive(Default)]
pub struct EventBus {
    handlers: HashMap<TypeId, Vec<Box<dyn Any + Send + Sync>>>,
}

impl EventBus {
    /// An empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe a handler to event type `E` (handlers run in subscription order).
    pub fn subscribe<E, H>(&mut self, handler: H) -> &mut Self
    where
        E: Event,
        H: EventHandler<E> + 'static,
    {
        let erased: Arc<dyn ErasedEventHandler<E>> = Arc::new(handler);
        self.handlers.entry(TypeId::of::<E>()).or_default().push(Box::new(erased));
        self
    }

    /// Publish an event to all its handlers.
    ///
    /// Every handler runs even if an earlier one fails (best-effort fan-out);
    /// the first error encountered is returned, the rest are still executed.
    pub async fn publish<E: Event>(&self, event: &E) -> Result<(), CqrsError> {
        let Some(entries) = self.handlers.get(&TypeId::of::<E>()) else {
            return Ok(());
        };

        let handlers: Vec<Arc<dyn ErasedEventHandler<E>>> = entries
            .iter()
            .filter_map(|h| h.downcast_ref::<Arc<dyn ErasedEventHandler<E>>>().cloned())
            .collect();

        let mut first_error = None;
        for handler in handlers {
            if let Err(error) = handler.handle_erased(event).await {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
