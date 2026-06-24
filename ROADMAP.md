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

## 1.0.0 (in progress) — release candidate published

**`1.0.0-rc.2` is the current candidate** (a pre-release on crates.io; stable
`0.10.0` remains the default) for real-world validation before the 1.0.0 SemVer
promise. The API has been frozen since the 0.10.0 settling cycle — no breaking
changes, only additions.

- Validation window: build against `1.0.0-rc.2`; surface API friction now, while
  the major version isn't yet committed. (`rc.2` already folded in the first such
  feedback — `klauthed_web::ApiResponse`/`ApiResult`, a uniform success envelope.)
- If nothing breaking surfaces → tag **1.0.0** (flip CONTRIBUTING's stability
  language to the firm 1.0 SemVer promise). If something does → fold it in and cut
  the next `rc`.
- Only additive changes on the public API meanwhile.

## Toward 1.0

The committed SemVer + deprecation policy (CONTRIBUTING.md), the MSRV policy, and
broad test / fuzz coverage are all in place; the API review found no breaking
changes. `1.0.0-rc.2` is out; **1.0.0 final follows once the RC validates clean.**
