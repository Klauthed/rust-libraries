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

## 0.3.0 (in progress)

Theme: **harden for 1.0 and round out the web/adoption surface.**

**Assurance** — done:
- [x] Fuzz targets for untrusted parsers — JWT decode, AEAD decrypt, OAuth2
      token-response deserialization, config tree (`fuzz/`, nightly CI).
      More targets (JWK, full OIDC/SCIM) can be added to the same harness.
- [x] Property tests for invariants (config merge, pagination cursors, ids).
- [x] Coverage gate (`cargo-llvm-cov`, line floor in CI) + criterion benchmarks
      on hot paths (config merge/expand, ids, cursors, JWT, AEAD).

**Features**
- [x] OpenAPI generation (`utoipa`) — `klauthed-web` `openapi` feature: annotated
      built-in endpoints + spec serving; services merge their own paths.
- [x] PASETO tokens — `klauthed-security` `paseto` feature: v4.public (Ed25519)
      `PasetoV4Signer`/`PasetoV4Verifier`, sharing the JWT `Claims`. (v4.local TBD.)
- Swagger UI bundling on top of the `openapi` feature.
- Discovery ↔ config push-refresh (bus event → `ReloadableConfig::reload_now`).
- Kubernetes discovery backend.
- Actix passkey (WebAuthn) HTTP endpoints in `klauthed-web`.

**Docs / adoption**
- mdBook guide (architecture, getting started, per-area how-tos).
- A reference service dogfooding the full suite end-to-end.

## Toward 1.0

API review per crate (preludes, consistent builders), committed SemVer +
deprecation policy, MSRV policy, and broad test/fuzz coverage.
