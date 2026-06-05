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
//! * [`Entity`] — has identity (a typed [`Id`]); equality is by id.
//! * [`ValueObject`] — immutable, compared by value.
//! * [`DomainEvent`] / [`EventEnvelope`] — facts that happened, plus transport metadata.
//! * [`EventLog`] — the recorder an aggregate embeds to track pending events + version.
//! * [`AggregateRoot`] — consistency boundary that records events.
//! * [`Repository`] — load/save aggregates (implemented by the data layer).

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::time::Timestamp;

// ── Entity & value object ─────────────────────────────────────────────────────

/// A domain object with a stable identity. Two entities are "the same" when
/// their ids are equal, regardless of their other fields.
pub trait Entity {
    /// The identity type, typically an [`Id<T>`](crate::id::Id).
    type Id;

    /// This entity's identity.
    fn id(&self) -> &Self::Id;
}

/// A marker for immutable values compared structurally (no identity).
///
/// Implement it on types like `Money`, `EmailAddress`, `DateRange` to document
/// and enforce value semantics.
pub trait ValueObject: Clone + PartialEq {}

// ── Domain events ─────────────────────────────────────────────────────────────

/// Marker tag for an event identifier.
pub struct EventTag;

/// The id minted for each emitted event.
pub type EventId = Id<EventTag>;

/// A fact that happened in the domain.
pub trait DomainEvent {
    /// A stable, dotted event name, e.g. `account.opened`.
    fn event_type(&self) -> &'static str;

    /// Payload schema version, for evolving event shapes over time.
    fn schema_version(&self) -> u32 {
        1
    }
}

/// A [`DomainEvent`] wrapped with the metadata needed to transport and store it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<E> {
    /// Unique id of this event occurrence.
    pub event_id: EventId,
    /// The event's stable type name.
    pub event_type: Cow<'static, str>,
    /// The aggregate this event belongs to (its id rendered as a string).
    pub aggregate_id: String,
    /// The aggregate's type name.
    pub aggregate_type: Cow<'static, str>,
    /// The aggregate version this event produced (monotonic per aggregate).
    pub sequence: u64,
    /// When the event occurred.
    pub occurred_at: Timestamp,
    /// The event payload.
    pub payload: E,
}

impl<E: DomainEvent> EventEnvelope<E> {
    /// Wrap `payload` with transport metadata.
    pub fn new(
        aggregate_id: String,
        aggregate_type: &'static str,
        sequence: u64,
        occurred_at: Timestamp,
        payload: E,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            event_type: Cow::Borrowed(payload.event_type()),
            aggregate_id,
            aggregate_type: Cow::Borrowed(aggregate_type),
            sequence,
            occurred_at,
            payload,
        }
    }
}

// ── Event log (embedded recorder) ─────────────────────────────────────────────

/// Tracks an aggregate's uncommitted events and its version.
///
/// Aggregates embed an `EventLog<MyEvent>` and delegate the [`AggregateRoot`]
/// event methods to it, so they don't re-implement the bookkeeping.
#[derive(Debug, Clone)]
pub struct EventLog<E> {
    pending: Vec<E>,
    version: u64,
}

impl<E> EventLog<E> {
    /// A fresh log at version 0 (a brand-new aggregate).
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            version: 0,
        }
    }

    /// A log for an aggregate loaded at an existing `version`.
    pub fn with_version(version: u64) -> Self {
        Self {
            pending: Vec::new(),
            version,
        }
    }

    /// Record an event and advance the version.
    pub fn record(&mut self, event: E) {
        self.pending.push(event);
        self.version += 1;
    }

    /// Drain the pending events (the version is unchanged — it is persistent).
    pub fn take(&mut self) -> Vec<E> {
        std::mem::take(&mut self.pending)
    }

    /// The pending (uncommitted) events.
    pub fn pending(&self) -> &[E] {
        &self.pending
    }

    /// The current version (number of events ever recorded).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether there are no pending events.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// The number of pending events.
    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

impl<E> Default for EventLog<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Aggregate root ────────────────────────────────────────────────────────────

/// A consistency boundary that owns its invariants and records the events its
/// state changes produce.
///
/// Implementors supply [`aggregate_type`](AggregateRoot::aggregate_type) and the
/// two `event_log` accessors; everything else is provided.
pub trait AggregateRoot: Entity {
    /// The event type this aggregate emits.
    type Event: DomainEvent;

    /// A stable type name, e.g. `account`.
    fn aggregate_type() -> &'static str;

    /// The embedded event log (read).
    fn event_log(&self) -> &EventLog<Self::Event>;

    /// The embedded event log (mutate).
    fn event_log_mut(&mut self) -> &mut EventLog<Self::Event>;

    /// The current version (an optimistic-lock token).
    fn version(&self) -> u64 {
        self.event_log().version()
    }

    /// Record a domain event the aggregate just produced.
    fn record(&mut self, event: Self::Event) {
        self.event_log_mut().record(event);
    }

    /// The uncommitted events, without draining them.
    fn pending_events(&self) -> &[Self::Event] {
        self.event_log().pending()
    }

    /// Drain the uncommitted events.
    fn take_events(&mut self) -> Vec<Self::Event> {
        self.event_log_mut().take()
    }

    /// Drain the uncommitted events as [`EventEnvelope`]s ready to publish,
    /// stamping each with its aggregate id/type, sequence, and `occurred_at`.
    fn drain_envelopes(&mut self, occurred_at: Timestamp) -> Vec<EventEnvelope<Self::Event>>
    where
        Self::Id: fmt::Display,
    {
        let aggregate_id = self.id().to_string();
        let aggregate_type = Self::aggregate_type();
        let events = self.take_events();
        // Sequences for the drained events end at the current version.
        let end = self.version();
        let start = end + 1 - events.len() as u64;
        events
            .into_iter()
            .enumerate()
            .map(|(offset, payload)| {
                EventEnvelope::new(
                    aggregate_id.clone(),
                    aggregate_type,
                    start + offset as u64,
                    occurred_at,
                    payload,
                )
            })
            .collect()
    }
}

