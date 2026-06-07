//! Translate one message across several locales, showing graceful fallback.
//!
//! Run with: `cargo run -p klauthed-i18n --example translate`

use klauthed_i18n::{Args, I18n, Locale};

fn main() {
    let i18n = I18n::with_embedded_defaults();

    for tag in ["en", "tr", "de", "es", "fr", "it", "en-US", "pt"] {
        let msg = i18n.translate_with(
            &Locale::new(tag),
            "validation.required",
            &Args::new().set("field", "email"),
        );
        // `pt` has no catalog → falls back to the default locale.
        println!("{tag:<6} {msg}");
    }
}
