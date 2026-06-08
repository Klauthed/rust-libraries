//! A runnable tour of the klauthed libraries.
//!
//! Each module exercises one area and prints what it does (and asserts the
//! results, so this doubles as an end-to-end smoke test). Run the whole tour:
//!
//! ```text
//! cargo run -p klauthed-examples
//! ```

mod authz_demo;
mod core_demo;
mod data_demo;
mod error_demo;
mod i18n_demo;
mod protocol_demo;
mod security_demo;

/// Print a section header.
fn section(title: &str) {
    println!("\n\x1b[1m── {title} ─────────────────────────────\x1b[0m");
}

#[tokio::main]
async fn main() {
    println!("klauthed feature tour");

    section("core: time, validation, ids");
    core_demo::run();

    section("error: DomainError derive");
    error_demo::run();

    section("security: jwt, password, aead, mfa");
    security_demo::run();

    section("authz: rbac, inheritance, abac, resource scoping");
    authz_demo::run();

    section("data: rate limiting");
    data_demo::run().await;

    section("protocol: oidc + oauth2");
    protocol_demo::run();

    section("i18n: catalogs + interpolation");
    i18n_demo::run();

    println!("\n\x1b[32m✓ all feature demos ran successfully\x1b[0m");
}
