# Contributing

Thanks for your interest in improving the klauthed Rust libraries!

## Prerequisites

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (stable
with `rustfmt`, `clippy`, and `rust-src`); `rustup` installs it automatically.
The minimum supported Rust version is declared as `rust-version` in the root
`Cargo.toml`.

Some optional backends compile native or driver crates; on Debian/Ubuntu you may
need:

```sh
sudo apt-get install -y libsqlite3-dev pkg-config
```

## Local checks

Run the same gates CI enforces before opening a PR:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

Formatting is enforced via [`rustfmt.toml`](rustfmt.toml). Run `cargo fmt` to fix
style automatically. To keep `git blame` clean across the one-time reformat, set:

```sh
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

## Project layout

This is a Cargo workspace of `klauthed-*` crates; see the
[root README](README.md) for the crate map. Errors follow a kernel + per-crate
model (`klauthed-error` owns the shared contract; each crate defines its own
error type). New public items should carry doc comments and tests.

## Pull requests

- Keep PRs focused; one logical change per PR.
- Make sure all four local checks above pass.
- Document public API changes and cover them with tests.
- Never commit secrets, credentials, or real keys (test fixtures use obvious
  throwaway values).

## Security

Do **not** file public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## License

Unless you state otherwise, your contributions are licensed under the same terms
as this project: **MIT OR Apache-2.0** (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)).
