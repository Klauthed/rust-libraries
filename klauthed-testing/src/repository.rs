//! A generic in-memory [`Repository`] for tests.
//!
//! [`InMemoryRepository`] is a thread-safe, dependency-free stand-in for the real
//! data-layer repositories, so unit tests can exercise aggregate behavior without
//! a database. It stores cloned aggregate state keyed by id, captures the events
//! drained on each [`save`](Repository::save) (mirroring how a real outbox-backed
//! repository consumes them), and exposes inspection helpers
//! ([`len`](InMemoryRepository::len), [`contains`](InMemoryRepository::contains),
//! [`drained_events`](InMemoryRepository::drained_events)).
//!
//! The aggregate's id must be usable as a map key: `A::Id: Eq + Hash + Clone`.
//! [`klauthed_core::id::Id`] satisfies this for any marker type.
//!
//! Future work (intentionally out of scope here): fault injection (forced save
//! errors), optimistic-concurrency conflict simulation by version.
//!
//! ```
//! # use klauthed_core::domain::{AggregateRoot, DomainEvent, Entity, EventLog, Repository};
//! # use klauthed_core::id::Id;
//! # use klauthed_testing::repository::InMemoryRepository;
//! # struct AccountTag;
//! # type AccountId = Id<AccountTag>;
//! # #[derive(Debug, Clone, PartialEq, Eq)]
//! # enum AccountEvent { Opened }
//! # impl DomainEvent for AccountEvent {
//! #     fn event_type(&self) -> &'static str { "account.opened" }
//! # }
//! # #[derive(Debug, Clone)]
//! # struct Account { id: AccountId, events: EventLog<AccountEvent> }
//! # impl Entity for Account { type Id = AccountId; fn id(&self) -> &AccountId { &self.id } }
//! # impl AggregateRoot for Account {
//! #     type Event = AccountEvent;
//! #     fn aggregate_type() -> &'static str { "account" }
//! #     fn event_log(&self) -> &EventLog<AccountEvent> { &self.events }
//! #     fn event_log_mut(&mut self) -> &mut EventLog<AccountEvent> { &mut self.events }
//! # }
//! # async fn example() {
//! let repo = InMemoryRepository::<Account>::new();
//! let mut account = Account { id: AccountId::new(), events: EventLog::new() };
//! account.record(AccountEvent::Opened);
//! let id = *account.id();
//!
//! repo.save(&mut account).await.unwrap();
//! assert_eq!(repo.len(), 1);
//! assert!(repo.contains(&id));
//! assert_eq!(repo.drained_events(), vec![AccountEvent::Opened]);
//! # }
//! ```

use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::sync::Mutex;

use async_trait::async_trait;

use klauthed_core::domain::{AggregateRoot, Repository};

/// A thread-safe, in-memory [`Repository`] implementation for tests.
///
/// Keys aggregates by their id (`A::Id: Eq + Hash + Clone`) and stores cloned
/// state. On [`save`](Repository::save) it drains the aggregate's pending events
/// (as a real repository would, e.g. into an outbox) and records them for later
/// inspection via [`drained_events`](InMemoryRepository::drained_events).
///
/// Its [`Error`](Repository::Error) is [`Infallible`]: nothing here can fail.
pub struct InMemoryRepository<A>
where
    A: AggregateRoot,
{
    inner: Mutex<Inner<A>>,
}

struct Inner<A>
where
    A: AggregateRoot,
{
    store: HashMap<A::Id, A>,
    drained: Vec<A::Event>,
}

