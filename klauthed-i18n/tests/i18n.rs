//! Public-API integration tests for message translation and fallback.

use klauthed_i18n::{Args, I18n, Locale};

#[test]
fn translates_with_interpolation_and_fallback() {
    let i18n = I18n::with_embedded_defaults();

    // Exact locale + `{field}` interpolation.
    let tr = i18n.translate_with(
        &Locale::new("tr"),
        "validation.required",
        &Args::new().set("field", "e-posta"),
    );
    assert!(tr.contains("e-posta"));

    // `en-US` has no catalog of its own → falls back to the `en` catalog.
    let en = i18n.translate_with(
        &Locale::new("en-US"),
        "validation.required",
        &Args::new().set("field", "email"),
    );
    assert!(en.contains("email"));

    // An unknown key degrades to the key itself rather than panicking.
    assert_eq!(i18n.translate(&Locale::new("en"), "does.not.exist"), "does.not.exist");
}
