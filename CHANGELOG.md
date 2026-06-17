# Changelog

All notable changes to the klauthed Rust libraries are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the workspace adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
All crates share a single version and are released together.

## [Unreleased]

### Added

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
