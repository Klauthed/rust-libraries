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

## 0.8.0 (in progress)

Theme: **continue toward a stable 1.0.**

- API consistency: naming conventions and re-export-completeness review per crate
  (the `#[must_use]` builder pass landed in 0.5.0).
- Broaden test / fuzz / property coverage on the remaining surface.
- Candidate features as they arise.

## Toward 1.0

A final broad API review per crate, the committed SemVer + deprecation policy now
in place (CONTRIBUTING.md), the MSRV policy, and broad test / fuzz coverage.
