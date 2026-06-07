//! The query (read) side: [`Query`], [`QueryHandler`], and the type-routed
//! [`QueryBus`].

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use klauthed_error::DomainError;

use super::CqrsError;

/// A read request. Produces an [`Output`](Query::Output).
pub trait Query: Send + 'static {
    /// The query result type.
    type Output: Send + 'static;
}

/// Handles one [`Query`] type.
#[async_trait::async_trait]
pub trait QueryHandler<Q: Query>: Send + Sync {
    /// The error this handler may return.
    type Error: DomainError + Send + Sync + 'static;

    /// Answer the query.
    async fn handle(&self, query: Q) -> Result<Q::Output, Self::Error>;
}

#[async_trait::async_trait]
trait ErasedQueryHandler<Q: Query>: Send + Sync {
    async fn handle_erased(&self, query: Q) -> Result<Q::Output, CqrsError>;
}

#[async_trait::async_trait]
impl<Q: Query, H: QueryHandler<Q>> ErasedQueryHandler<Q> for H {
    async fn handle_erased(&self, query: Q) -> Result<Q::Output, CqrsError> {
        self.handle(query).await.map_err(CqrsError::handler)
    }
}

/// Routes a [`Query`] to its single registered [`QueryHandler`] by type.
#[derive(Default)]
pub struct QueryBus {
    handlers: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl QueryBus {
    /// An empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the handler for query type `Q` (replacing any prior one).
    pub fn register<Q, H>(&mut self, handler: H) -> &mut Self
    where
        Q: Query,
        H: QueryHandler<Q> + 'static,
    {
        let erased: Arc<dyn ErasedQueryHandler<Q>> = Arc::new(handler);
        self.handlers.insert(TypeId::of::<Q>(), Box::new(erased));
        self
    }

    /// Dispatch a query to its handler.
    pub async fn dispatch<Q: Query>(&self, query: Q) -> Result<Q::Output, CqrsError> {
        let handler = self
            .handlers
            .get(&TypeId::of::<Q>())
            .and_then(|h| h.downcast_ref::<Arc<dyn ErasedQueryHandler<Q>>>())
            .cloned()
            .ok_or_else(CqrsError::no_handler::<Q>)?;
        handler.handle_erased(query).await
    }
}
