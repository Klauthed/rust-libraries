//! Public-API integration tests for the domain building blocks.

use klauthed_core::domain::*;
use klauthed_core::id::Id;
use klauthed_core::time::Timestamp;

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
        let mut account = Account { id: AccountId::new(), balance: 0, events: EventLog::new() };
        account.record(AccountEvent::Opened { owner: owner.to_owned() });
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
        self.store.lock().unwrap().insert(*aggregate.id().as_uuid(), aggregate.clone());
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
