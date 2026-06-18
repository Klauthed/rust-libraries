# Architecture

klauthed is a **workspace of small, focused crates** layered so that a leaf crate
never depends on a heavier one. The shape and a few deliberate design decisions:

## Layering

```text
        klauthed (umbrella, feature flags)
                     │
   ┌─────────────────┼───────────────────────────────┐
 web   data   security   discovery   observability   platform   protocol   i18n
   └─────────────────┼───────────────────────────────┘
                     │
                 klauthed-core   (config, time, ids, context, wiring, validation)
                     │
                 klauthed-error  (the error kernel)   ◄── klauthed-macros (leaf)
```

Every crate publishes independently and shares one workspace version; they are
released together.

## The error kernel

`klauthed-error` is a **kernel, not a warehouse**. It defines the `DomainError`
trait and a small category/code taxonomy (`bad_request`, `unauthorized`,
`not_found`, `internal`, …). Each crate defines *its own* error enum and derives
`DomainError` (via `klauthed-macros`) with stable `area.reason` codes — e.g.
`security.expired_token`. Nothing imports a giant shared error type; the web layer
maps any `DomainError` to a uniform HTTP response.

## Configuration is a trait

`ConfigProvider` is a **trait**, not an enum, so a service can register its own
sources alongside the built-ins (env, file, Vault, config server). Providers yield
a `ConfigMap` that the builder deep-merges (later wins). A `Profile`
(Local/Dev/Test/Staging/Prod) governs which sources are allowed — staging and prod
*must* source secrets from Vault, enforced in the builder. See
[Configuration](configuration.md).

## Time is an injected dependency

All datetime handling is encapsulated behind `klauthed_core::time` (built on the
[`time`](https://docs.rs/time) crate, not chrono, and kept swappable). Components
take a `Clock` — `SystemClock` in production, `FixedClock` in tests — so
time-dependent logic (token expiry, TTLs) is deterministic under test.

## Spring-style wiring, Rust-idiomatic

There is no reflective dependency injection — Rust has none — but klauthed offers
the *ergonomics* of it:

- **`FromConfig`** (derive) binds a config section to a typed struct, like
  `@ConfigurationProperties`.
- **`AppContext`** is a type-keyed registry of shared singletons (`register` /
  `get` / `require`).
- **`Starter`** + **`AppBuilder`** compose async auto-configuration: a starter
  reads config and contributes live resources (a pool, a client) to the
  `AppContext`. `DataStarter` and `WebStarter` wire the data and web layers.

## No panics in library code

Every library crate denies `unwrap`, `expect`, `panic!`, and indexing-slicing
outside tests (`#![cfg_attr(not(test), deny(...))]`), and denies missing docs.
Fallible paths return typed errors; CI enforces it with `-D warnings`, a feature
powerset check, an MSRV check, `cargo-deny`, an OSV scan, a coverage floor, and
live-infra integration tests. Untrusted parsers additionally have `cargo-fuzz`
targets run on a nightly schedule.
