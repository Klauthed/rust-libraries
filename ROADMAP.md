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

## 0.5.0 (in progress)

Theme: **toward a stable 1.0 — API ergonomics and policy.**

- [x] Per-crate `prelude` modules re-exporting each crate's common types.
- API consistency pass (builder patterns, naming, re-export surface) — in
  progress: `#[must_use]` now on every builder method workspace-wide; naming and
  re-export-surface review ongoing.
- [x] GitHub Pages deployment for the mdBook guide (`Pages` workflow).
- [x] Committed SemVer + deprecation policy and an explicit MSRV policy
      (CONTRIBUTING.md "Stability policy").

## Toward 1.0

Broad API review per crate, committed SemVer + deprecation policy, MSRV policy,
and broad test / fuzz coverage.
