# klauthed

**klauthed** is a Cargo workspace of `klauthed-*` crates that give Rust services
**Spring-Boot-like ergonomics** — typed configuration, a uniform error model,
pre-built data / security / web plumbing, and observability — so each service
writes domain logic instead of boilerplate.

> **Status:** pre-1.0 and under active development. APIs may change between
> releases. Built for [actix-web](https://actix.rs/) services.

## Why klauthed

A typical Rust service re-implements the same scaffolding every time: load layered
config, wire a database pool, render errors consistently as HTTP, sign and verify
tokens, expose health checks, set up tracing. klauthed factors that out into small,
focused, feature-gated crates you compose — you depend on the umbrella `klauthed`
crate (or individual crates) and turn on only what you use.

The design borrows Spring Boot's *ergonomics* — typed configuration properties,
auto-configuration "starters", a shared application context — but stays
Rust-idiomatic: no reflection, no runtime magic, everything is explicit traits and
cargo features.

## The crates

| Crate | What it provides |
|-------|------------------|
| `klauthed` | Umbrella "starter" crate — depend on one crate, enable pieces via features. |
| `klauthed-core` | Typed config, ids, clock/time, request context, CQRS, domain, validation, wiring. |
| `klauthed-error` | The error kernel: `DomainError` trait + category/code taxonomy. |
| `klauthed-data` | DB / cache / messaging / storage connectors, outbox, idempotency, locks, migrations. |
| `klauthed-security` | JWT & PASETO, AEAD, MFA, WebAuthn, passwords, OAuth2 primitives. |
| `klauthed-web` | actix-web layer: middleware, extractors, OAuth2/OIDC endpoints, health, OpenAPI. |
| `klauthed-discovery` | Service registry abstraction (in-memory, Consul, Eureka, Kubernetes). |
| `klauthed-observability` | Tracing, Prometheus metrics, OpenTelemetry export. |
| `klauthed-protocol` | Spec-accurate OAuth2 / OIDC / SCIM / JWKS wire types. |
| `klauthed-platform` | Tenancy, audit, webhooks, jobs, feature flags. |
| `klauthed-i18n` | Locales, message bundles, formatting. |
| `klauthed-macros` | Proc-macros (e.g. `#[derive(DomainError)]`, `FromConfig`). |
| `klauthed-testing` | Dev-dependency test utilities. |

For a dense, single-page tour of every crate's entry points see
[**CAPABILITIES.md**](https://github.com/Klauthed/rust-libraries/blob/master/CAPABILITIES.md)
in the repository; this guide is the narrative companion.

## Licence

Dual-licensed under MIT or Apache-2.0, at your option.
