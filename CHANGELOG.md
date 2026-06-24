# Changelog

All notable changes to the klauthed Rust libraries are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the workspace adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
All crates share a single version and are released together.

## [Unreleased]

### Changed

- Docs (pre-1.0 readiness): refreshed the install snippets to the current version,
  and corrected stale "future work / out of scope" notes — the durable `JobQueue`
  backends (`SqlJobQueue`, `RedisJobQueue`) and the `HttpWebhookSender`
  (`webhook-http`) have shipped and are now documented as available.

## [0.10.0] - 2026-06-24

### Added

- **`klauthed_testing::prelude`** — a prelude of the common test helpers
  (assertions, `FixedClock` + `fixed_clock`/`epoch_clock`, `test_context`,
  `nil_id`/`seeded_id`, `InMemoryRepository`), so every library crate now offers a
  prelude.
- **`AppContext::builder(config)`** (`klauthed-core`) — a `Type::builder()` entry
  point delegating to `AppBuilder::new`, for consistency with `Config::builder`
  and the suite's builder convention.

### Changed

- Documented the implementation / test-double / config-provider naming
  conventions (`InMemory*`, `Recording*`, `<Source>Provider`) in CONTRIBUTING's
  API conventions, as part of the 0.10.0 pre-1.0 API review.
- Broadened property-test coverage as part of the pre-1.0 settling cycle: the
  credential primitives (JWT, PASETO v4.public/local, AES-256-GCM AEAD, Argon2id
  passwords), `Timestamp`, cron, i18n fallback + interpolation, and cross-backend
  job-queue parity. No public API changes.

## [0.9.0] - 2026-06-23

### Added

- **Durable `RedisJobQueue`** (`klauthed-platform`, feature `jobs-redis`): a
  Redis-backed `JobQueue` — jobs as hashes, a `due` sorted set scored by `run_at`,
  and a `run` sorted set for stall detection. Claiming, failing (with backoff),
  and stalled-recovery run as atomic Lua scripts so concurrent workers never
  double-claim. Same semantics as `InMemoryJobQueue`; exercised by the CI
  integration job against a live Redis at `REDIS_URL`.
- **Durable `SqlJobQueue`** (`klauthed-platform`, feature `jobs-sql`): a
  relational `JobQueue` over sqlx's `AnyPool` (portable across SQLite / Postgres /
  MySQL), with the same claim / retry-backoff / stalled-recovery semantics as
  `InMemoryJobQueue` and a portable `ensure_schema`. On PostgreSQL,
  `dequeue_due_skip_locked` claims with `FOR UPDATE SKIP LOCKED` so concurrent
  workers get disjoint batches (exercised by the CI integration job at `DB_URL`).

### Changed

- **BREAKING** (`klauthed-platform`): the `JobQueue` trait's `enqueue`,
  `schedule`, `dequeue_due`, and `dequeue_stalled` now return
  `Result<_, PlatformError>` (they were infallible), so durable backends can
  surface storage errors — aligning `JobQueue` with the already-fallible `Outbox`
  trait. Direct callers must handle the `Result` (`?`/`unwrap`); `JobWorker` and
  the in-memory queue are updated.

### Fixed

- **`SqlOutbox` on PostgreSQL** (`klauthed-data`): the `AnyPool`-backed
  `enqueue` / `mark_published` used `?` bind placeholders, which PostgreSQL
  rejects — sqlx's `Any` driver passes SQL through without rewriting placeholders.
  They now translate `?` → `$n` when the pool is Postgres (same fix as
  `SqlJobQueue`), with a live-Postgres integration test covering the round-trip
  and the `FOR UPDATE SKIP LOCKED` claim.

### Security

- Bumped the transitive `quinn-proto` dependency 0.11.14 → 0.11.15 to address
  RUSTSEC-2026-0185 (High).

## [0.8.0] - 2026-06-22

### Added

- **User notifications** (`klauthed-platform`): a `Notifier` trait, a
  `Notification` / `Channel` (email / SMS / push) model, and a
  `RecordingNotifier` for tests. User-facing messaging, distinct from `webhooks`
  (system events to endpoint URLs). Completes the platform's declared service set.
