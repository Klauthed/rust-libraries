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

## 0.2.0 (ready to release)

Theme: **finish the auto-config / observability story, close the migrations gap,
and a security quick-win.**

- [x] Umbrella crate fronts everything (`discovery` + newer feature pass-throughs).
- [x] Trusted Publishing (OIDC) — tokenless releases.
- [x] **Resource starters** — `DataStarter` (pool from `DatabaseConfig`) and
      `WebStarter` (assembles the web `Components` from wired resources) so
      `AppBuilder` auto-config reaches live components. (Made `Starter`/`AppBuilder`
      async.)
- [x] **OTEL span auto-instrumentation** — `RequestTracing` middleware (span per
      web request) + W3C trace-context propagation (klauthed-observability::propagation).
- [x] **DB migrations runner** — embedded, versioned migrations in `klauthed-data` (`migrate::Migrator`, over `AnyPool`).
- [x] **HIBP breach check** — k-anonymity password check in `klauthed-security`
      (`password::hibp`, feature-gated).

## Backlog (0.3.0+ / parallel tracks)

**Assurance**
- [x] Fuzz targets for untrusted parsers — JWT decode, AEAD decrypt, OAuth2
      token-response deserialization, config tree (`fuzz/`, nightly CI).
      More targets (JWK, full OIDC/SCIM) can be added to the same harness.
- [x] Property tests for invariants (config merge, pagination cursors, ids).
- Coverage gate (`cargo-llvm-cov`) + criterion benchmarks on hot paths.

**Features**
- Discovery ↔ config push-refresh (bus event → `ReloadableConfig::reload_now`).
- Kubernetes discovery backend.
- Actix passkey (WebAuthn) HTTP endpoints in `klauthed-web`.
- PASETO tokens; OpenAPI generation (utoipa).

**Docs / adoption**
- mdBook guide (architecture, getting started, per-area how-tos).
- A reference service dogfooding the full suite end-to-end.

## Toward 1.0

API review per crate (preludes, consistent builders), committed SemVer +
deprecation policy, MSRV policy, and broad test/fuzz coverage.
