//! Public-API integration tests for the CQRS traits and buses.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use klauthed_core::cqrs::{
    Command, CommandBus, CommandHandler, CqrsError, Event, EventBus, EventHandler, Query, QueryBus,
    QueryHandler,
};
use klauthed_error::{DomainError, ErrorCategory, ErrorCode};

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

    let out = bus.dispatch(CreateUser { name: "ada".into() }).await.unwrap();
    assert_eq!(out, "user:ada");

    let err = bus.dispatch(CreateUser { name: String::new() }).await.unwrap_err();
    assert_eq!(err.code().as_str(), "demo.failed");
    assert_eq!(err.category(), ErrorCategory::Conflict);
}

#[tokio::test]
async fn command_bus_reports_missing_handler() {
    let bus = CommandBus::new();
    let err = bus.dispatch(CreateUser { name: "x".into() }).await.unwrap_err();
    assert!(matches!(err, CqrsError::NoHandler { .. }));
    assert_eq!(err.code().as_str(), "cqrs.no_handler");
    assert_eq!(err.category(), ErrorCategory::Internal);
}

#[tokio::test]
async fn query_bus_dispatches() {
    let mut bus = QueryBus::new();
    bus.register::<GetGreeting, _>(GetGreetingHandler);
    let out = bus.dispatch(GetGreeting { who: "bob".into() }).await.unwrap();
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
