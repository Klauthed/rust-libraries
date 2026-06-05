#![deny(unsafe_code)]

//! Command/Query/Event handling with optional in-process buses.
//!
//! The core is three pairs of traits — [`Command`]/[`CommandHandler`],
//! [`Query`]/[`QueryHandler`], [`Event`]/[`EventHandler`] — that you can use
//! directly (call a handler) for maximum clarity. On top of them sit
//! type-routed dispatchers — [`CommandBus`], [`QueryBus`], [`EventBus`] — that
//! look a handler up by the message type, so call sites depend on the bus rather
//! than on every concrete handler.
//!
//! Handlers are async and their errors must be [`DomainError`], so failures flow
//! through the shared error handling as [`CqrsError`].
//!
//! ```ignore
//! let mut commands = CommandBus::new();
//! commands.register::<CreateUser, _>(CreateUserHandler::new(repo));
//! let id = commands.dispatch(CreateUser { name: "Ada".into() }).await?;
//! ```

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;

use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

// ── Messages & handlers ───────────────────────────────────────────────────────

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

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors raised by the dispatch buses.
#[derive(Debug)]
pub enum CqrsError {
    /// No handler was registered for the dispatched message type.
    NoHandler {
        /// The message type that had no handler.
        message_type: &'static str,
    },
    /// A handler returned an error.
    Handler(Box<dyn DomainError + Send + Sync>),
}

impl CqrsError {
    fn no_handler<M: 'static>() -> Self {
        CqrsError::NoHandler {
            message_type: type_name::<M>(),
        }
    }

    /// Wrap a handler's [`DomainError`].
    pub fn handler<E: DomainError + Send + Sync + 'static>(error: E) -> Self {
        CqrsError::Handler(Box::new(error))
    }
}

impl std::fmt::Display for CqrsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CqrsError::NoHandler { message_type } => {
                write!(f, "no handler registered for '{message_type}'")
            }
            CqrsError::Handler(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CqrsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CqrsError::NoHandler { .. } => None,
            CqrsError::Handler(error) => Some(&**error),
        }
    }
}

impl DomainError for CqrsError {
    fn category(&self) -> ErrorCategory {
        match self {
            // A missing handler is a wiring bug, not a caller error.
            CqrsError::NoHandler { .. } => ErrorCategory::Internal,
            CqrsError::Handler(error) => error.category(),
        }
    }

    fn code(&self) -> ErrorCode {
        match self {
            CqrsError::NoHandler { .. } => ErrorCode::new("cqrs.no_handler"),
            CqrsError::Handler(error) => error.code(),
        }
    }
}

// ── Type-erased handlers (uniform error so the registry is homogeneous) ────────

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

// ── Buses ─────────────────────────────────────────────────────────────────────

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
        self.handlers
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Box::new(erased));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A minimal DomainError for handlers to return.
    #[derive(Debug)]
    struct DemoError(&'static str);
    impl std::fmt::Display for DemoError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for DemoError {}
    impl DomainError for DemoError {
        fn category(&self) -> ErrorCategory {
            ErrorCategory::Conflict
        }
        fn code(&self) -> ErrorCode {
            ErrorCode::new("demo.failed")
        }
    }

    // Command
    struct CreateUser {
        name: String,
    }
    impl Command for CreateUser {
        type Output = String;
    }
    struct CreateUserHandler;
    #[async_trait::async_trait]
    impl CommandHandler<CreateUser> for CreateUserHandler {
        type Error = DemoError;
        async fn handle(&self, command: CreateUser) -> Result<String, DemoError> {
            if command.name.is_empty() {
                return Err(DemoError("name required"));
            }
            Ok(format!("user:{}", command.name))
        }
    }

    // Query
    struct GetGreeting {
        who: String,
    }
    impl Query for GetGreeting {
        type Output = String;
    }
    struct GetGreetingHandler;
    #[async_trait::async_trait]
    impl QueryHandler<GetGreeting> for GetGreetingHandler {
        type Error = DemoError;
        async fn handle(&self, query: GetGreeting) -> Result<String, DemoError> {
            Ok(format!("hello, {}", query.who))
        }
    }

    // Event
    struct UserCreated {
        name: String,
    }
    impl Event for UserCreated {}
    struct CountingHandler(Arc<AtomicUsize>);
    #[async_trait::async_trait]
    impl EventHandler<UserCreated> for CountingHandler {
        type Error = DemoError;
        async fn handle(&self, event: &UserCreated) -> Result<(), DemoError> {
            // Read the payload so the handler genuinely consumes the event.
            if !event.name.is_empty() {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }
    struct FailingHandler;
    #[async_trait::async_trait]
    impl EventHandler<UserCreated> for FailingHandler {
        type Error = DemoError;
        async fn handle(&self, _event: &UserCreated) -> Result<(), DemoError> {
            Err(DemoError("boom"))
        }
    }

    #[tokio::test]
    async fn command_bus_dispatches_and_reports_handler_errors() {
        let mut bus = CommandBus::new();
        bus.register::<CreateUser, _>(CreateUserHandler);

        let out = bus
            .dispatch(CreateUser { name: "ada".into() })
            .await
            .unwrap();
        assert_eq!(out, "user:ada");

        let err = bus
            .dispatch(CreateUser { name: String::new() })
            .await
            .unwrap_err();
        assert_eq!(err.code().as_str(), "demo.failed");
        assert_eq!(err.category(), ErrorCategory::Conflict);
    }

    #[tokio::test]
    async fn command_bus_reports_missing_handler() {
        let bus = CommandBus::new();
        let err = bus
            .dispatch(CreateUser { name: "x".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, CqrsError::NoHandler { .. }));
        assert_eq!(err.code().as_str(), "cqrs.no_handler");
        assert_eq!(err.category(), ErrorCategory::Internal);
    }

    #[tokio::test]
    async fn query_bus_dispatches() {
        let mut bus = QueryBus::new();
        bus.register::<GetGreeting, _>(GetGreetingHandler);
        let out = bus
            .dispatch(GetGreeting { who: "bob".into() })
            .await
            .unwrap();
        assert_eq!(out, "hello, bob");
    }

    #[tokio::test]
    async fn event_bus_fans_out_and_runs_all_handlers_despite_failure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut bus = EventBus::new();
        bus.subscribe::<UserCreated, _>(CountingHandler(counter.clone()));
        bus.subscribe::<UserCreated, _>(FailingHandler);
        bus.subscribe::<UserCreated, _>(CountingHandler(counter.clone()));

        let result = bus.publish(&UserCreated { name: "z".into() }).await;

        // The failing handler surfaces an error...
        assert!(result.is_err());
        // ...but both counting handlers still ran.
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn publishing_event_with_no_subscribers_is_ok() {
        let bus = EventBus::new();
        assert!(bus.publish(&UserCreated { name: "z".into() }).await.is_ok());
    }
}
