# klauthed-security

Security primitives for klauthed — a focused toolkit built entirely on vetted
cryptographic crates (no hand-rolled primitives).

- **Password hashing** (`password`) — Argon2id PHC strings: `hash_password` /
  `verify_password`.
- **JWTs** (`jwt`) — `Claims` with `JwtSigner` / `JwtVerifier` (HS256, RS256, ES256, EdDSA), with
  `exp`/`iss`/`aud`/`nbf` validation driven by an injected `Clock`.
- **Secure random tokens** (`token`) — `random_token` / `random_bytes` from the OS CSPRNG.
- **Constant-time comparison** (`compare`) — `constant_time_eq` for secret/MAC equality.
- **AEAD encryption** (`aead`) — AES-256-GCM with a per-message random nonce.
- **Key derivation** (`kdf`) — HKDF-SHA256 subkey derivation.
- **API keys** (`apikey`) — high-entropy bearer credentials (SHA-256 verifier).
- **Sessions** (`session`) — opaque server-side sessions behind a `SessionStore` trait.
- **Authorization / RBAC** (`authz`) — permissions (with wildcards), roles, and an
  `Authorizer` policy checker.
- **MFA / TOTP** (`mfa`) — RFC 6238 one-time passwords.

### OAuth 2.0 / OIDC building blocks

- `authz_code` — single-use authorization codes + PKCE (plain/S256).
- `oauth2_client` — client registry (`ClientStore`): types, allowed grants, redirect URIs.
- `refresh_token` — rotating refresh tokens with family-based replay detection.
- `revocation` — a JWT `jti` denylist (`TokenDenylist`).

All fallible operations return `SecurityError`, which implements
`klauthed_error::DomainError` (stable `security.*` codes). HTTP endpoints that use these
primitives live in [`klauthed-web`](../klauthed-web/).

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-security --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