impl<A> InMemoryRepository<A>
where
    A: AggregateRoot,
    A::Id: Eq + Hash + Clone,
{
    /// An empty repository.
    pub fn new() -> Self {
        Self { inner: Mutex::new(Inner { store: HashMap::new(), drained: Vec::new() }) }
    }

    /// The number of stored aggregates.
    pub fn len(&self) -> usize {
        self.lock().store.len()
    }

    /// Whether the repository holds no aggregates.
    pub fn is_empty(&self) -> bool {
        self.lock().store.is_empty()
    }

    /// Whether an aggregate with `id` is stored.
    pub fn contains(&self, id: &A::Id) -> bool {
        self.lock().store.contains_key(id)
    }

    /// Insert (or replace) an aggregate's state directly, without draining its
    /// events — a convenience for seeding fixtures.
    pub fn insert(&self, aggregate: A) {
        let mut inner = self.lock();
        inner.store.insert(aggregate.id().clone(), aggregate);
    }

    /// Remove and return all events captured from `save` calls so far.
    ///
    /// Each [`save`](Repository::save) appends the aggregate's drained events
    /// here, preserving order across saves. Calling this clears the buffer.
    pub fn drained_events(&self) -> Vec<A::Event> {
        std::mem::take(&mut self.lock().drained)
    }

    /// The number of events captured from `save` calls (without draining them).
    pub fn drained_event_count(&self) -> usize {
        self.lock().drained.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<A>> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<A> Default for InMemoryRepository<A>
where
    A: AggregateRoot,
    A::Id: Eq + Hash + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<A> Repository<A> for InMemoryRepository<A>
where
    A: AggregateRoot + Clone + Send + Sync,
    A::Id: Eq + Hash + Clone + Send + Sync,
    A::Event: Send,
{
    type Error = Infallible;

    async fn find(&self, id: &A::Id) -> Result<Option<A>, Infallible> {
        Ok(self.lock().store.get(id).cloned())
    }

    async fn save(&self, aggregate: &mut A) -> Result<(), Infallible> {
        // Mirror a real repository: drain pending events (would go to an outbox)
        // and persist the current state.
        let events = aggregate.take_events();
        let key = aggregate.id().clone();
        let mut inner = self.lock();
        inner.drained.extend(events);
        inner.store.insert(key, aggregate.clone());
        Ok(())
    }

    async fn delete(&self, id: &A::Id) -> Result<(), Infallible> {
        self.lock().store.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klauthed_core::domain::{DomainEvent, Entity, EventLog};
    use klauthed_core::id::Id;

    use crate::ids::seeded_id;

    struct AccountTag;
    type AccountId = Id<AccountTag>;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AccountEvent {
        Opened { owner: String },
        Deposited { amount: i64 },
    }

    impl DomainEvent for AccountEvent {
        fn event_type(&self) -> &'static str {
            match self {
                AccountEvent::Opened { .. } => "account.opened",
                AccountEvent::Deposited { .. } => "account.deposited",
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
        fn open(id: AccountId, owner: &str) -> Self {
            let mut account = Account { id, balance: 0, events: EventLog::new() };
            account.record(AccountEvent::Opened { owner: owner.to_owned() });
            account
        }
        fn deposit(&mut self, amount: i64) {
            self.balance += amount;
            self.record(AccountEvent::Deposited { amount });
        }
    }

    #[tokio::test]
    async fn save_find_delete_round_trip() {
        let repo = InMemoryRepository::<Account>::new();
        assert!(repo.is_empty());

        let id = seeded_id::<AccountTag>(1);
        let mut account = Account::open(id, "alice");
        account.deposit(25);

        repo.save(&mut account).await.unwrap();
        assert_eq!(repo.len(), 1);
        assert!(repo.contains(&id));

        let loaded = repo.find(&id).await.unwrap().expect("present");
        assert_eq!(loaded.balance, 25);
        // Events were drained on save, so loaded state has none pending.
        assert!(loaded.pending_events().is_empty());

        repo.delete(&id).await.unwrap();
        assert!(!repo.contains(&id));
        assert!(repo.find(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn captures_drained_events_in_order() {
        let repo = InMemoryRepository::<Account>::new();
        let mut a = Account::open(seeded_id::<AccountTag>(1), "alice");
        a.deposit(10);
        let mut b = Account::open(seeded_id::<AccountTag>(2), "bob");

        repo.save(&mut a).await.unwrap();
        repo.save(&mut b).await.unwrap();

        assert_eq!(repo.drained_event_count(), 3);
        let events = repo.drained_events();
        assert_eq!(
            events,
            vec![
                AccountEvent::Opened { owner: "alice".into() },
                AccountEvent::Deposited { amount: 10 },
                AccountEvent::Opened { owner: "bob".into() },
            ]
        );
        // Draining clears the buffer.
        assert_eq!(repo.drained_event_count(), 0);
    }

    #[tokio::test]
    async fn insert_seeds_without_draining() {
        let repo = InMemoryRepository::<Account>::new();
        let account = Account::open(seeded_id::<AccountTag>(7), "carol");
        repo.insert(account);
        assert_eq!(repo.len(), 1);
        // insert does not touch the drained-events buffer.
        assert_eq!(repo.drained_event_count(), 0);
    }

    #[tokio::test]
    async fn usable_as_dyn_repository() {
        let repo: Box<dyn Repository<Account, Error = Infallible>> =
            Box::new(InMemoryRepository::<Account>::new());
        let id = seeded_id::<AccountTag>(3);
        let mut account = Account::open(id, "dave");
        repo.save(&mut account).await.unwrap();
        assert!(repo.find(&id).await.unwrap().is_some());
    }
}