- **Usage metering** (`klauthed-platform`): a `Meter` trait and an
  `InMemoryMeter` for per-tenant usage accounting — `record`/`usage`/`reset` by
  `(tenant, metric)` — for quotas and usage-based billing. A peer of the existing
  tenancy/audit/feature-flag services.
- **Job worker** (`klauthed-platform`): a `JobHandler` trait and a `JobWorker`
  that drains a `JobQueue` — `run_once` claims due jobs, runs the handler, and
  marks each succeeded or failed (the queue applies retry/backoff). Completes the
  background-jobs lifecycle (store → claim → process → outcome); compose with the
  scheduler for a long-running worker.
- **Resilience patterns** (`klauthed-web::resilience`): `RetryPolicy` — retry a
  fallible async operation with capped exponential backoff — and `CircuitBreaker`
  — fail fast after N consecutive failures, then half-open probe after a cooldown
  (clock-injectable, so the cooldown is deterministically testable). For hardening
  outbound calls and other fallible async work.
- **Transactional executors** (`klauthed-data`): `SqlxTransact` (feature `sql`)
  and `MongoTransact` (feature `mongodb`) — the concrete production counterparts to
  the `Transact` trait + `NoopTransact`. `run(async |handle| …)` begins a
  transaction, passes the connection/session handle to the closure, and commits on
  `Ok` / rolls back on `Err`. (They pass the handle rather than implementing the
  connection-less trait, since statements only join a transaction when issued on
  its connection/session.) Adds a `DataError::Transaction` variant.
- **In-process event bus** (`klauthed-data`): an `EventBus` publish trait, an
  `EventHandler` subscriber trait, and an `InMemoryEventBus` that fans every
  published event out to all subscribers — for decoupled in-process domain-event
  handling. Fills the previously-stubbed `eventbus` module.
- **Saga orchestration** (`klauthed-data`): a `Saga` of compensable steps — each a
  forward action paired with a compensation — run by `execute()`, which on the
  first failure runs the completed steps' compensations in reverse (returning a
  `SagaError` with the failed step index). Fills the previously-stubbed `saga`
  module; pure and in-memory.
- **Outbox relay** (`klauthed-data`): an `OutboxPublisher` sink trait and an
  `OutboxRelay` that drains an `Outbox` (fetch unpublished → publish → mark
  published) in batches, completing the transactional-outbox pattern. Stops at
  the first publish failure for at-least-once delivery; call `drain` periodically
  (e.g. from the platform scheduler).

## [0.7.0] - 2026-06-21

### Added

- **Prometheus `/metrics` endpoint** (`klauthed-web`, `metrics` feature; `metrics`
  on the umbrella): `metrics::serve_metrics(cfg, handle)` mounts `GET /metrics`
  rendering the exposition format from klauthed-observability's `MetricsHandle`,
  for Prometheus scraping.
- **Cron schedules** for the `Scheduler` (`klauthed-platform`, `scheduler`
  feature): a chrono-free `Cron` parser + next-occurrence calculator (5-field
  `minute hour day-of-month month day-of-week`, with ranges/lists/steps and the
  standard day-of-month/day-of-week OR rule), in UTC (`Cron::parse`) or a named
  IANA timezone (`Cron::parse_in_timezone("0 9 * * *", "America/New_York")`, DST
  handled), plus `Scheduler::cron(schedule, task)` to run tasks on a calendar
  schedule alongside interval tasks. The CLI's `--with-scheduler` scaffold now
  demonstrates both an interval and a cron task.

### Changed

- **`klauthed-data` messaging connectors** — the per-backend modules now expose a
  plain `connect` (`messaging::nats::connect`) instead of a stuttering
  `connect_nats`; the canonical `messaging::connect_nats` / `connect_rabbitmq` /
  `connect_kafka` paths are unchanged (re-exported). Documented the suite's API
  naming conventions in CONTRIBUTING.

## [0.6.0] - 2026-06-19

### Added

