# Roadmap

Direction for the klauthed Rust libraries. All crates share one version and ship
together (see [CONTRIBUTING.md](CONTRIBUTING.md#versioning--releases)). This file
tracks intent, not a commitment — scope shifts as we learn.

## Shipped

- **0.1.0** — first crates.io release. Config (env/file/Vault/config-server +
  hot-reload + `FromConfig` + `AppContext`/starters), security (JWT, AEAD, MFA,
  WebAuthn, OAuth2 primitives), data (SQL/Mongo/Redis/NATS/Rabbit/Kafka/storage +
  outbox/idempotency/locks/rate-limit/saga), discovery (in-memory/Consul/Eureka +
  agent), actix web layer, observability, i18n, platform. No-panic lints, OSV gate,
  live-infra integration tests. See [CHANGELOG](CHANGELOG.md).
- **0.2.0** — auto-config / observability / migrations round-out: resource
  starters (`DataStarter`/`WebStarter`) with async wiring, embedded DB migration
  runner, Rust-native config server + native client format, HIBP breach check,
  and OpenTelemetry request tracing + W3C trace-context propagation. Trusted
  Publishing (OIDC) releases. See [CHANGELOG](CHANGELOG.md).
- **0.3.0** — harden + round out the web surface: assurance (property tests,
  cargo-fuzz targets + nightly workflow, coverage gate + criterion benches),
  OpenAPI 3.1 generation + bundled Swagger UI, PASETO v4 tokens (v4.public
  Ed25519 + v4.local XChaCha20-Poly1305), and config push-refresh
  (`RefreshTrigger` + `POST /refresh`). See [CHANGELOG](CHANGELOG.md).
- **0.4.0** — discovery/auth surface + adoption: Kubernetes discovery backend,
  WebAuthn passkey HTTP endpoints, more fuzz targets (JWK/OIDC/SCIM), an mdBook
  guide, and a runnable reference service dogfooding the suite. Per-job CI
  timeouts. See [CHANGELOG](CHANGELOG.md).

- **0.5.0** — toward-1.0 API ergonomics & policy: per-crate `prelude` modules,
  `#[must_use]` on every builder method, a committed stability policy
  (SemVer/deprecation/MSRV in CONTRIBUTING.md), and a GitHub Pages workflow for
  the mdBook guide. See [CHANGELOG](CHANGELOG.md).
- **0.6.0** — a service scaffolding CLI (`klauthed-cli`: `cargo klauthed new`
  with `--with-jwt` / `--database` / `--with-scheduler`), an interval `Scheduler`
  in `klauthed-platform`, umbrella `data-sql`/`data-redis` feature forwarding, and
  added auth/event/cqrs test coverage. See [CHANGELOG](CHANGELOG.md).
- **0.7.0** — cron schedules (UTC + named-timezone, DST-aware) on the `Scheduler`,
  a Prometheus `GET /metrics` endpoint (`klauthed-web`), and an API naming
  consistency pass (de-stuttered messaging connectors + documented conventions).
  See [CHANGELOG](CHANGELOG.md).

- **0.8.0** — reliability & background-work patterns: user notifications, usage
  metering, and a job worker (`klauthed-platform`); resilience patterns
  (`RetryPolicy` + `CircuitBreaker`, `klauthed-web`); and data patterns —
  transactional executors (`SqlxTransact` / `MongoTransact`), an in-process event
  bus, saga orchestration, and an outbox relay. The reference service now dogfoods
  the queue → worker → scheduler → notifications pipeline. See [CHANGELOG](CHANGELOG.md).
- **0.9.0** — durable `JobQueue` backends: `SqlJobQueue` (SQLite/Postgres/MySQL,
  with a Postgres `FOR UPDATE SKIP LOCKED` claim) and `RedisJobQueue` (atomic Lua
  claim), behind a now-fallible `JobQueue` trait. Plus a `SqlOutbox` Postgres
  placeholder fix, cross-backend parity + cron property tests, and a `quinn-proto`
  security bump. See [CHANGELOG](CHANGELOG.md).

- **0.10.0** — the pre-1.0 settling cycle. A per-crate API review (found the public
  surface already coherent — no breaking changes; added the `klauthed-testing`
  prelude, `AppContext::builder`, and documented the naming conventions) plus a
  broad property-test sweep over the credential primitives (JWT, PASETO, AEAD,
  Argon2), `Timestamp`, cron, i18n, and cross-backend job-queue parity.
  See [CHANGELOG](CHANGELOG.md).
- **1.0.0** — first stable release. Four release candidates of real-world
  hardening — validated by a full enterprise reference service built on the
  *published* crates — folded in `klauthed_web::ApiResponse`/`ApiResult` (rc.2)
  and umbrella-friendly `#[derive(DomainError)]` / `#[derive(FromConfig)]` via
  `crate = "…"` (rc.3 / rc.4). The public API is now under the firm SemVer +
  deprecation policy ([CONTRIBUTING.md](CONTRIBUTING.md#stability-policy)).
  See [CHANGELOG](CHANGELOG.md).

## 1.1.0 (next)

1.0 is out; the public API is **stable under SemVer** — breaking changes now
require a major bump, so future work lands as additive minors:

- Optional Redis/SQL-backed implementations of the auth stores (refresh tokens,
  passkeys, OAuth code/client) to complement the in-memory defaults.
- More reference examples + broader per-operation OpenAPI coverage.
- Continued ecosystem additions (backends, discovery, observability) as they
  earn their place — kept backward-compatible.
