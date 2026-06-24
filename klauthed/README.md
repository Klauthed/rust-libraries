# klauthed

Umbrella "starter" crate for the klauthed framework. Depend on this one crate and
turn on the pieces you need with features; each enabled library is re-exported as a
module (`klauthed::core`, `klauthed::web`, …), with the most common items in
`klauthed::prelude`.

```toml
# A typical actix-web service:
klauthed = { version = "0.10", features = ["web", "data", "observability", "security", "postgres"] }
```

## Feature map

| Feature | Re-exports / enables |
|---------|----------------------|
| `core` | `klauthed::core` (config, id, time, context, domain, cqrs, validation); implies `error` |
| `error` | `klauthed::error` (the `DomainError` kernel) |
| `macros` | `klauthed::macros` (`#[derive(DomainError)]`) |
| `data` | `klauthed::data` (db/cache/messaging/storage + outbox/idempotency/locks) |
| `web` | `klauthed::web` (actix `AppError`, context middleware, health, auth, OAuth2/OIDC) |
| `observability` | `klauthed::observability` (logging/metrics/otel) |
| `i18n` | `klauthed::i18n` (message catalogs) |
| `security` | `klauthed::security` (password hashing, JWT, tokens, …) |
| `platform` | `klauthed::platform` (tenancy, feature flags, audit, …) |
| `protocol` | `klauthed::protocol` (OIDC, SCIM) |
| `full` | all of the above |

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