- **Interval scheduler** (`klauthed-platform`, `scheduler` feature; `scheduler` on
  the umbrella): a lightweight `Scheduler` that runs async tasks on a fixed period
  on the Tokio runtime, with a `SchedulerHandle` that stops them on `shutdown()`
  or drop. Runs are sequential per task and a panic in one run is isolated, so a
  bad run can't silently kill the schedule. Fills the recurring-background-work
  gap alongside the existing `JobQueue`.
- **`klauthed-cli` — service scaffolding** (`cargo install klauthed-cli`): the
  `cargo klauthed new <name>` subcommand generates a ready-to-run actix-web
  service (config + telemetry + web with `/hello` and the framework health
  probes, plus tests, config, and a README). The generated project depends on the
  umbrella `klauthed` crate at the matching `major.minor`. A `--with-jwt` flag
  adds JWT auth — a `/login` endpoint plus a protected `/api/me` route (enabling
  the `security` feature) — `--database postgres|mysql|sqlite` wires a
  connection pool into the web layer (and its readiness probe) with a `[database]`
  config section, and `--with-scheduler` starts an interval scheduler with an
  example recurring task. Flags compose.

### Changed

- The umbrella `klauthed` crate's `postgres` / `mysql` / `sqlite` / `redis`
  features now also forward `klauthed-web?/data-sql` (or `data-redis`), so a SQL
  pool / Redis connection can be wired into the web `Components` (and its health
  probe) when both `web` and a backend are enabled — previously there was no way
  to enable that integration through the umbrella crate.

## [0.5.0] - 2026-06-19

### Added

- **`#[must_use]` on all builders** — every builder method returning `Self`
  across the workspace is now `#[must_use]`, so dropping a builder chain is a
  compile-time warning. Part of the toward-1.0 API-consistency pass.
- **Stability policy** (CONTRIBUTING.md): a committed public-API/SemVer
  definition, a deprecation policy (`#[deprecated]`, kept ≥1 minor release), and
  an explicit MSRV policy (1.95; raising it is a minor bump).
- **mdBook guide on GitHub Pages**: a `Pages` workflow builds `guide/` and
  publishes it to GitHub Pages on every change (self-enabling via
  `configure-pages`), so the guide is browsable online.
- **Per-crate `prelude` modules** — each library crate now exposes a curated
  `prelude` re-exporting its common types, so a service can
  `use klauthed_web::prelude::*;` (and likewise for `core`, `error`, `data`,
  `security`, `observability`, `discovery`, `protocol`, `platform`, `i18n`).
  Feature-gated items (e.g. the `sql` data types) are included under the matching
  cfg. First step of the toward-1.0 API-ergonomics pass.

## [0.4.0] - 2026-06-18

### Added

- **Reference service** (`reference-service/`, not published): a small runnable
  service wiring config + telemetry + the web layer + JWT auth end to end
  (`/login` issues a token, `/api/me` is `JwtAuth`-protected, health probes via
  `serve_with_defaults`). A starting template, dogfooding the suite; covered by
  end-to-end tests.
- **mdBook guide** (`guide/`): a narrative companion to the reference docs —
  introduction, getting started, architecture & design principles, the
  configuration model, a capability map, and the release/versioning policy. Built
  in CI (`mdbook build`).
- **Kubernetes discovery backend** (`klauthed-discovery`, feature `kubernetes`):
  `KubernetesRegistry` resolves a service's ready instances from the Kubernetes
  Endpoints API (`instances`), with `in_cluster()` config (service-account token,
  CA, namespace) or an explicit API base URL. Read-only — `register`/`deregister`/
  `heartbeat` error (the platform owns pod lifecycle). Reuses `reqwest` (no heavy
  SDK) and is wiremock-tested, no live cluster required.
- **Passkey HTTP endpoints** (`klauthed-web`, feature `webauthn`): a `passkey`
  module exposing the WebAuthn ceremonies over four `POST` routes
  (`register/start`·`finish`, `login/start`·`finish`). Mount a `PasskeyApi`
  (relying party + `PasskeyStore` + a new `CeremonyStore` for in-flight state,
  with an `InMemoryCeremonyStore`); `login/finish` returns the verified user
  handle and persists the signature counter. Verified end-to-end against a
  software authenticator. The umbrella's `webauthn` feature enables it with `web`.
