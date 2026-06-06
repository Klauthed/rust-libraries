# klauthed-platform

Cross-cutting platform concerns for klauthed services — data structures, SPI traits, and
in-memory implementations, each in its own module:

- **tenancy** — the `Tenant` model, `TenantStatus`, a `TenantResolver` trait, and a helper
  to read the tenant from a `RequestContext`.
- **featureflag** — a `FeatureFlag` key type, the `FeatureFlags` trait, and an
  `InMemoryFeatureFlags` provider with global defaults, per-tenant overrides, and
  multivariate values.
- **audit** — the `AuditEvent` record (with a builder), the `AuditSink` trait, an
  in-memory sink, and an optional SQL-outbox sink (feature `audit-outbox`).
- **jobs** — a background-job *store* abstraction: `JobStatus`, `EnqueuedJob`, the async
  `JobQueue` trait, and a clock-driven `InMemoryJobQueue` (queueing only — no worker).
- **webhooks** — `WebhookEndpoint`/`WebhookEvent` types, HMAC-SHA256 signing/verification,
  the `WebhookSender` trait, an in-memory recorder, and an optional HTTP sender
  (feature `webhook-http`).

All errors are reported via `PlatformError` (`impl DomainError`, codes `platform.*`).

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-platform --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
