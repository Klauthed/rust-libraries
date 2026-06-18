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

## 0.4.0 (in progress)

Theme: **finish the discovery/auth surface and invest in adoption.**

**Features**
- Kubernetes discovery backend (`kube`) — live-cluster integration-tested,
  alongside the existing Consul/Eureka backends.
- Actix passkey (WebAuthn) HTTP endpoints in `klauthed-web` — the ceremony
  routes over the existing `klauthed-security` SPI (needs a ceremony-state-store
  + post-auth design).
- More fuzz targets (JWK, full OIDC/SCIM) on the existing harness.

**Docs / adoption**
- mdBook guide (architecture, getting started, per-area how-tos).
- A reference service dogfooding the full suite end-to-end.

## Toward 1.0

API review per crate (preludes, consistent builders), committed SemVer +
deprecation policy, MSRV policy, and broad test/fuzz coverage.