- More **fuzz targets** (`cargo-fuzz`, in `fuzz/`) for the untrusted-input
  parsers: JWKS / JWK documents, OIDC discovery metadata + ID-token claims, and
  SCIM `User` / `Group` / PATCH deserialization. Wired into the nightly `Fuzz`
  workflow matrix alongside the existing targets.

## [0.3.0] - 2026-06-18

### Added

- **Config push-refresh** (`klauthed-core`, feature `hot-reload`):
  `ReloadableConfig::start_with_refresh` returns a clonable `RefreshTrigger` whose
  `refresh()` re-resolves the provider chain immediately (coalesced), so a
  config-server webhook, a discovery / message-bus event, or an HTTP `/refresh`
  endpoint can push changes live instead of waiting for the poll interval. The
  periodic refresh remains as a safety net. `klauthed-web`'s `config-refresh`
  feature adds `refresh::serve_refresh`, a `POST /refresh` endpoint that drives
  the trigger (the Spring `/actuator/refresh` analog).
- **Swagger UI** (`klauthed-web`, feature `swagger-ui`): `openapi::serve_swagger_ui`
  mounts an interactive Swagger UI backed by the generated spec, with the UI
  assets vendored into the binary (no build-time or runtime network access).
- **PASETO v4 tokens** (`klauthed-security`, feature `paseto`): mint/verify from
  the same `Claims` as JWT — a misuse-resistant alternative (versioned protocol,
  no `alg` confusion), built on the audited `pasetors`. `PasetoV4Signer` /
  `PasetoV4Verifier` for **v4.public** (Ed25519, signed/readable) and
  `PasetoV4Local` for **v4.local** (XChaCha20-Poly1305, encrypted/confidential).
  Adds `Timestamp::parse_rfc3339` to `klauthed-core` (inverse of `to_rfc3339`).
- **OpenAPI generation** (`klauthed-web`, feature `openapi`): generate an OpenAPI
  3.1 document with `utoipa`. The built-in health endpoints ship annotated
  (`openapi::base_openapi()`), `openapi::serve_spec` exposes the JSON, and
  `utoipa` is re-exported so a service merges its own annotated paths into one
  document. The umbrella's `openapi` feature enables it (with `web`).
- **Coverage gate + benchmarks**: a `coverage` CI job runs `cargo-llvm-cov` with
  a line-coverage floor (currently ~79%), and `criterion` micro-benchmarks
  (dev-only) cover the hot paths — config merge/expand, id generation/parse,
  pagination cursor encode/decode, and HS256 JWT + AES-256-GCM AEAD.
- **Fuzz targets** (`cargo-fuzz`, in `fuzz/`) for the untrusted-input parsers —
  JWT decode, AEAD decrypt, OAuth2 token-response deserialization, and the config
  tree-shaping (`expand_dotted` / `merge`). A separate nightly `Fuzz` workflow
  runs them time-boxed (weekly + manual); the crate stays out of the stable
  workspace gates.
- **Property tests** (`proptest`, dev-only) for core invariants: config
  deep-merge (empty-identity, idempotence, key-union, non-object overlay wins),
  `Id` UUID/ULID string round-trips and value-ordering, and pagination `Cursor`
  encode/decode round-trips (including graceful handling of arbitrary input).

## [0.2.0] - 2026-06-17

### Changed

- `klauthed-core`: **`ConfigServerProvider` now defaults to the klauthed-native
  format** (`ConfigServerFormat::Klauthed`), pairing with our own config server;
  added `klauthed()` / `spring_cloud()` shorthands. `SpringCloud` / `RawJson`
  remain for talking to an existing Spring Cloud server or arbitrary JSON.
