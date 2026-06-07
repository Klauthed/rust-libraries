#![deny(unsafe_code)]

//! Domain modeling primitives (DDD).
//!
//! These traits encode the building blocks of a domain model without dictating
//! persistence. The aggregate style is **state-based with event emission**: an
//! [`AggregateRoot`] mutates its own state *and* records [`DomainEvent`]s, which
//! are later drained as [`EventEnvelope`]s for publishing (outbox, integration).
//! Storage holds current state — this is not event sourcing, though it doesn't
//! preclude it.
//!
//! * [`Entity`] — has identity (a typed [`Id`](crate::id::Id)); equality is by id.
//! * [`ValueObject`] — immutable, compared by value.
//! * [`DomainEvent`] / [`EventEnvelope`] — facts that happened, plus transport metadata.
//! * [`EventLog`] — the recorder an aggregate embeds to track pending events + version.
//! * [`AggregateRoot`] — consistency boundary that records events.
//! * [`Repository`] — load/save aggregates (implemented by the data layer).

pub mod aggregate;
pub mod entity;
pub mod event;

pub use aggregate::{AggregateRoot, Repository};
pub use entity::{Entity, ValueObject};
pub use event::{DomainEvent, EventEnvelope, EventId, EventLog, EventTag};
