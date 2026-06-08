//! `klauthed-i18n`: per-locale catalogs, `{name}` interpolation, and the
//! locale → primary-subtag → default-locale → key fallback chain.

use klauthed_i18n::{Args, I18n, Locale};

pub fn run() {
    let i18n = I18n::builder()
        .add_catalog("en", "greeting = \"Hello, {name}!\"\nbye = \"Goodbye\"")
        .unwrap()
        .add_catalog("tr", "greeting = \"Merhaba, {name}!\"")
        .unwrap()
        .default_locale("en")
        .build();

    let en = Locale::new("en");
    let tr = Locale::new("tr");

    let hello_en = i18n.translate_with(&en, "greeting", &Args::new().set("name", "Alice"));
    let hello_tr = i18n.translate_with(&tr, "greeting", &Args::new().set("name", "Ali"));
    println!("  en: {hello_en}");
    println!("  tr: {hello_tr}");
    assert_eq!(hello_en, "Hello, Alice!");
    assert_eq!(hello_tr, "Merhaba, Ali!");

    // `bye` exists only in the default (en) catalog -> tr falls back to it.
    let bye_tr = i18n.translate(&tr, "bye");
    println!("  tr 'bye' (fallback to default): {bye_tr}");
    assert_eq!(bye_tr, "Goodbye");

    // An unknown key returns the key itself.
    assert_eq!(i18n.translate(&en, "missing.key"), "missing.key");
}
