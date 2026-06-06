//! Tests for the job model and the in-memory queue.

use crate::error::PlatformError;
use klauthed_core::time::{Clock, Duration, Timestamp};
use std::sync::Arc;

use super::*;
use klauthed_core::time::FixedClock;

fn queue(max_attempts: u32) -> (Arc<FixedClock>, InMemoryJobQueue) {
    let clock = Arc::new(FixedClock::at_unix_millis(0));
    let q = InMemoryJobQueue::with_max_attempts(clock.clone(), max_attempts);
    (clock, q)
}

#[tokio::test]
async fn enqueue_then_dequeue_due_marks_running() {
    let (_clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!({"a": 1})).await;
    assert_eq!(job.status(), JobStatus::Queued);
    assert_eq!(job.attempts(), 0);

    let due = q.dequeue_due(None).await;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id(), job.id());
    assert_eq!(due[0].status(), JobStatus::Running);
    assert_eq!(due[0].attempts(), 1);

    // No longer queued, so a second poll returns nothing.
    assert!(q.dequeue_due(None).await.is_empty());
}

#[tokio::test]
async fn future_job_is_not_due_until_clock_advances() {
    let (clock, q) = queue(5);
    let now = clock.now();
    let run_at = now.checked_add(Duration::seconds(60)).unwrap();
    let job = q.schedule("k".into(), serde_json::json!(null), run_at).await;

    // Not due yet.
    assert!(q.dequeue_due(None).await.is_empty());
    assert_eq!(q.get(job.id()).unwrap().status(), JobStatus::Queued);

    // Advance past run_at.
    clock.advance(Duration::seconds(61));
    let due = q.dequeue_due(None).await;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id(), job.id());
}

#[tokio::test]
async fn dequeue_due_respects_limit_and_ordering() {
    let (clock, q) = queue(5);
    let base = clock.now();
    // Three jobs with increasing run_at, all already due.
    clock.set(base.checked_add(Duration::seconds(100)).unwrap());
    let a = q.schedule("k".into(), serde_json::json!("a"), base).await;
    let b = q
        .schedule(
            "k".into(),
            serde_json::json!("b"),
            base.checked_add(Duration::seconds(1)).unwrap(),
        )
        .await;
    let _c = q
        .schedule(
            "k".into(),
            serde_json::json!("c"),
            base.checked_add(Duration::seconds(2)).unwrap(),
        )
        .await;

    let due = q.dequeue_due(Some(2)).await;
    assert_eq!(due.len(), 2);
    assert_eq!(due[0].id(), a.id());
    assert_eq!(due[1].id(), b.id());
}

#[tokio::test]
async fn mark_failed_requeues_with_backoff_until_max_then_stays_failed() {
    let (clock, q) = queue(3);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;

    // Attempt 1.
    let due = q.dequeue_due(None).await;
    assert_eq!(due[0].attempts(), 1);
    q.mark_failed(job.id(), "boom-1".into()).await.unwrap();
    let after1 = q.get(job.id()).unwrap();
    assert_eq!(after1.status(), JobStatus::Queued);
    assert_eq!(after1.last_error(), Some("boom-1"));
    // Backoff after 1 attempt = 1s.
    assert_eq!(after1.run_at().duration_since(clock.now()).whole_seconds(), 1);

    // Advance and run attempt 2.
    clock.advance(Duration::seconds(2));
    let due = q.dequeue_due(None).await;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].attempts(), 2);
    q.mark_failed(job.id(), "boom-2".into()).await.unwrap();
    let after2 = q.get(job.id()).unwrap();
    assert_eq!(after2.status(), JobStatus::Queued);
    // Backoff after 2 attempts = 2s.
    assert_eq!(after2.run_at().duration_since(clock.now()).whole_seconds(), 2);

    // Advance and run attempt 3 (== max_attempts).
    clock.advance(Duration::seconds(3));
    let due = q.dequeue_due(None).await;
    assert_eq!(due[0].attempts(), 3);
    q.mark_failed(job.id(), "boom-3".into()).await.unwrap();
    let after3 = q.get(job.id()).unwrap();
    assert_eq!(after3.status(), JobStatus::Failed);
    assert!(after3.status().is_terminal());
    assert_eq!(after3.last_error(), Some("boom-3"));

    // A failed job is never due again.
    clock.advance(Duration::seconds(3600));
    assert!(q.dequeue_due(None).await.is_empty());
}

