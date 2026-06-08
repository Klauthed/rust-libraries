//! Redis-backed [`RateLimiter`] for a shared, cross-replica budget.

use std::time::Duration;

use async_trait::async_trait;
use redis::aio::ConnectionManager;

use super::{RateLimitOutcome, RateLimiter};
use crate::error::DataError;

/// Lua: increment the window counter and, on the first hit, arm its expiry.
///
/// `KEYS[1]` = counter key, `ARGV[1]` = window length in ms. Returns
/// `{current_count, pttl_ms}` atomically so the counter can never be left
/// without a TTL (which would wedge a key permanently).
const CHECK_SCRIPT: &str = "\
local current = redis.call('INCR', KEYS[1])
if current == 1 then
    redis.call('PEXPIRE', KEYS[1], ARGV[1])
end
return {current, redis.call('PTTL', KEYS[1])}";

/// A [`RateLimiter`] whose counters live in Redis, so every replica sharing the
/// instance enforces one global budget per key.
///
/// Fixed-window: each request `INCR`s `"<prefix><key>"`; the first increment in
/// a window sets its `PEXPIRE` to the window length, and the key expires when the
/// window closes. Clone-cheap (holds a [`ConnectionManager`]).
#[derive(Clone)]
pub struct RedisRateLimiter {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisRateLimiter {
    /// Wrap a managed Redis connection (see `cache::connect`), keying counters
    /// under the default `"ratelimit:"` prefix.
    #[must_use]
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn, prefix: "ratelimit:".to_owned() }
    }

    /// Use a custom key prefix (e.g. to namespace per service/tenant).
    #[must_use]
    pub fn with_prefix(conn: ConnectionManager, prefix: impl Into<String>) -> Self {
        Self { conn, prefix: prefix.into() }
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(
        &self,
        key: &str,
        max: u32,
        window: Duration,
    ) -> Result<RateLimitOutcome, DataError> {
        let max = max.max(1);
        let window_ms = window.as_millis().min(i64::MAX as u128) as i64;
        let redis_key = format!("{}{key}", self.prefix);

        let mut conn = self.conn.clone();
        let (count, pttl_ms): (i64, i64) = redis::Script::new(CHECK_SCRIPT)
            .key(redis_key)
            .arg(window_ms)
            .invoke_async(&mut conn)
            .await?;

        if count <= i64::from(max) {
            let remaining = (i64::from(max) - count).max(0) as u32;
            Ok(RateLimitOutcome::Allowed { remaining })
        } else {
            // PTTL is -1 (no expiry) / -2 (no key) only in races; fall back to
            // the full window so callers always get a sane Retry-After.
            let retry_ms = if pttl_ms > 0 { pttl_ms as u64 } else { window_ms.max(0) as u64 };
            Ok(RateLimitOutcome::Limited { retry_after: Duration::from_millis(retry_ms) })
        }
    }
}

/// Lua: continuous token-bucket check, evaluated atomically against the Redis
/// server clock so all replicas agree on "now".
///
/// `KEYS[1]` = bucket key, `ARGV[1]` = capacity, `ARGV[2]` = tokens-per-ms refill
/// rate, `ARGV[3]` = key TTL in ms. The bucket (`tokens`, `ts`) is stored as a
/// hash. Returns `{allowed (1/0), remaining (floor), retry_after_ms}`.
const TOKEN_BUCKET_SCRIPT: &str = "\
local capacity = tonumber(ARGV[1])
local refill = tonumber(ARGV[2])
local ttl = tonumber(ARGV[3])
local t = redis.call('TIME')
local now = (tonumber(t[1]) * 1000) + (tonumber(t[2]) / 1000)
local data = redis.call('HMGET', KEYS[1], 'tokens', 'ts')
local tokens = tonumber(data[1])
local ts = tonumber(data[2])
if tokens == nil then tokens = capacity; ts = now end
local elapsed = now - ts
if elapsed < 0 then elapsed = 0 end
tokens = math.min(capacity, tokens + (elapsed * refill))
local allowed = 0
local retry = 0
if tokens >= 1 then
    tokens = tokens - 1
    allowed = 1
else
    retry = math.ceil((1 - tokens) / refill)
end
redis.call('HSET', KEYS[1], 'tokens', tokens, 'ts', now)
redis.call('PEXPIRE', KEYS[1], ttl)
return {allowed, math.floor(tokens), retry}";

/// A Redis-backed **token-bucket** [`RateLimiter`] for a shared, cross-replica
/// budget with smooth (continuous) refill.
///
/// Like [`super::InMemoryTokenBucket`] but the bucket lives in Redis, evaluated
/// by an atomic Lua script against the Redis server clock so replicas can't drift.
/// Same `(max, window)` semantics as the fixed-window [`RedisRateLimiter`]:
/// `max` is the burst capacity, refilled at `max / window`.
#[derive(Clone)]
pub struct RedisTokenBucket {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisTokenBucket {
    /// Wrap a managed Redis connection, keying buckets under `"ratelimit:tb:"`.
    #[must_use]
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn, prefix: "ratelimit:tb:".to_owned() }
    }

    /// Use a custom key prefix.
    #[must_use]
    pub fn with_prefix(conn: ConnectionManager, prefix: impl Into<String>) -> Self {
        Self { conn, prefix: prefix.into() }
    }
}

#[async_trait]
impl RateLimiter for RedisTokenBucket {
    async fn check(
        &self,
        key: &str,
        max: u32,
        window: Duration,
    ) -> Result<RateLimitOutcome, DataError> {
        let capacity = f64::from(max.max(1));
        let window_ms = window.as_secs_f64().max(f64::MIN_POSITIVE) * 1000.0;
        let refill_per_ms = capacity / window_ms;
        let ttl_ms = window.as_millis().min(i64::MAX as u128) as i64;
        let redis_key = format!("{}{key}", self.prefix);

        let mut conn = self.conn.clone();
        let (allowed, remaining, retry_ms): (i64, i64, i64) =
            redis::Script::new(TOKEN_BUCKET_SCRIPT)
                .key(redis_key)
                .arg(capacity)
                .arg(refill_per_ms)
                .arg(ttl_ms)
                .invoke_async(&mut conn)
                .await?;

        if allowed == 1 {
            Ok(RateLimitOutcome::Allowed { remaining: remaining.max(0) as u32 })
        } else {
            Ok(RateLimitOutcome::Limited {
                retry_after: Duration::from_millis(retry_ms.max(0) as u64),
            })
        }
    }
}
