# Reliability & background jobs

Production services have to cope with flaky dependencies, work that must happen
*after* the response is sent, and workflows that span several resources without a
single transaction. klauthed ships composable building blocks for each. This page
walks through them; the
[reference service](https://github.com/Klauthed/rust-libraries/blob/master/reference-service/src/main.rs)
wires the background-jobs pieces together end to end.

## Retrying flaky calls — `RetryPolicy`

`klauthed-web`'s `RetryPolicy` retries an async operation with capped exponential
backoff. The operation is an `AsyncFnMut`, so it can be retried in place.

```rust,ignore
use std::time::Duration;
use klauthed_web::RetryPolicy;

let policy = RetryPolicy::new()
    .max_attempts(5)
    .base_backoff(Duration::from_millis(50))
    .max_backoff(Duration::from_secs(5))
    .multiplier(2.0);

// Retries on `Err`, sleeping base · multiplierⁿ (capped at max) between attempts.
let body = policy.retry(async || fetch(&url).await).await?;
```

## Failing fast — `CircuitBreaker`

A `CircuitBreaker` stops hammering a dependency that is already failing. After
`failure_threshold` consecutive failures it **opens** and rejects calls with
`CircuitError::Open` for the cooldown; the next call after the cooldown is a
**half-open** trial that closes the breaker on success or re-opens it on failure.
The `Clock` is injected, so the cooldown is deterministically testable with a
`FixedClock`.

```rust,ignore
use std::sync::Arc;
use std::time::Duration;
use klauthed_core::time::SystemClock;
use klauthed_web::{CircuitBreaker, CircuitError};

let breaker = CircuitBreaker::new(Arc::new(SystemClock), 5, Duration::from_secs(30));

match breaker.call(async || call_dependency().await).await {
    Ok(value) => { /* use it */ }
    Err(CircuitError::Open) => { /* fail fast: the dependency is unhealthy */ }
    Err(CircuitError::Inner(error)) => { /* the call ran and returned this error */ }
}
```

The two compose: wrap a `RetryPolicy::retry` call so each *attempt* goes through
the breaker, or guard a whole retry loop behind one breaker — pick based on
whether retries should count toward tripping it.

## Background jobs — queue, worker, scheduler

`klauthed-platform` separates *storing* work (`JobQueue`) from *running* it
(`JobWorker`). A handler processes one job; returning `Err(reason)` marks it failed
and the queue applies retry/backoff (or moves it to `Failed` once attempts run
out). Drive the worker periodically with the `scheduler` feature.

```rust,ignore
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use klauthed_core::time::SystemClock;
use klauthed_platform::{EnqueuedJob, InMemoryJobQueue, JobHandler, JobQueue, JobWorker};
use klauthed_platform::scheduler::Scheduler;

struct EmailHandler;

#[async_trait]
impl JobHandler for EmailHandler {
    async fn handle(&self, job: &EnqueuedJob) -> Result<(), String> {
        let to = job.payload().get("to").and_then(|v| v.as_str()).unwrap_or_default();
        send_email(to).await.map_err(|e| e.to_string())
    }
}

let queue = Arc::new(InMemoryJobQueue::new(Arc::new(SystemClock)));
let worker = Arc::new(JobWorker::new(queue.clone(), Arc::new(EmailHandler)));

// Enqueue from a request handler (anywhere holding the queue):
queue.enqueue("send_email".into(), serde_json::json!({ "to": "a@b.com" })).await;

// Drain the queue every 5s; the handle keeps the schedule alive.
let _handle = Scheduler::new()
    .every(Duration::from_secs(5), move || {
        let worker = Arc::clone(&worker);
        async move { let _ = worker.run_once().await; }
    })
    .start();
```

Swap `InMemoryJobQueue` for a durable backend — `SqlJobQueue` (feature `jobs-sql`;
SQLite / Postgres / MySQL, with a Postgres `FOR UPDATE SKIP LOCKED` claim for
concurrent workers) or `RedisJobQueue` (feature `jobs-redis`; atomic Lua claim) —
and the rest is unchanged. For
durable *event* publishing, the data **outbox** writes messages in the same
transaction as your state change and a polling **relay** drains them to a broker —
no lost or double-sent events.

## Cross-resource workflows — `Saga`

When a workflow spans resources that can't share one transaction (reserve stock,
charge a card, ship), a `klauthed-data` `Saga` pairs each forward step with a
compensation. If a step fails, the completed steps are compensated in reverse.

```rust,ignore
use klauthed_data::Saga;

Saga::new()
    .step(
        || async { reserve_stock().await },  // forward action -> Result<(), DataError>
        || async { release_stock().await },  // compensation, if a later step fails
    )
    .step(
        || async { charge_card().await },
        || async { refund_card().await },
    )
    .execute()
    .await?; // on failure: completed compensations run in reverse, then SagaError
```

For in-process fan-out (one event, many handlers) there's also a lightweight
**event bus** (`klauthed_data::EventBus`), and `SqlxTransact` / `MongoTransact`
run a closure inside a real database transaction, committing on `Ok` and rolling
back on `Err`.