- `klauthed-core`: **`wiring::Starter` / `AppBuilder` are now async** (breaking) —
  `Starter::configure` and `AppBuilder::build` are `async`, and a starter fails
  with the new broad `StarterError` (`Box<dyn Error + Send + Sync>`) instead of
  `ConfigError`, so starters can build live resources (pools, clients) and
  surface any error. (Pre-1.0; `klauthed` 0.1.0 had the sync form.)
- Release: publishing now uses **crates.io Trusted Publishing (OIDC)** via
  `rust-lang/crates-io-auth-action` — no long-lived `CRATES_IO_TOKEN` secret.
  Requires a one-time per-crate Trusted Publisher setup on crates.io (see
  CONTRIBUTING).

### Added

- **OpenTelemetry request tracing** (feature `otel`): `klauthed-web`'s
  `RequestTracing` actix middleware opens a span per request (method/path/status)
  and links it to the caller's trace by extracting the inbound W3C `traceparent`;
  `klauthed-observability::propagation` carries W3C context across services
  (`inject_current` for outbound reqwest). Spans export through the existing OTLP
  pipeline. The umbrella's `otel` / `config-server` features now also enable the
  matching `klauthed-web` features when `web` is on.
- `klauthed-data`: **migration runner** (`migrate::{Migrator, Migration}`, feature
  `sql`) — embedded, versioned schema migrations over the driver-agnostic
  `AnyPool`, tracked in a `_klauthed_migrations` table so each runs exactly once
  (idempotent re-runs); each migration applies in its own transaction. Re-exports
  `AnyPool`. New `DataError::Migration`.
- `klauthed-web`: **config server** (`config_server` module, feature
  `config-server`) — run a klauthed service *as* the config server other services
  pull from, the Rust-native counterpart to Spring Cloud Config Server. A
  `ConfigServer` serves `GET /{application}/{profile}[/{label}]` as a native
  `ConfigDocument`, backed by a `ConfigSource` (`DirectoryConfigSource` over
  layered TOML/JSON files, or `InMemoryConfigSource`). `klauthed-core`'s
  `ConfigServerProvider` consumes it with the default `Klauthed` format.
- `klauthed-security`: **HIBP breach check** (`password::hibp::HibpClient`, feature
  `hibp`) — checks a password against Have I Been Pwned's "Pwned Passwords" via
  the k-anonymity range API: only the first 5 hex chars of the password's SHA-1
  are sent, the match is done client-side. `pwned_count` / `is_pwned`; new
  `SecurityError::Hibp`. wiremock-tested.
- **Resource starters** — `wiring::Starter`s that wire live resources into an
  `AppContext`:
  - `klauthed-data`: **`DataStarter`** (feature `sql`) builds the relational pool
    (`sqlx::AnyPool`) from the `database` config section and registers it, so
    components `require::<AnyPool>()` instead of connecting by hand.
  - `klauthed-web`: **`WebStarter`** assembles the actix `Components` (web `Data`
    + readiness health checks) from resources already in the context (e.g. the
    `DataStarter` pool), ready for `serve_with_components`.
- `klauthed` (umbrella): re-export **`klauthed::discovery`** behind a `discovery`
  feature, and forward the newer sub-crate features that were previously
  unreachable through the umbrella — `config-server` / `hot-reload` (core),
  `webauthn` (security), and `consul` / `eureka` / `agent` (discovery). Added
  `discovery` to `full` and surfaced `ServiceInstance` / `ServiceRegistry` in the
  prelude, so the one-crate entry point now reaches every library.

## [0.1.0] - 2026-06-09

### Added

- Initial release of the workspace: the `klauthed` umbrella plus `klauthed-core`,
  `-error`, `-macros`, `-i18n`, `-security`, `-protocol`, `-data`, `-discovery`,
  `-platform`, `-observability`, `-web`, and `-testing` — published to crates.io.
- **Release automation** — a tag-triggered `release` workflow that runs
  `cargo publish --workspace` (native dependency-ordered publish) and cuts a
  GitHub Release, plus `release.toml` for `cargo-release` (shared workspace
  version). The crates are publish-ready (verified via
  `cargo publish --workspace --dry-run`). Documented in CONTRIBUTING under
  "Versioning & releases"; added `CODE_OF_CONDUCT.md`.
