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
//! Handlers are async and their errors must be [`DomainError`](klauthed_error::DomainError), so failures flow
//! through the shared error handling as [`CqrsError`].
//!
//! ```ignore
//! let mut commands = CommandBus::new();
//! commands.register::<CreateUser, _>(CreateUserHandler::new(repo));
//! let id = commands.dispatch(CreateUser { name: "Ada".into() }).await?;
//! ```

pub mod command;
pub mod error;
pub mod event;
pub mod query;

pub use command::{Command, CommandBus, CommandHandler};
pub use error::CqrsError;
pub use event::{Event, EventBus, EventHandler};
pub use query::{Query, QueryBus, QueryHandler};
