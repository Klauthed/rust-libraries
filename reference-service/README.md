# reference-service

A small, runnable **reference service** that wires the klauthed crates together
end to end — a starting template that shows the recommended layout, not a
product. It is part of the workspace but **not published**.

What it dogfoods:

- **`klauthed-core`** — profile-aware configuration loaded once at startup
  (`ConfigBuilder` + `ServerConfig`), and the injectable `Clock` for token expiry.
- **`klauthed-observability`** — telemetry initialised from a `TelemetryConfig`.
- **`klauthed-web`** — bound via `server::serve_with_defaults` (request-context
  middleware + health probes pre-wired), the `JwtAuth` middleware, the
  `AuthenticatedUser` extractor, and the uniform `AppError` response model.
- **`klauthed-security`** — `JwtSigner` / `JwtVerifier` for HS256 tokens.

## Endpoints

| Method & path | Description |
|---------------|-------------|
| `GET /health`, `GET /health/ready` | liveness / readiness (framework) |
| `POST /login` | issue a 1-hour HS256 JWT for a (demo) `{ "username": … }` |
| `GET /api/me` | JWT-protected; returns the authenticated subject |

## Run it

```sh
cargo run -p reference-service

# in another shell:
TOKEN=$(curl -s localhost:8080/login -d '{"username":"alice"}' \
  -H 'content-type: application/json' | jq -r .token)
curl -s localhost:8080/api/me -H "authorization: Bearer $TOKEN"   # {"sub":"alice"}
curl -s localhost:8080/api/me                                     # 401
```

The end-to-end flow (login → protected route, plus health and the unauthenticated
case) is covered by the crate's tests. The JWT secret is hard-coded here for the
demo; a real service sources it from configuration / Vault, and a data layer plugs
in via `klauthed-data` (see the `data` and migration demos in `examples/`).