- CI: an **`osv-scanner`** supply-chain job that scans `Cargo.lock` against the
  OSV database (GHSA + RustSec). It catches GHSA-only advisories that `cargo-deny`
  (RustSec DB) can miss — the gap that let the jsonwebtoken type-confusion
  advisory through before it was mirrored to RustSec.
- **`CAPABILITIES.md`** — a single, guided tour of every crate's capabilities,
  feature flags, and entry points (linked from the README).
- `klauthed-core`: **`wiring::AppContext`** — a small, explicit application wiring
  container: a type-keyed registry of shared singletons (`register` / `get` /
  `require`), framework-agnostic, paired with `FromConfig` via
  `register_from_config::<T>(&config)`. Not a reflective DI container (Rust has no
  runtime reflection) — components are constructed in dependency order and
  resolved by type.
- `klauthed-core`: **auto-config starters** (`wiring::{Starter, AppBuilder,
  ConfigSectionsStarter}`) — `AppBuilder` runs a chain of `Starter`s over a
  resolved `Config` to wire an `AppContext` (Rust-idiomatic Spring-Boot-style
  auto-configuration, composed explicitly rather than scanned).
  `ConfigSectionsStarter` registers the present standard typed config sections.
- `klauthed-core` / `klauthed-macros`: **`#[derive(FromConfig)]`** — bind a typed
  struct to a config section (`#[config(key = "database")]`, defaulting to the
  snake-cased type name; `#[config(default)]` binds a missing section to
  `Default`). The klauthed analog of Spring's `@ConfigurationProperties`:
  `MyConfig::from_config(&config)?` instead of a hand-written `config.get(...)`.
  New `config::FromConfig` trait + derive.
- `klauthed-core`: **hot-reloading config** (`config::ReloadableConfig`, feature
  `hot-reload`) — re-resolves the provider chain on an interval (and on demand
  via `reload_now`), atomically swapping in the new `Config` and notifying
  subscribers (`subscribe`) only when the resolved tree actually changes. Reads
  are cheap `Arc<Config>` snapshots via `current()`; the background task is
  aborted on drop. Pairs with the config-server provider for restart-free config
  changes. `ConfigBuilder` gains `resolve(&self)` / `ensure_defaults`.
- `klauthed-core`: **remote config-server provider** (`config::provider::
  ConfigServerProvider`, feature `config-server`) — loads configuration over HTTP
  from a **Spring Cloud Config Server**-compatible endpoint
  (`/{application}/{profile}[/{label}]`, merging ordered `propertySources` of
  flat dotted keys) or a plain JSON document (`RawJson`), deep-merged into the
  config tree like the file/env/Vault providers. Basic/Bearer auth, an `optional`
  fail-soft mode, and a new non-secret `ProviderKind::ConfigServer` (secrets stay
  in Vault). New `ConfigError::ConfigServer` variant.
- **`klauthed-discovery`** crate — service discovery: a `ServiceRegistry` trait
  (`register` / `deregister` / `heartbeat` / `instances`) over a `ServiceInstance`
  type, with an `InMemoryRegistry` for tests/single-process use and a lock-free
  `RoundRobin` picker for client-side load balancing. HTTP backends for **Consul**
  (`ConsulRegistry`, feature `consul`) and **Eureka** (`EurekaRegistry`, feature
  `eureka`), plus a self-registering **`ServiceAgent`** (feature `agent`) that
  registers on start, heartbeats in the background, and deregisters on
  shutdown/drop.
- CI: an **`integration` job** that runs the `#[ignore]`d live-infra tests
  against real Postgres, Redis, and MongoDB service containers (`cargo test
  -- --ignored`), so the data-layer (outbox, idempotency, locks, Mongo repo)
  and web health-check paths are actually exercised, not just compiled. Locally:
  start the three backends and run with `DB_URL` / `REDIS_URL` / `MONGODB_URL`
  set.
