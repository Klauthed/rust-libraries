# klauthed — Rust libraries

[![CI](https://github.com/Klauthed/rust-libraries/actions/workflows/ci.yml/badge.svg)](https://github.com/Klauthed/rust-libraries/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A Cargo workspace of `klauthed-*` crates that give Rust services **Spring-Boot-like
ergonomics** — typed configuration, a uniform error model, pre-built data/security/web
plumbing, and observability — so each service writes domain logic instead of boilerplate.

> **Status:** pre-1.0 and under active development. APIs may change between releases.
> Built for [actix-web](https://actix.rs/) services.

📖 **[CAPABILITIES.md](CAPABILITIES.md)** — a single, guided tour of every crate's
capabilities, features, and entry points. · 🗺️ **[ROADMAP.md](ROADMAP.md)** — what's
next.

## Crates

| Crate | What it provides |
|-------|------------------|
| [`klauthed`](klauthed/) | Umbrella "starter" crate: depend on one crate, enable pieces via features; re-exports each library as a module + a `prelude`. |
| [`klauthed-error`](klauthed-error/) | The zero-dependency **error kernel**: `ErrorCategory`, `ErrorCode`, and the `DomainError` trait shared by every crate. |
| [`klauthed-core`](klauthed-core/) | Foundations: profile-driven config (custom `ConfigProvider` + Vault), ids, injectable time (`Clock`/`Timestamp`), validation, `RequestContext`, domain & CQRS building blocks. |
| [`klauthed-macros`](klauthed-macros/) | Proc-macros (leaf crate). Currently `#[derive(DomainError)]`. |
| [`klauthed-i18n`](klauthed-i18n/) | Internationalized message catalogs with graceful locale fallback; ships embedded en/de/es/fr/it/tr defaults. |
| [`klauthed-observability`](klauthed-observability/) | Structured logging/tracing, Prometheus metrics, and OpenTelemetry export from one `TelemetryConfig`. |
| [`klauthed-protocol`](klauthed-protocol/) | Spec-accurate serde wire types for OIDC, JWKS, OAuth 2.0, and SCIM (typed data modeling — no I/O). |
| [`klauthed-security`](klauthed-security/) | Crypto & auth primitives: Argon2id password hashing, JWT (HS256/RS256), AEAD, HKDF, MFA/TOTP, sessions, RBAC, and the OAuth2/OIDC building blocks. |
| [`klauthed-data`](klauthed-data/) | Connected resources from typed config: SQL/Mongo pools, Redis/in-memory cache, NATS/RabbitMQ/Kafka, object storage, plus outbox, idempotency, locks, sagas, pagination & event bus. |
| [`klauthed-discovery`](klauthed-discovery/) | Service discovery: a `ServiceRegistry` (in-memory, Consul, Eureka) with a self-registering agent and round-robin client-side load balancing. |
| [`klauthed-platform`](klauthed-platform/) | Cross-cutting concerns: tenancy, feature flags, audit, background-job stores, and webhooks (SPI traits + in-memory impls). |
| [`klauthed-web`](klauthed-web/) | The actix-web layer: `AppError`, request-context middleware, health probes, server builder, rate limiting, extractors, JWT auth, CORS, and the OAuth2/OIDC endpoints. |
| [`klauthed-testing`](klauthed-testing/) | Dev-dependency test utilities (fixed clock, assertions, deterministic ids, context/repository helpers). |

## Quick start

Most services depend on the umbrella crate and turn on the pieces they need:

```toml
[dependencies]
klauthed = { version = "0.1", features = ["web", "data", "security", "observability", "postgres"] }
```

Each enabled library is re-exported as a module (`klauthed::web`, `klauthed::data`, …),
with the most common items in `klauthed::prelude`. You can also depend on individual
`klauthed-*` crates directly.

## Design highlights

- **Error kernel, not a warehouse.** `klauthed-error` owns only the shared contract
  (`DomainError`: `category()` + stable `domain.reason` `code()`); each crate defines
  its own error enum and `impl DomainError` (usually via `#[derive(DomainError)]`).
- **Profile-driven config.** A `Profile` (Local/Dev/Test/Staging/Prod) governs sources;
  staging/prod must use Vault. `ConfigProvider` is a trait, so services can register
  their own sources.
- **Injectable time.** Code takes a `Clock` (`SystemClock` in prod, `FixedClock` in
  tests). `Timestamp`/`Duration` are backed by the [`time`](https://docs.rs/time) crate
  and fully encapsulated behind `klauthed_core::time`.
- **SPI traits everywhere.** Stores and registries (`ClientStore`, `AuthCodeStore`,
  `RefreshTokenStore`, `TokenDenylist`, `UserInfoProvider`, `CorsOriginRegistry`, …) are
  traits the service implements against its own data layer; the libraries ship in-memory
  implementations for tests and single-node use.

## Development

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). Run the same
checks CI enforces:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

Browse a crate's API docs locally with `cargo doc -p klauthed-<crate> --open`.

See [CONTRIBUTING.md](CONTRIBUTING.md) for details and [SECURITY.md](SECURITY.md) to
report vulnerabilities.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution you submit for
inclusion in the work shall be dual licensed as above, without additional terms.
