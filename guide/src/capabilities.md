# Capabilities by crate

A guided map of what each crate offers. For the exhaustive entry-point list see
[CAPABILITIES.md](https://github.com/Klauthed/rust-libraries/blob/master/CAPABILITIES.md);
for full API docs, each crate is on [docs.rs](https://docs.rs).

## klauthed-core

The shared foundation: layered [configuration](configuration.md), an injectable
`Clock` and UTC-canonical `Timestamp`, phantom-typed `Id<T>` (UUID/ULID), a
`Validate` trait, per-request `RequestContext`, CQRS and domain building blocks,
and the `AppContext` / `Starter` wiring.

## klauthed-security

A toolkit over vetted crypto crates — no hand-rolled primitives:

- **JWT** — `JwtSigner` / `JwtVerifier` (HS256/RS256/ES256/EdDSA).
- **PASETO** (feature `paseto`) — v4.public (Ed25519) and v4.local
  (XChaCha20-Poly1305), sharing the same `Claims` as JWT.
- **AEAD** — AES-256-GCM, envelope encryption, sealed boxes (feature `sealed`).
- **Passwords** — Argon2id; optional HIBP breach check (feature `hibp`).
- **MFA** — TOTP + recovery codes. **Passkeys/WebAuthn** (feature `webauthn`).
- **OAuth2 server primitives** — auth codes + PKCE, client registry, rotating
  refresh tokens, a revocation denylist.

## klauthed-data

Feature-gated connectors and reliability patterns: SQL (`sqlx`), Mongo, Redis,
NATS/RabbitMQ/Kafka, object storage; plus a **migration runner**, transactional
**outbox**, **idempotency**, distributed **locks**, **rate limiting**, **sagas**,
and cursor **pagination**.

## klauthed-web

The actix-web layer: context / security-headers / CSRF / CORS / rate-limit /
`JwtAuth` middleware, validating extractors, a uniform `AppError`, health probes,
the OAuth2/OIDC server endpoints, **OpenAPI generation** + **Swagger UI**, a
**`POST /refresh`** config endpoint, and **passkey HTTP endpoints** (feature
`webauthn`).

## klauthed-discovery

A `ServiceRegistry` (register / deregister / heartbeat / instances) with
in-memory, **Consul**, **Eureka**, and **Kubernetes** (read-only, Endpoints API)
backends, a `ServiceAgent` for lifecycle, and lock-free `RoundRobin` balancing.

## klauthed-observability

Structured tracing, Prometheus **metrics**, and **OpenTelemetry** OTLP export with
W3C trace-context propagation — paired with klauthed-web's `RequestTracing`
middleware for end-to-end distributed traces.

## The rest

- **klauthed-protocol** — spec-accurate OAuth2 / OIDC / SCIM / JWKS wire types.
- **klauthed-platform** — tenancy, audit, webhooks, jobs, feature flags.
- **klauthed-i18n** — locales, message bundles, formatting.
- **klauthed-error** — the `DomainError` kernel (see [Architecture](architecture.md)).
- **klauthed-testing** — assertions, a controllable clock, builders, in-memory repos.
