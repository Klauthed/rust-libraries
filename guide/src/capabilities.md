# Capabilities by crate

A guided map of what each crate offers. For the exhaustive entry-point list see
[CAPABILITIES.md](https://github.com/Klauthed/rust-libraries/blob/master/CAPABILITIES.md);
for full API docs, each crate is on [docs.rs](https://docs.rs).

## klauthed-core

The shared foundation: layered [configuration](configuration.md), an injectable
`Clock` and UTC-canonical `Timestamp`, phantom-typed `Id<T>` (UUID/ULID), a
`Validate` trait, per-request `RequestContext`, CQRS and domain building blocks,
and the `AppContext` / `Starter` wiring.

## klauthed-security

A toolkit over vetted crypto crates — no hand-rolled primitives:

- **JWT** — `JwtSigner` / `JwtVerifier` (HS256/RS256/ES256/EdDSA).
- **PASETO** (feature `paseto`) — v4.public (Ed25519) and v4.local
  (XChaCha20-Poly1305), sharing the same `Claims` as JWT.
- **AEAD** — AES-256-GCM, envelope encryption, sealed boxes (feature `sealed`).
- **Passwords** — Argon2id; optional HIBP breach check (feature `hibp`).
- **MFA** — TOTP + recovery codes. **Passkeys/WebAuthn** (feature `webauthn`).
- **OAuth2 server primitives** — auth codes + PKCE, client registry, rotating
  refresh tokens, a revocation denylist.

## klauthed-data

Feature-gated connectors and reliability patterns: SQL (`sqlx`), Mongo, Redis,
NATS/RabbitMQ/Kafka, object storage; plus a **migration runner**, transactional
**outbox** with a polling **relay**, an in-process **event bus**, **transactional
executors** (`SqlxTransact`, `MongoTransact`), **sagas** (compensating steps),
**idempotency**, distributed **locks**, **rate limiting**, and cursor
**pagination**.

## klauthed-web

The actix-web layer: context / security-headers / CSRF / CORS / rate-limit /
`JwtAuth` middleware, validating extractors, a uniform `AppError`, health probes,
the OAuth2/OIDC server endpoints, **OpenAPI generation** + **Swagger UI**, a
**`POST /refresh`** config endpoint, a Prometheus **`GET /metrics`** scrape
endpoint (feature `metrics`), **passkey HTTP endpoints** (feature `webauthn`),
and **resilience patterns** (`RetryPolicy` with exponential backoff + a
`CircuitBreaker`) for outbound calls.

## klauthed-discovery

A `ServiceRegistry` (register / deregister / heartbeat / instances) with
in-memory, **Consul**, **Eureka**, and **Kubernetes** (read-only, Endpoints API)
backends, a `ServiceAgent` for lifecycle, and lock-free `RoundRobin` balancing.

## klauthed-observability

Structured tracing, Prometheus **metrics**, and **OpenTelemetry** OTLP export with
W3C trace-context propagation — paired with klauthed-web's `RequestTracing`
middleware for end-to-end distributed traces.

## klauthed-platform

Cross-cutting platform services — each a trait with an in-memory implementation:
multi-tenancy (`TenantResolver`), audit logging (`AuditSink`), HMAC-signed
outbound `WebhookSender`, feature flags, a background `JobQueue` **with a
`JobWorker`** that drains it, per-tenant usage **metering** (`Meter`), and
user **notifications** (`Notifier` — email / SMS / push).

The **`scheduler`** feature adds an in-process `Scheduler` for recurring work,
with fixed intervals or cron schedules (UTC or a named IANA timezone, DST-aware):

```rust,ignore
use std::time::Duration;
use klauthed_platform::scheduler::{Cron, Scheduler};

let handle = Scheduler::new()
    .every(Duration::from_secs(30), || async { /* every 30s */ })
    .cron(Cron::parse_in_timezone("0 2 * * *", "America/New_York")?, || async {
        // 02:00 New York time, daily (handles DST)
    })
    .start();
// handle.shutdown().await; — or drop it to stop the tasks
```

A panic in one run is isolated, so a bad run never silently kills the schedule.
Pairing a `JobWorker` with `Scheduler::every` to drain a `JobQueue` on an interval
is the canonical background-jobs setup — the
[reference service](https://github.com/Klauthed/rust-libraries/blob/master/reference-service/src/main.rs)
wires queue → worker → scheduler → notifications end to end.

## The rest

- **klauthed-protocol** — spec-accurate OAuth2 / OIDC / SCIM / JWKS wire types.
- **klauthed-i18n** — locales, message bundles, formatting.
- **klauthed-error** — the `DomainError` kernel (see [Architecture](architecture.md)).
- **klauthed-testing** — assertions, a controllable clock, builders, in-memory repos.
