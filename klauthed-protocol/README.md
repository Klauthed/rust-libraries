# klauthed-protocol

Spec-accurate serde data types for the identity protocols klauthed speaks.

This crate is *typed data modeling*: it defines the wire shapes (field names matching the
relevant specs exactly) so the rest of the system can serialize/deserialize them. It
implements **no** network I/O, OAuth flows, token-validation crypto, or HTTP clients —
those belong to other crates (JWT signing/verification lives in `klauthed-security`).

Protocol families live behind independent features (all on by default):

| Module | Covers | Feature |
|--------|--------|---------|
| `oidc` | OpenID Connect discovery metadata, claim types, and **claim-level** ID-token validation | `oidc` |
| `jwks` | JSON Web Key / Key Set types (RFC 7517) and key lookup | `oidc` |
| `oauth2` | OAuth 2.0 message types (RFC 6749) + revocation/introspection (RFC 7009/7662) | `oauth2` (implies `oidc`) |
| `scim` | SCIM 2.0 core resource types (RFC 7643/7644) | `scim` |

Parse/validation failures surface as `ProtocolError` (`impl DomainError`). ID-token
*signature* verification, JWKS fetching, JWT decoding, OAuth flow execution, and HTTP
transport are intentionally out of scope.

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-protocol --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
