# klauthed-web

The HTTP layer every klauthed service shares, built on [actix-web](https://actix.rs/).

- **error** — `AppError`, the aggregate error handlers return. It absorbs any
  `klauthed_error::DomainError` and renders a uniform HTTP response via `ResponseError`.
- **context** — `RequestContextMiddleware` establishes a `RequestContext` per request from
  inbound headers; the `Context` extractor hands it to handlers.
- **auth** — `JwtAuth` Bearer middleware + `AuthenticatedUser` / `OptionalAuthentication`
  extractors, with an optional revocation-denylist check.
- **cors** — static `build_cors` plus a dynamic, registry-backed `DynamicCors` for
  multi-tenant origin allow-lists.
- **health** — liveness/readiness endpoints with a pluggable `HealthCheck` registry, and a
  `Components` app builder.
- **server** — bind an actix `HttpServer` from a `ServerConfig`, pre-wiring context
  middleware and health endpoints.
- **ratelimit** — an in-memory fixed-window rate-limit middleware (`429` + `Retry-After`).
- **extract** — `Json` / `Validated` body extractors that surface failures as `AppError`.
- **oauth** — the OAuth2/OIDC server endpoints: `/oauth/authorize`, `/oauth/token`
  (with `id_token`), `/oauth/revoke`, `/oauth/introspect`, `/oauth/userinfo`,
  `/oauth/jwks`, and `/.well-known/openid-configuration`. Stores and the
  `UserInfoProvider` are SPI traits the service implements.
- **config_server** (feature `config-server`) — `ConfigServer` turns the service
  *into* a config server (a Rust-native alternative to Spring Cloud Config
  Server). Mount it and it serves `GET /{application}/{profile}[/{label}]` from a
  `ConfigSource` (a directory of TOML/JSON files, or in-memory); clients point a
  `klauthed_core` `ConfigServerProvider` at it.
- **trace** (feature `otel`) — `RequestTracing` middleware opens an
  OpenTelemetry span per request and links it to the caller's trace via the
  inbound W3C `traceparent`, exporting through `klauthed-observability`'s pipeline.
- **starter** — `WebStarter` assembles the actix `Components` (pools + common
  middleware) from an `AppContext`, the web half of the Spring-style auto-config.
- **openapi** (feature `openapi`) — generate an OpenAPI 3.1 spec with `utoipa`:
  built-in endpoints ship annotated (`openapi::base_openapi`), `serve_spec`
  exposes the JSON, and `utoipa` is re-exported so services merge their own paths.

Optional `data-sql` / `data-redis` features add ready-made health checks and rate-limit
stores.

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-web --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