#[tokio::test]
async fn mark_succeeded_is_terminal_and_clears_error() {
    let (_clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;
    q.dequeue_due(None).await;
    q.mark_failed(job.id(), "transient".into()).await.unwrap();
    q.dequeue_due(None).await; // not due (backoff) — but force success anyway
    q.mark_succeeded(job.id()).await.unwrap();
    let done = q.get(job.id()).unwrap();
    assert_eq!(done.status(), JobStatus::Succeeded);
    assert_eq!(done.last_error(), None);
}

#[tokio::test]
async fn mark_unknown_job_is_not_found() {
    let (_clock, q) = queue(5);
    let err = q.mark_succeeded(JobId::new()).await.unwrap_err();
    assert!(matches!(err, PlatformError::JobNotFound { .. }));
}

// ── dequeue_stalled tests ─────────────────────────────────────────────────

#[tokio::test]
async fn fresh_queued_job_never_stalls() {
    let (_clock, q) = queue(5);
    q.enqueue("k".into(), serde_json::json!(null)).await;
    // Still Queued — not Running — so it must not appear in stall recovery.
    let recovered = q.dequeue_stalled(Duration::ZERO).await;
    assert!(recovered.is_empty());
}

#[tokio::test]
async fn running_job_within_stall_window_is_not_recovered() {
    let (clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;
    // Dequeue: status -> Running, run_at stays at t=0.
    q.dequeue_due(None).await;

    // Advance by exactly stall_after (not *strictly* greater).
    let stall_after = Duration::seconds(30);
    clock.advance(stall_after);

    let recovered = q.dequeue_stalled(stall_after).await;
    assert!(recovered.is_empty(), "job still within window must not be recovered");
    assert_eq!(q.get(job.id()).unwrap().status(), JobStatus::Running);
}

#[tokio::test]
async fn running_job_past_stall_window_is_recovered_to_queued() {
    let (clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;
    // run_at = t=0; after dequeue the job is Running at run_at=t=0.
    q.dequeue_due(None).await;

    let stall_after = Duration::seconds(30);
    // Advance past the stall window.
    clock.advance(Duration::seconds(31));

    let recovered = q.dequeue_stalled(stall_after).await;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id(), job.id());
    assert_eq!(recovered[0].status(), JobStatus::Queued);
    // run_at is reset to now (immediately due).
    assert_eq!(recovered[0].run_at(), clock.now());
}

#[tokio::test]
async fn recovered_jobs_appear_in_next_dequeue_due() {
    let (clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;
    q.dequeue_due(None).await;
    // Simulate a stall.
    clock.advance(Duration::seconds(61));
    let recovered = q.dequeue_stalled(Duration::seconds(60)).await;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id(), job.id());

    // The recovered job must now be picked up by dequeue_due.
    let due = q.dequeue_due(None).await;
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id(), job.id());
    assert_eq!(due[0].status(), JobStatus::Running);
}

#[tokio::test]
async fn succeeded_job_is_not_recovered_by_dequeue_stalled() {
    let (clock, q) = queue(5);
    let job = q.enqueue("k".into(), serde_json::json!(null)).await;
    q.dequeue_due(None).await;
    q.mark_succeeded(job.id()).await.unwrap();

    // Even past the stall window, Succeeded jobs must be ignored.
    clock.advance(Duration::seconds(999));
    let recovered = q.dequeue_stalled(Duration::ZERO).await;
    assert!(recovered.is_empty());
}

#[test]
fn enqueued_job_round_trips_through_json() {
    let job = EnqueuedJob {
        id: JobId::new(),
        kind: "k".into(),
        payload: serde_json::json!({"x": 1}),
        run_at: Timestamp::from_unix_millis(0),
        attempts: 0,
        max_attempts: 5,
        status: JobStatus::Queued,
        created_at: Timestamp::from_unix_millis(0),
        last_error: None,
    };
    let json = serde_json::to_string(&job).unwrap();
    let back: EnqueuedJob = serde_json::from_str(&json).unwrap();
    assert_eq!(job, back);
}
