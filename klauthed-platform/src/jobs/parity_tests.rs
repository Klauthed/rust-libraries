//! Parity tests: the in-memory and SQL [`JobQueue`] backends must produce
//! identical observable behavior across the job lifecycle, given the same clock.
//!
//! Both queues are driven through the same scripted sequence and their
//! [`EnqueuedJob`] outputs compared field-by-field (ignoring the id, which each
//! backend mints independently). This pins down the "same semantics" guarantee
//! the durable backends advertise. Runs on SQLite; the Redis backend has no
//! in-process equivalent and is covered by its own `#[ignore]` integration tests.

use std::sync::Arc;

use klauthed_core::time::{Clock, Duration, FixedClock};

use super::{EnqueuedJob, InMemoryJobQueue, JobQueue, JobStatus, SqlJobQueue};

/// Build a SQLite-backed `SqlJobQueue` sharing `clock`, with `max_attempts`.
async fn sqlite_queue(clock: Arc<FixedClock>, max_attempts: u32) -> SqlJobQueue {
    sqlx::any::install_default_drivers();
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");
    let queue = SqlJobQueue::new(pool, clock).with_max_attempts(max_attempts);
    queue.ensure_schema().await.expect("ensure schema");
    queue
}

/// Assert two jobs are equivalent on every observable field except the id.
#[track_caller]
fn assert_parity(mem: &EnqueuedJob, sql: &EnqueuedJob) {
    assert_eq!(mem.kind(), sql.kind(), "kind");
    assert_eq!(mem.status(), sql.status(), "status");
    assert_eq!(mem.attempts(), sql.attempts(), "attempts");
    assert_eq!(mem.max_attempts(), sql.max_attempts(), "max_attempts");
    assert_eq!(mem.payload(), sql.payload(), "payload");
    assert_eq!(mem.run_at(), sql.run_at(), "run_at");
    assert_eq!(mem.last_error(), sql.last_error(), "last_error");
}

#[tokio::test]
async fn backends_agree_across_the_retry_lifecycle() {
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let mem = InMemoryJobQueue::with_max_attempts(clock.clone(), 2);
    let sql = sqlite_queue(clock.clone(), 2).await;

    // Enqueue: both Queued, attempts 0, identical payload/kind/run_at.
    let payload = serde_json::json!({ "to": "a@b.com", "n": 1 });
    let m = mem.enqueue("email".into(), payload.clone()).await.unwrap();
    let s = sql.enqueue("email".into(), payload).await.unwrap();
    assert_parity(&m, &s);
    assert_eq!(m.status(), JobStatus::Queued);

    // Claim: both Running, attempts 1.
    let (md, sd) = (mem.dequeue_due(None).await.unwrap(), sql.dequeue_due(None).await.unwrap());
    assert_eq!(md.len(), 1);
    assert_eq!(sd.len(), 1);
    assert_parity(&md[0], &sd[0]);
    assert_eq!(md[0].status(), JobStatus::Running);
    assert_eq!(md[0].attempts(), 1);

    // Fail attempt 1 (< max 2): both re-queued with the same backoff, not yet due.
    mem.mark_failed(m.id(), "boom-1".into()).await.unwrap();
    sql.mark_failed(s.id(), "boom-1".into()).await.unwrap();
    assert!(mem.dequeue_due(None).await.unwrap().is_empty());
    assert!(sql.dequeue_due(None).await.unwrap().is_empty());

    // Past the 1s backoff: both due again, attempts 2, last_error preserved.
    clock.advance(Duration::seconds(2));
    let (md, sd) = (mem.dequeue_due(None).await.unwrap(), sql.dequeue_due(None).await.unwrap());
    assert_parity(&md[0], &sd[0]);
    assert_eq!(md[0].attempts(), 2);
    assert_eq!(md[0].last_error(), Some("boom-1"));

    // Fail attempt 2 (== max): both terminal Failed, never due again.
    mem.mark_failed(m.id(), "boom-2".into()).await.unwrap();
    sql.mark_failed(s.id(), "boom-2".into()).await.unwrap();
    clock.advance(Duration::seconds(3600));
    assert!(mem.dequeue_due(None).await.unwrap().is_empty());
    assert!(sql.dequeue_due(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn backends_agree_on_ordering_and_limit() {
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let mem = InMemoryJobQueue::new(clock.clone());
    let sql = sqlite_queue(clock.clone(), super::DEFAULT_MAX_ATTEMPTS).await;

    // Three jobs with increasing run_at, all already due.
    let base = clock.now();
    clock.set(base.checked_add(Duration::seconds(100)).unwrap());
    for (i, kind) in ["a", "b", "c"].iter().enumerate() {
        let at = base.checked_add(Duration::seconds(i64::try_from(i).unwrap())).unwrap();
        mem.schedule((*kind).into(), serde_json::json!(i), at).await.unwrap();
        sql.schedule((*kind).into(), serde_json::json!(i), at).await.unwrap();
    }

    // dequeue_due(Some(2)) returns the two oldest, in the same order, on both.
    let (md, sd) =
        (mem.dequeue_due(Some(2)).await.unwrap(), sql.dequeue_due(Some(2)).await.unwrap());
    assert_eq!(md.len(), 2);
    assert_eq!(sd.len(), 2);
    for (m, s) in md.iter().zip(sd.iter()) {
        assert_parity(m, s);
    }
    assert_eq!(md.iter().map(EnqueuedJob::kind).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(sd.iter().map(EnqueuedJob::kind).collect::<Vec<_>>(), ["a", "b"]);

    // The third remains and is returned next on both.
    let (md, sd) = (mem.dequeue_due(None).await.unwrap(), sql.dequeue_due(None).await.unwrap());
    assert_eq!(md.len(), 1);
    assert_eq!(sd.len(), 1);
    assert_parity(&md[0], &sd[0]);
    assert_eq!(md[0].kind(), "c");
}

#[tokio::test]
async fn backends_agree_on_stalled_recovery() {
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let mem = InMemoryJobQueue::new(clock.clone());
    let sql = sqlite_queue(clock.clone(), super::DEFAULT_MAX_ATTEMPTS).await;

    let m = mem.enqueue("k".into(), serde_json::json!(null)).await.unwrap();
    let s = sql.enqueue("k".into(), serde_json::json!(null)).await.unwrap();
    mem.dequeue_due(None).await.unwrap(); // -> Running
    sql.dequeue_due(None).await.unwrap();

    // Within the window: neither recovers.
    clock.advance(Duration::seconds(30));
    assert!(mem.dequeue_stalled(Duration::seconds(30)).await.unwrap().is_empty());
    assert!(sql.dequeue_stalled(Duration::seconds(30)).await.unwrap().is_empty());

    // Past the window: both recover to Queued with run_at = now.
    clock.advance(Duration::seconds(1));
    let mr = mem.dequeue_stalled(Duration::seconds(30)).await.unwrap();
    let sr = sql.dequeue_stalled(Duration::seconds(30)).await.unwrap();
    assert_eq!(mr.len(), 1);
    assert_eq!(sr.len(), 1);
    assert_parity(&mr[0], &sr[0]);
    assert_eq!(mr[0].status(), JobStatus::Queued);
    assert_eq!(mr[0].run_at(), clock.now());
    assert_eq!(sr[0].run_at(), clock.now());

    let _ = (m.id(), s.id());
}