- `klauthed-security`: **WebAuthn / passkeys** (`passkey` module, feature
  `webauthn`) — a `PasskeyAuthenticator` relying-party wrapper over `webauthn-rs`
  driving the registration and authentication ceremonies (`start`/`finish` for
  each), plus a `PasskeyStore` trait with an in-memory implementation
  (`InMemoryPasskeyStore`) keyed by user handle. The challenge/state/credential
  types are re-exported so callers need no direct `webauthn-rs` dependency, and a
  software-authenticator integration test exercises the full
  register → store → authenticate flow. The feature is **off by default**: it
  pulls `webauthn-rs` (MPL-2.0, now allow-listed in `deny.toml`) and links
  OpenSSL (CI installs `libssl-dev`). New `SecurityError::WebauthnConfig` /
  `Webauthn` variants.
- `klauthed-security`: **MFA recovery codes** (`mfa::RecoveryCodeSet`) — generate
  a set of single-use backup codes, shown to the user once and persisted only as
  SHA-256 hashes (serializes to a JSON hash list). `verify_and_consume` is
  case/separator-insensitive, constant-time, and spends a code on use. No new
  dependencies.
- `klauthed-web`: **security-headers middleware** (`headers::SecurityHeaders`) —
  adds the standard hardening response headers (HSTS, `Content-Security-Policy`,
  `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, and the
  cross-origin isolation headers) from a `SecurityHeadersConfig`, with strict
  API defaults and a `relaxed()` preset for HTML apps. Existing handler-set
  headers are never clobbered.
- `klauthed-web`: **CSRF protection** (`csrf::Csrf`) — a stateless
  double-submit-cookie middleware. Unsafe requests must echo the CSRF cookie in
  a header (constant-time compared via `klauthed-security`); safe requests can
  auto-issue the cookie; `Authorization: Bearer` requests are skipped by default.
  Returns `403 Forbidden` on failure, and `Csrf::issue_cookie` rotates the token.

- `klauthed-security`: JWT signing/verification now supports **ES256** (ECDSA
  P-256) and **EdDSA** (Ed25519) in addition to HS256/RS256. Asymmetric keys load
  from PEM or DER (`{rs256,es256,eddsa}_{pem,der}` on `JwtSigner`/`JwtVerifier`).
- `klauthed-security`: **sealed-box (public-key) encryption** (`aead::asymmetric`,
  feature `sealed`) — `seal_to` a recipient's X25519 public key with no pre-shared
  key (ECIES-style: ephemeral X25519 ECDH -> HKDF -> AES-256-GCM); only the
  matching `SecretKey` can `open`. New optional dep: x25519-dalek.
- `klauthed-security`: **AEAD envelope encryption** (`aead::seal` / `Envelope`) —
  encrypt under a fresh per-message data key wrapped by a long-lived root key,
  with `Envelope::rewrap` for root-key rotation without re-encrypting payloads,
  and a self-contained byte/base64 wire format.
- `klauthed-core`: optional **`tz` feature** — convert the UTC `Timestamp` to
  civil time in a named IANA zone via `time::TimeZone` (`get`, `to_zone`,
  `offset_in`), backed by `time-tz`. The `Timestamp` stays UTC-canonical.
- `klauthed-security`: **role inheritance** — a `Role` may declare parent roles
  (`Role::inherits` / `inherit`), and `RoleRegistry::effective_permissions`
  resolves the permission union transitively and cycle-safely.
- `klauthed-security`: **resource-instance scoping** — `Authorizer::is_authorized_for_resource`
  / `authorize_for_resource` permit an action when the principal holds the
  permission globally *or* owns the resource and holds its `:own`-suffixed form
  (e.g. `articles:edit:own`).
- `klauthed-security`: **ABAC policy layer** (`authz::policy`) — a `PolicySet` of
  `Allow`/`Deny` `Policy` rules whose `Condition`s test request `Attributes`
  (subject/resource/action/env), evaluated with deny-overrides and default-deny,
  complementing the existing RBAC `Authorizer`.
- `klauthed-web`: `auth_service` example — a runnable end-to-end demo
  (password login → JWT → rate-limited, JWT-protected API + health), wiring
  `klauthed-security`, `klauthed-web` middleware/extractors, and the error layer.
- `klauthed-data`: new `rate_limit` module — a `RateLimiter` trait with a
  clock-injected `InMemoryRateLimiter` and a `RedisRateLimiter` (`redis` feature)
  for shared, cross-replica fixed-window limiting. Token-bucket variants
  (`InMemoryTokenBucket`, `RedisTokenBucket`) add continuous-refill smoothing with
  the same `(max, window)` API, interchangeable behind `Arc<dyn RateLimiter>`.
- `klauthed-web`: the rate-limit middleware now uses a pluggable `RateLimiter`
  store. `RateLimit::new` keeps the per-process in-memory limiter;
  `RateLimit::with_store` accepts any `Arc<dyn RateLimiter>` (e.g. Redis) for one
  global budget across replicas. The middleware **fails open** if the store
  errors, so a limiter outage cannot take the service down.
- Supply-chain CI gates: `cargo-deny` (RustSec advisories + license allow-list +
  source policy) and an MSRV (Rust 1.95) build job.
- crates.io publish metadata on every member crate (`description`, `keywords`,
  `categories`, workspace-inherited `license`/`repository`/`authors`, `readme`).

### Security

- `klauthed-security`: upgraded `jsonwebtoken` from 9.3 to 10.3, which fixes the
  algorithm-confusion advisory **GHSA-h395-gr6q-cpjc** (type confusion that could
  lead to an authorization bypass). v10 drops the `ring` backend and requires an
  explicit crypto provider; we use **`aws_lc_rs`**, which keeps the dependency
  tree free of security advisories (the alternative `rust_crypto` backend pulls
  the `rsa` crate, RUSTSEC-2023-0071). The `JwtSigner` / `JwtVerifier` public API
  is unchanged.
- `klauthed-security`: `SessionId` and `RefreshToken` no longer expose their raw
  bearer token via `Debug` — `{:?}` now redacts the secret (e.g. `SessionId(***)`),
  so an accidental `tracing::debug!(?session)` can't leak a live credential.

### Changed

- Every library crate now **denies** `clippy::unwrap_used`, `expect_used`,
  `panic`, and `indexing_slicing` in non-test code, so a bare `.unwrap()` /
  `.expect()` or a panicking index (`v[i]`, `&s[a..b]`) in shipping code is a hard
  error (not just under CI's `-D warnings`, but on any `cargo clippy`). The few
  unavoidable sites are `#[allow(..., reason = "…")]` with a proven invariant or a
  documented `# Panics` contract. As part of the sweep, the
  in-memory stores, caches, and registries (token denylist, sessions, OAuth
  client/code/refresh stores, idempotency, locks, outbox, rate limiter, job
  queue, audit/webhook sinks, tenancy resolver, CORS registries) now **recover
  from lock poisoning** instead of propagating a panic — a single panicked
  request can no longer wedge a shared in-memory store for the whole process.
- Every library crate now sets `#![deny(missing_docs)]`, so documentation of all
  public items (modules, types, fields, variants, traits, and functions) is
  CI-enforced workspace-wide. Previously only `klauthed-security` enforced this;
  the remaining gaps have been filled.
- `klauthed-error::ErrorCategory` and `klauthed-core::error::ConfigError` are now
  `#[non_exhaustive]` (forward-compatibility). Downstream `match`es on them must
  add a `_` arm.
- All crates now publish docs with `all-features` on docs.rs, so feature-gated
  APIs (database/cache/messaging backends, Redis rate limiter, …) are documented.
- Restructured every crate into folder modules (one concept per file), with
  integration tests under each crate's `tests/` and unit tests inline.
- Datetime handling moved from `chrono` to the `time` crate, encapsulated behind
  `klauthed_core::time`.
- CI bumped `actions/checkout` to v6 (Node 24).

[Unreleased]: https://github.com/klauthed/rust-libraries/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/klauthed/rust-libraries/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/klauthed/rust-libraries/releases/tag/v0.1.0
