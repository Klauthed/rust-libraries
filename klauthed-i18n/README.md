# klauthed-i18n

Internationalized messages for klauthed services.

An `I18n` holds one `Catalog` per `Locale` and resolves a dotted message key
(`validation.required`) to a localized, interpolated string. Lookup falls back
**exact locale → primary language → default locale → the key itself**, so a missing
translation degrades gracefully.

The framework ships default catalogs (en, de, es, fr, it, tr) embedded in the binary;
services can override individual messages or add locales at runtime.

```rust
use klauthed_i18n::{Args, I18n, Locale};

let i18n = I18n::with_embedded_defaults();
let msg = i18n.translate_with(
    &Locale::new("tr"),
    "validation.required",
    &Args::new().set("field", "e-posta"),
);
assert_eq!(msg, "e-posta alanı zorunludur.");
```

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-i18n --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
