# klauthed — Capabilities Guide

A single, guided tour of everything the klauthed Rust libraries can do. Each
crate is a focused layer; together they cover the cross-cutting needs of a
backend / auth service. Start at the [crate map](#crate-map), then jump to the
layer you need.

- **Edition** 2024 · **MSRV** 1.95 · **License** MIT OR Apache-2.0
- Every library crate enforces `#![deny(missing_docs)]` and denies
  `clippy::{unwrap_used, expect_used, panic}` in non-test code — shipping code
  doesn't panic on fallible paths.
- Runnable tour of the highlights: `cargo run -p klauthed-examples`.

---

## Crate map

| Crate | One-liner |
|---|---|
| [`klauthed-error`](#klauthed-error) | The error kernel: `DomainError` trait + stable codes/categories |
| [`klauthed-macros`](#klauthed-macros) | Derives: `DomainError`, `FromConfig` |
| [`klauthed-core`](#klauthed-core) | Config, time/clock, context, ids, CQRS, domain, validation, wiring |
| [`klauthed-security`](#klauthed-security) | JWT, passwords, AEAD, MFA, passkeys, authz, OAuth2 primitives |
| [`klauthed-data`](#klauthed-data) | DB/cache/messaging/storage + outbox, idempotency, locks, rate limit, saga |
| [`klauthed-discovery`](#klauthed-discovery) | Service registry: in-memory, Consul, Eureka + self-registering agent |
| [`klauthed-protocol`](#klauthed-protocol) | Wire types: OAuth2, OIDC, JWKS, SCIM |
| [`klauthed-i18n`](#klauthed-i18n) | Localized message catalogs with fallback |
| [`klauthed-observability`](#klauthed-observability) | Tracing, metrics, OpenTelemetry wiring |
| [`klauthed-platform`](#klauthed-platform) | Tenancy, audit, webhooks, jobs, feature flags |
| [`klauthed-web`](#klauthed-web) | actix-web layer: middleware, extractors, OAuth2/OIDC endpoints |
| [`klauthed-testing`](#klauthed-testing) | Test utilities (dev-dependency) |
| [`klauthed`](#klauthed-umbrella) | Umbrella crate: feature-gated re-exports + prelude |

**Dependency direction:** `error` → `macros`/`core` → everything else. `core` is
the shared foundation; higher layers depend on it, never the reverse.

---

## klauthed-error

The kernel every other crate's errors plug into.

- **`DomainError`** trait — maps any error to a stable `ErrorCode` (`"config.missing_required"`),
  an `ErrorCategory`, an HTTP status, and a retryability flag.
- **`ErrorCategory`** — `BadRequest`, `Unauthorized`, `Forbidden`, `NotFound`,
  `Conflict`, `UnprocessableEntity`, `RateLimited`, `Timeout`, `Unavailable`,
  `Internal` (non-exhaustive). Each maps to an HTTP status + retry semantics.
- **`ErrorCode`** — a stable, serializable string code for clients/logs.

*Features:* `serde` (default) for serializing codes/categories.

---

## klauthed-macros

Procedural macros, kept deliberately small.

- **`#[derive(DomainError)]`** — generates the `DomainError` impl from
  `#[domain(prefix = "…", category = "…", code = "…")]`, with `transparent`
  delegation for `#[from]` wrappers and compile-time validation of code format.
- **`#[derive(FromConfig)]`** — binds a struct to a config section
  (`#[config(key = "database")]`, default = snake-cased type name;
  `#[config(default)]` = bind missing section to `Default`). The
  `@ConfigurationProperties` analog.

---

## klauthed-core

The shared foundation. Modules: `config`, `time`, `context`, `id`, `cqrs`,
`domain`, `validation`, `wiring`, `error`.

### config — layered, typed configuration
- **`Config`** loaded once at startup; read typed sections synchronously.
- **`ConfigBuilder`** + a **provider chain** that deep-merges (later wins):
  `EnvProvider`, `FileProvider` (TOML/JSON), `MemoryProvider`, `VaultProvider`
  (feature `vault`, KV v2 + Token/AppRole/Kubernetes auth), and
  **`ConfigServerProvider`** (feature `config-server`) — pulls config from a
  remote config server. Defaults to the **klauthed-native** format served by
  `klauthed-web`'s [config server](#klauthed-web); `spring_cloud()` / `RawJson`
  talk to a Spring Cloud Config Server or arbitrary JSON instead.
- **`Profile`** (local/dev/test/staging/prod) drives policy — staging/prod must
  source secrets from Vault, never files/env.
- **Typed sections** out of the box: `DatabaseConfig`, `CacheConfig`,
  `MessagingConfig`, `StorageConfig`, `ServerConfig` (`config.database()?`, …).
- **`FromConfig`** trait + derive for binding your own structs.
- **`ReloadableConfig`** (feature `hot-reload`) — re-resolves the chain on an
  interval / on demand, atomically swaps, and notifies subscribers on change.
  `start_with_refresh` also returns a **`RefreshTrigger`** for **push-refresh**:
  wire a config-server webhook, a discovery / message-bus event, or an HTTP
  `/refresh` endpoint to `trigger.refresh()` to reload immediately (coalesced).

### wiring — application assembly (Spring-style, Rust-idiomatic)
- **`AppContext`** — a type-keyed registry of shared singletons
  (`register` / `get` / `require`), bridged to config via
  `register_from_config::<T>(&config)`.
- **`Starter`** + **`AppBuilder`** — compose auto-config "starters" over a
  `Config` to wire an `AppContext`; `ConfigSectionsStarter` registers present
  typed sections. (No reflective DI — Rust has none — but the ergonomics of it.)

### the rest
- **time** — injectable `Clock` (`SystemClock`, `FixedClock`), UTC-canonical
  `Timestamp`, re-exported `Duration`; feature `tz` adds IANA zone conversion.
- **context** — per-request `RequestContext` (request id, correlation, tenant,
  locale); feature `task-local` adds ambient `current()` / `scope()`.
- **id** — phantom-typed 128-bit `Id<T>` (ULID/UUID-backed).
- **cqrs** — command / query / event bus traits.
- **domain** — domain-event / aggregate building blocks.
- **validation** — the `Validate` trait.
- **error** — `ConfigError`.

*Features:* `vault` (default), `config-server`, `hot-reload`, `task-local`, `tz`.

---

## klauthed-security

A focused toolkit over vetted crypto crates — no hand-rolled primitives.

- **JWT** (`jwt`) — `JwtSigner` / `JwtVerifier` for HS256, RS256, ES256, EdDSA
  (PEM or DER keys), with `exp`/`iss`/`aud`/`nbf` validation. Built on
  `jsonwebtoken` ≥10 (`aws_lc_rs` backend).
- **PASETO** (`paseto`, feature `paseto`) — v4.public (Ed25519) via
  `PasetoV4Signer` / `PasetoV4Verifier`, and v4.local (XChaCha20-Poly1305) via
  `PasetoV4Local`, sharing the same `Claims` as JWT. A misuse-resistant
  alternative (versioned protocol, no `alg` confusion). Built on the audited
  `pasetors`.
- **Passwords** (`password`) — Argon2id PHC `hash_password` / `verify_password`.
- **AEAD** (`aead`) — AES-256-GCM `encrypt`/`decrypt`, **envelope encryption**
  (`Envelope`, per-message data key + `rewrap` rotation), and **sealed-box**
  public-key encryption (feature `sealed`, X25519 ECIES).
- **MFA** (`mfa`) — TOTP (RFC 6238) + one-time **recovery codes**
  (`RecoveryCodeSet`).
- **Passkeys / WebAuthn** (`passkey`, feature `webauthn`) —
  `PasskeyAuthenticator` registration/auth ceremonies + `PasskeyStore`.
- **Authorization** (`authz`) — `Permission` (wildcards), `Role` (with
  inheritance), `RoleRegistry`, `Authorizer` (incl. resource-instance scoping).
- **OAuth2 server primitives** — authorization codes + PKCE (`authz_code`),
  client registry (`oauth2_client`), rotating refresh tokens with replay
  detection (`refresh_token`), revocation denylist (`revocation`).
- **Sessions** (`session`) — opaque server-side sessions behind `SessionStore`.
- **Tokens / KDF / compare / API keys** — CSPRNG `random_token`, HKDF
  `derive_key`, `constant_time_eq`, `generate_api_key` / `verify_api_key`.

*Features:* `sealed`, `webauthn`, `paseto`, `hibp`.

---

## klauthed-data

Backend data-access building blocks. Drivers are all feature-gated, so you pull
only what you use.

- **Databases** (`db`) — connection pools for SQL (`sqlx` Any; `postgres`,
  `mysql`, `sqlite`) and `mongodb`.
- **Migrations** (`sql`) — `Migrator` runs embedded, versioned SQL migrations
  (`Migration { version, name, sql }`) in order, each in its own transaction,
  tracked in a portable `_klauthed_migrations` table; safe to re-run (already
  applied versions are skipped).
- **Cache** (`cache`) — Redis (`redis`) and in-process (`cache-memory`, moka).
- **Messaging** (`messaging`) — NATS (`nats`), RabbitMQ (`rabbitmq`), Kafka
  (`kafka`).
- **Object storage** (`storage`) — S3 / GCS (`storage-gcs`) / Azure
  (`storage-azure`) / local, via `object_store`.
- **Transactional outbox** (`outbox`) — SQL + Mongo backends, with a polling
  **relay** that drains pending messages to a publisher.
- **Idempotency** (`idempotency`) — store-backed request de-duplication.
- **Distributed locks** (`locks`) — Redis + Mongo backends.
- **Rate limiting** (`rate_limit`) — in-memory + Redis (cross-replica, Lua).
- **Sagas** (`saga`), **event bus** (`eventbus`), **transactions**
  (`transaction`), and cursor-based **pagination** (`pagination`).

*Features:* `sql`, `postgres`, `mysql`, `sqlite`, `redis`, `cache-memory`,
`nats`, `rabbitmq`, `kafka`, `storage`, `storage-gcs`, `storage-azure`,
`mongodb`. *Live backends are exercised by the CI `integration` job.*

---

## klauthed-discovery

Service discovery: register instances and resolve peers, backend-agnostic.

- **`ServiceRegistry`** trait — `register` / `deregister` / `heartbeat` /
  `instances`, over a `ServiceInstance` (host/port/metadata).
- **Backends** — `InMemoryRegistry` (tests/single-process), `ConsulRegistry`
  (feature `consul`), `EurekaRegistry` (feature `eureka`), and `KubernetesRegistry`
  (feature `kubernetes`) — read-only discovery over the Endpoints API, with
  `in_cluster()` service-account config.
- **`ServiceAgent`** (feature `agent`) — registers on start, heartbeats in the
  background, deregisters on shutdown/drop.
- **`RoundRobin`** — lock-free client-side load balancing over resolved instances.

*Features:* `consul`, `eureka`, `kubernetes`, `agent`.

---

## klauthed-protocol

Strongly-typed wire messages for auth protocols (no I/O — the HTTP endpoints live
in `klauthed-web`).

- **OAuth2** (`oauth2`) — authorization/token requests & responses, error codes,
  introspection (RFC 7662) and revocation (RFC 7009), PKCE methods, token types.
- **OIDC** (`oidc`, feature `oidc`) — discovery metadata, ID-token claims.
- **JWKS** (`jwks`) — JSON Web Key Set types.
- **SCIM** (`scim`, feature `scim`) — provisioning resource types.

*Features:* `oidc`, `scim`.

---

## klauthed-i18n

Localized, interpolated messages.

- **`I18n`** holds one **`Catalog`** per **`Locale`**; resolves dotted keys
  (`validation.required`) with fallback **exact → primary language → default →
  key**.
- Built-in catalogs embedded for **en, de, es, fr, it, tr**; override messages or
  add locales at runtime (`add_catalog`, `load_dir`).
- **`Args`** for `{placeholder}` interpolation.

---

## klauthed-observability

Wiring for the three pillars.

- **Logging / tracing** — structured `tracing` setup.
- **Metrics** (feature `metrics`) — counters/gauges/histograms with a Prometheus
  exporter.
- **OpenTelemetry** (feature `otel`) — OTLP span export, plus a
  **`propagation`** module that carries W3C trace context across services:
  `extract` a parent context from inbound headers, `inject` / `inject_current`
  the active span into outbound request headers. Pairs with `klauthed-web`'s
  [`RequestTracing`](#klauthed-web) middleware for end-to-end distributed traces.

*Features:* `metrics`, `otel`.

---

## klauthed-platform

Higher-level platform services.

- **Tenancy** (`tenancy`) — tenant resolution / context.
- **Audit** (`audit`) — audit-log sink (feature `audit-outbox` routes through the
  data outbox).
- **Webhooks** (`webhooks`) — outbound webhook delivery (feature `webhook-http`).
- **Jobs** (`jobs`) — a background `JobQueue` plus a `JobWorker` that drains it
  (claim due jobs → run a `JobHandler` → mark each succeeded/failed). Durable
  backends: `SqlJobQueue` (feature `jobs-sql`; SQLite/Postgres/MySQL, with a
  Postgres `FOR UPDATE SKIP LOCKED` claim) and `RedisJobQueue` (feature
  `jobs-redis`; atomic Lua claim) — both interchangeable with the in-memory queue.
- **Scheduler** (feature `scheduler`) — in-process recurring work on fixed
  intervals or cron schedules (UTC or a named IANA timezone, DST-aware); a panic
  in one run is isolated. Pairs with `JobWorker` for periodic queue draining.
- **Metering** (`metering`) — per-tenant usage accounting (`Meter`) for quotas
  and usage-based billing.
- **Notifications** (`notifications`) — user-facing messages (`Notifier`: email /
  SMS / push), distinct from webhooks.
- **Feature flags** (`featureflag`) — runtime flag evaluation.

*Features:* `audit-outbox`, `webhook-http`, `scheduler`.

---

## klauthed-web

The actix-web HTTP layer shared by services.

- **Middleware** — `RequestContextMiddleware` (per-request context),
  `SecurityHeaders` (HSTS/CSP/X-Frame/…), `Csrf` (double-submit-cookie),
  `RateLimit`, CORS (static `build_cors` + dynamic `DynamicCors`), `JwtAuth`,
  and `RequestTracing` (feature `otel`) — opens an OpenTelemetry span per
  request, linking it to the caller's trace via the inbound W3C `traceparent`.
- **Config server** (feature `config-server`) — `ConfigServer` turns the service
  *into* a config server (a Rust-native alternative to Spring Cloud Config
  Server): mount it and it answers `GET /{application}/{profile}[/{label}]` with
  the merged config tree from a `ConfigSource` (directory of TOML/JSON, or
  in-memory). Clients point a `ConfigServerProvider` at it (see
  [klauthed-core](#klauthed-core)).
- **Starter** — `WebStarter` assembles the actix `Components` (pools, middleware)
  from an `AppContext`, completing the Spring-style auto-config story.
- **OpenAPI** (feature `openapi`) — generate an OpenAPI 3.1 spec with `utoipa`:
  the built-in endpoints (health probes) ship annotated as `base_openapi()`,
  `serve_spec` exposes the JSON, and `utoipa` is re-exported so a service merges
  its own annotated paths into one document. Feature `swagger-ui` adds
  `serve_swagger_ui` — an interactive Swagger UI with assets vendored into the
  binary (no build-time or runtime network access).
- **Config refresh** (feature `config-refresh`) — `refresh::serve_refresh` mounts
  a `POST /refresh` endpoint that drives a `klauthed_core` `RefreshTrigger` to
  push-reload configuration live (the Spring `/actuator/refresh` analog).
- **Passkeys / WebAuthn** (feature `webauthn`) — `PasskeyApi` mounts the four
  ceremony routes (`register`/`login` × `start`/`finish`) over
  `klauthed-security`'s SPI, with a `CeremonyStore` (+ `InMemoryCeremonyStore`)
  for in-flight state. Verified end-to-end against a software authenticator.
- **Extractors** (`extract`) — `Json` and `Validated` bodies that surface
  deserialization / `Validate` failures as `AppError`.
- **Errors** (`error`) — `AppError` absorbs any `DomainError` and renders a
  uniform HTTP response.
- **Health** (`health`) — liveness/readiness with a pluggable `HealthCheck`
  registry (SQL/Redis checks behind `data-sql` / `data-redis`).
- **OAuth2/OIDC endpoints** (`oauth`) — authorize/token/userinfo, discovery, JWKS.
- **Server** (`server`) — bind an `HttpServer` from `ServerConfig` with the
  common middleware/health pre-wired.
- **Resilience** (`resilience`) — `RetryPolicy` (capped exponential backoff over
  an async operation) and a `CircuitBreaker` (Closed → Open → HalfOpen with an
  injected `Clock`) for hardening outbound calls.
- **Metrics endpoint** (feature `metrics`) — `serve_metrics` mounts a Prometheus
  `GET /metrics` scrape endpoint backed by `klauthed-observability`.

*Features:* `context-scope`, `data-sql`, `data-redis`, `config-server`, `otel`,
`openapi`, `swagger-ui`, `config-refresh`, `webauthn`, `metrics`.
*See the runnable `auth_service` example: `cargo run -p klauthed-web --example auth_service`.*

---

## klauthed-testing

Dev-dependency test utilities: `assert_category` / `assert_code` and friends
(`assertions`), a controllable test `clock`, `context` builders, error
helpers, deterministic `ids`, and an in-memory `repository`.

---

## klauthed (umbrella)

A single dependency that re-exports the libraries behind feature flags, plus a
`prelude`. Enable per-area features (`security`, `data`, `web`, `observability`,
`platform`, `protocol`, …) or `full`. Driver/area sub-features
(`postgres`, `redis`, `otel`, …) are forwarded to the underlying crates.

---

## Cross-cutting guarantees

- **Uniform errors** — every error implements `DomainError`, so HTTP status,
  retryability, and stable codes are consistent across layers; `AppError`
  renders them.
- **Config-first** — components bind from `Config` (typed sections / `FromConfig`)
  and can be wired via `AppContext` + starters; secrets always come from Vault in
  staging/prod.
- **Pay-for-what-you-use** — heavy/optional dependencies (DB drivers, Vault,
  OpenSSL-backed WebAuthn, OTel) are behind cargo features; the default build
  stays lean.
- **Quality gates (CI):** rustfmt, clippy `-D warnings`, tests, docs
  `-D warnings`, cargo-deny (advisories/licenses/bans), **OSV-Scanner**
  (catches GHSA-only advisories the RustSec DB misses), MSRV 1.95, per-feature
  build check, and a live-infra integration job (Postgres/Redis/Mongo
  containers).
