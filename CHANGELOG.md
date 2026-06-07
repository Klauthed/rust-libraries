# Changelog

All notable changes to the klauthed Rust libraries are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the workspace adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
All crates share a single version and are released together.

## [Unreleased]

### Added

- `klauthed-security`: JWT signing/verification now supports **ES256** (ECDSA
  P-256) and **EdDSA** (Ed25519) in addition to HS256/RS256. Asymmetric keys load
  from PEM or DER (`{rs256,es256,eddsa}_{pem,der}` on `JwtSigner`/`JwtVerifier`).
- `klauthed-data`: new `rate_limit` module — a `RateLimiter` trait with a
  clock-injected `InMemoryRateLimiter` and a `RedisRateLimiter` (`redis` feature)
  for shared, cross-replica fixed-window limiting.
- `klauthed-web`: the rate-limit middleware now uses a pluggable `RateLimiter`
  store. `RateLimit::new` keeps the per-process in-memory limiter;
  `RateLimit::with_store` accepts any `Arc<dyn RateLimiter>` (e.g. Redis) for one
  global budget across replicas. The middleware **fails open** if the store
  errors, so a limiter outage cannot take the service down.
- Supply-chain CI gates: `cargo-deny` (RustSec advisories + license allow-list +
  source policy) and an MSRV (Rust 1.95) build job.
- crates.io publish metadata on every member crate (`description`, `keywords`,
  `categories`, workspace-inherited `license`/`repository`/`authors`, `readme`).

### Changed

- Restructured every crate into folder modules (one concept per file), with
  integration tests under each crate's `tests/` and unit tests inline.
- Datetime handling moved from `chrono` to the `time` crate, encapsulated behind
  `klauthed_core::time`.
- CI bumped `actions/checkout` to v6 (Node 24).

## [0.1.0]

- Initial workspace: `klauthed` umbrella plus `klauthed-core`, `-error`,
  `-macros`, `-i18n`, `-security`, `-protocol`, `-data`, `-platform`,
  `-observability`, `-web`, and `-testing`.

[Unreleased]: https://github.com/klauthed/rust-libraries/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/klauthed/rust-libraries/releases/tag/v0.1.0