// ── Repository ────────────────────────────────────────────────────────────────

/// Persists and retrieves aggregates by identity.
///
/// The data layer (e.g. `klauthed-data`) provides concrete implementations; the
/// domain only depends on this abstraction.
#[async_trait::async_trait]
pub trait Repository<A>: Send + Sync
where
    A: AggregateRoot + Send + Sync,
    A::Id: Send + Sync,
{
    /// The error type the implementation reports.
    type Error;

    /// Load an aggregate by id, or `None` if it does not exist.
    async fn find(&self, id: &A::Id) -> Result<Option<A>, Self::Error>;

    /// Persist an aggregate. Takes `&mut` so the implementation may drain its
    /// events (e.g. into an outbox) as part of the save.
    async fn save(&self, aggregate: &mut A) -> Result<(), Self::Error>;

    /// Delete an aggregate by id.
    async fn delete(&self, id: &A::Id) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::convert::Infallible;
    use std::sync::Mutex;

    struct AccountTag;
    type AccountId = Id<AccountTag>;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AccountEvent {
        Opened { owner: String },
        Deposited { amount: i64 },
        Withdrawn { amount: i64 },
    }

    impl DomainEvent for AccountEvent {
        fn event_type(&self) -> &'static str {
            match self {
                AccountEvent::Opened { .. } => "account.opened",
                AccountEvent::Deposited { .. } => "account.deposited",
                AccountEvent::Withdrawn { .. } => "account.withdrawn",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct Account {
        id: AccountId,
        balance: i64,
        events: EventLog<AccountEvent>,
    }

    impl Entity for Account {
        type Id = AccountId;
        fn id(&self) -> &AccountId {
            &self.id
        }
    }

    impl AggregateRoot for Account {
        type Event = AccountEvent;
        fn aggregate_type() -> &'static str {
            "account"
        }
        fn event_log(&self) -> &EventLog<AccountEvent> {
            &self.events
        }
        fn event_log_mut(&mut self) -> &mut EventLog<AccountEvent> {
            &mut self.events
        }
    }

    impl Account {
        fn open(owner: &str) -> Self {
            let mut account = Account {
                id: AccountId::new(),
                balance: 0,
                events: EventLog::new(),
            };
            account.record(AccountEvent::Opened {
                owner: owner.to_owned(),
            });
            account
        }
        fn deposit(&mut self, amount: i64) {
            self.balance += amount;
            self.record(AccountEvent::Deposited { amount });
        }
        fn withdraw(&mut self, amount: i64) {
            self.balance -= amount;
            self.record(AccountEvent::Withdrawn { amount });
        }
    }

    #[test]
    fn records_events_and_tracks_version() {
        let mut account = Account::open("alice");
        account.deposit(100);
        account.deposit(50);
        account.withdraw(30);

        assert_eq!(account.balance, 120);
        assert_eq!(account.version(), 4);
        assert_eq!(account.pending_events().len(), 4);
        assert_eq!(account.pending_events()[3].event_type(), "account.withdrawn");
    }

    #[test]
    fn drains_envelopes_with_sequences_and_metadata() {
        let mut account = Account::open("bob");
        account.deposit(10);
        let now = Timestamp::from_unix_millis(1_000);

        let envelopes = account.drain_envelopes(now);

        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].event_type, "account.opened");
        assert_eq!(envelopes[0].sequence, 1);
        assert_eq!(envelopes[1].event_type, "account.deposited");
        assert_eq!(envelopes[1].sequence, 2);
        assert_eq!(envelopes[0].aggregate_type, "account");
        assert_eq!(envelopes[0].aggregate_id, account.id().to_string());
        // Draining clears pending but keeps the version.
        assert!(account.pending_events().is_empty());
        assert_eq!(account.version(), 2);
    }

    // ── In-memory Repository implementation, to prove the trait is usable ──────

    #[derive(Default)]
    struct InMemoryAccounts {
        store: Mutex<HashMap<uuid::Uuid, Account>>,
    }

    #[async_trait::async_trait]
    impl Repository<Account> for InMemoryAccounts {
        type Error = Infallible;

        async fn find(&self, id: &AccountId) -> Result<Option<Account>, Infallible> {
            Ok(self.store.lock().unwrap().get(id.as_uuid()).cloned())
        }

        async fn save(&self, aggregate: &mut Account) -> Result<(), Infallible> {
            // A real impl would publish drained events here; we just persist state.
            let _events = aggregate.take_events();
            self.store
                .lock()
                .unwrap()
                .insert(*aggregate.id().as_uuid(), aggregate.clone());
            Ok(())
        }

        async fn delete(&self, id: &AccountId) -> Result<(), Infallible> {
            self.store.lock().unwrap().remove(id.as_uuid());
            Ok(())
        }
    }

    #[tokio::test]
    async fn repository_round_trip() {
        let repo = InMemoryAccounts::default();
        let mut account = Account::open("carol");
        account.deposit(25);
        let id = *account.id();

        repo.save(&mut account).await.unwrap();
        let loaded = repo.find(&id).await.unwrap().expect("account present");
        assert_eq!(loaded.balance, 25);
        assert_eq!(loaded.version(), 2);

        repo.delete(&id).await.unwrap();
        assert!(repo.find(&id).await.unwrap().is_none());
    }
}
