//! End-to-end test of the `cargo-klauthed` binary: run `new` and inspect the
//! generated tree.

use std::fs;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_cargo-klauthed");

#[test]
fn new_generates_the_expected_layout() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("svc");

    let status = Command::new(BIN)
        .arg("new")
        .arg("demo-svc")
        .arg("--path")
        .arg(&dir)
        .status()
        .expect("run cargo-klauthed");
    assert!(status.success(), "scaffold should succeed");

    for rel in ["Cargo.toml", "src/main.rs", "config/default.toml", ".gitignore", "README.md"] {
        assert!(dir.join(rel).exists(), "expected generated file: {rel}");
    }

    let cargo = fs::read_to_string(dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(cargo.contains("name = \"demo-svc\""));
    assert!(cargo.contains("klauthed = { version ="));

    let main = fs::read_to_string(dir.join("src/main.rs")).expect("read main.rs");
    assert!(main.contains("hello from demo-svc"));
    assert!(!main.contains("__NAME__"), "all placeholders must be substituted");
}

#[test]
fn new_via_cargo_subcommand_argv_is_accepted() {
    // Simulate `cargo klauthed new …`, where cargo injects the `klauthed` arg.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("svc");

    let status = Command::new(BIN)
        .arg("klauthed")
        .arg("new")
        .arg("demo")
        .arg("--path")
        .arg(&dir)
        .status()
        .expect("run cargo-klauthed");
    assert!(status.success());
    assert!(dir.join("src/main.rs").exists());
}

#[test]
fn new_with_jwt_includes_auth() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("svc");

    let status = Command::new(BIN)
        .arg("new")
        .arg("authy")
        .arg("--with-jwt")
        .arg("--path")
        .arg(&dir)
        .status()
        .expect("run cargo-klauthed");
    assert!(status.success());

    let cargo = fs::read_to_string(dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(cargo.contains("\"security\""), "jwt scaffold enables the security feature");

    let main = fs::read_to_string(dir.join("src/main.rs")).expect("read main.rs");
    assert!(main.contains("/login"));
    assert!(main.contains("JwtAuth"));
}

#[test]
fn new_refuses_a_nonempty_target() {
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("existing"), "x").expect("seed file");

    let status = Command::new(BIN)
        .arg("new")
        .arg("demo")
        .arg("--path")
        .arg(tmp.path())
        .status()
        .expect("run cargo-klauthed");
    assert!(!status.success(), "should refuse a non-empty directory");
}

#[test]
fn new_rejects_an_invalid_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let status = Command::new(BIN)
        .arg("new")
        .arg("1bad")
        .arg("--path")
        .arg(tmp.path().join("svc"))
        .status()
        .expect("run cargo-klauthed");
    assert!(!status.success());
}
