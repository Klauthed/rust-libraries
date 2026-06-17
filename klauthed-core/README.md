# klauthed-core

Foundational primitives shared by every klauthed service.

- **config** — profile-driven configuration. A `Profile` (Local/Dev/Test/Staging/Prod)
  governs which sources are allowed; staging/prod must use Vault (enforced in the
  builder). `ConfigProvider` is a **trait** (file, env, and a built-in Vault client over
  reqwest behind the `vault` feature), so a service can register its own sources. The
  `config-server` feature adds `ConfigServerProvider`, which pulls config from a remote
  config server — defaulting to the klauthed-native format served by `klauthed-web`'s
  config server, with `spring_cloud()` / `RawJson` modes for other servers.
- **wiring** — Spring-style application assembly: `AppContext` (a type-keyed registry of
  shared singletons) plus async `Starter` / `AppBuilder` auto-config, so crates contribute
  resources (pools, clients) to one composed context.
- **time** — time as an injectable dependency: components take a `Clock` (`SystemClock`
  in production, `FixedClock` in tests). `Timestamp`/`Duration` are the canonical instant
  and span types, backed by the [`time`](https://docs.rs/time) crate and fully
  encapsulated here.
- **id** — typed identifiers (UUID v4/v7, ULID).
- **validation** — a `Validate` trait + structured validation errors.
- **context** — `RequestContext` (request id, principal, tenant, deadline), optionally
  ambient via a tokio task-local (feature `task-local`).
- **domain** / **cqrs** — building blocks for domain entities and command/query handlers.

Fallible operations return `ConfigError` (and friends), which implement
`klauthed_error::DomainError`.

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-core --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
