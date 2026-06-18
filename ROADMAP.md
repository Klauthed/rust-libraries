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
- [x] PASETO tokens — `klauthed-security` `paseto` feature, sharing the JWT
      `Claims`: v4.public (Ed25519) `PasetoV4Signer`/`PasetoV4Verifier` and
      v4.local (XChaCha20-Poly1305) `PasetoV4Local`.
- [x] Swagger UI — `klauthed-web` `swagger-ui` feature: `serve_swagger_ui`,
      assets vendored (no network at build/run).
- [x] Config push-refresh — `ReloadableConfig::start_with_refresh` + a clonable
      `RefreshTrigger` (`refresh()` reloads immediately, coalesced). Any event
      source (config-server webhook, discovery / bus event, HTTP `/refresh`)
      drives it. (A built-in web `/refresh` endpoint is a possible follow-up.)
- Kubernetes discovery backend.
- Actix passkey (WebAuthn) HTTP endpoints in `klauthed-web`.

**Docs / adoption**
- mdBook guide (architecture, getting started, per-area how-tos).
- A reference service dogfooding the full suite end-to-end.

## Toward 1.0

API review per crate (preludes, consistent builders), committed SemVer +
deprecation policy, MSRV policy, and broad test/fuzz coverage.
