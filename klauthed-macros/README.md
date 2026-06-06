# klauthed-macros

Procedural macros for klauthed (a leaf crate).

Currently provides `#[derive(DomainError)]`, which generates the
`klauthed_error::DomainError` impl from `#[domain(...)]` attributes so error enums don't
hand-write the `category()` / `code()` match arms:

```rust,ignore
#[derive(Debug, DomainError)]
#[domain(prefix = "security", category = "internal")]
enum SecurityError {
    #[domain(category = "unauthorized", code = "expired_token")]
    ExpiredToken,
    // default category = "internal", default code = snake_case(variant)
    Rng,
}
```

Codes are emitted as `"<prefix>.<reason>"`; the derive correctly snake-cases acronyms
(`HTTPError` → `http_error`) and forwards `#[cfg]` on variants.

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-macros --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
