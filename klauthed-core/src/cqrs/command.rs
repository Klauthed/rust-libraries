//! The command (write) side: [`Command`], [`CommandHandler`], and the
//! type-routed [`CommandBus`].

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use klauthed_error::DomainError;

use super::CqrsError;

/// A request to change state. Produces an [`Output`](Command::Output) on success.
pub trait Command: Send + 'static {
    /// What handling this command returns (e.g. the new aggregate's id, or `()`).
    type Output: Send + 'static;
}

/// Handles one [`Command`] type.
#[async_trait::async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    /// The error this handler may return.
    type Error: DomainError + Send + Sync + 'static;

    /// Execute the command.
    async fn handle(&self, command: C) -> Result<C::Output, Self::Error>;
}

#[async_trait::async_trait]
trait ErasedCommandHandler<C: Command>: Send + Sync {
    async fn handle_erased(&self, command: C) -> Result<C::Output, CqrsError>;
}

#[async_trait::async_trait]
impl<C: Command, H: CommandHandler<C>> ErasedCommandHandler<C> for H {
    async fn handle_erased(&self, command: C) -> Result<C::Output, CqrsError> {
        self.handle(command).await.map_err(CqrsError::handler)
    }
}

/// Routes a [`Command`] to its single registered [`CommandHandler`] by type.
#[derive(Default)]
pub struct CommandBus {
    handlers: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl CommandBus {
    /// An empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the handler for command type `C` (replacing any prior one).
    pub fn register<C, H>(&mut self, handler: H) -> &mut Self
    where
        C: Command,
        H: CommandHandler<C> + 'static,
    {
        let erased: Arc<dyn ErasedCommandHandler<C>> = Arc::new(handler);
        self.handlers.insert(TypeId::of::<C>(), Box::new(erased));
        self
    }

    /// Dispatch a command to its handler.
    pub async fn dispatch<C: Command>(&self, command: C) -> Result<C::Output, CqrsError> {
        let handler = self
            .handlers
            .get(&TypeId::of::<C>())
            .and_then(|h| h.downcast_ref::<Arc<dyn ErasedCommandHandler<C>>>())
            .cloned()
            .ok_or_else(CqrsError::no_handler::<C>)?;
        handler.handle_erased(command).await
    }
}
