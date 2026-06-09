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
sudo apt-get install -y libsqlite3-dev libssl-dev pkg-config
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

CI runs a few more gates you can reproduce locally: `cargo deny check`
(advisories/licenses/bans), `osv-scanner scan source --lockfile=Cargo.lock`
(GHSA + RustSec), and an MSRV build. Library code is held to
`deny(missing_docs)` and denies `clippy::{unwrap_used, expect_used, panic,
indexing_slicing}` in non-test code — shipping code must not panic on a fallible
path. The live-infra integration tests are `#[ignore]`d; run them against local
Postgres/Redis/Mongo with `DB_URL` / `REDIS_URL` / `MONGODB_URL` set and
`cargo test --workspace --all-features -- --ignored`.

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

## Versioning & releases

All `klauthed-*` crates **share a single version** and are released together,
following SemVer. While the suite is **pre-1.0**, a minor bump (`0.x`) may carry
breaking changes and a patch bump (`0.x.y`) is backward-compatible. The **MSRV**
(`rust-version` in the root `Cargo.toml`) is part of the contract — raising it is
a minor-version change.

Publishing is owned by CI; locally you only bump and tag.

1. Bump the shared version with
   [`cargo-release`](https://github.com/crate-ci/cargo-release) (`shared-version`
   in [`release.toml`](release.toml)). The `version` subcommand **only** rewrites
   version numbers — it does not publish:

   ```sh
   cargo release version minor --execute   # or `patch` / `major`
   ```

   This bumps every crate and the workspace dependency entries to the same
   `X.Y.Z` (verify with `git diff`).
2. Commit, tag, and push:

   ```sh
   git commit -am "release: vX.Y.Z"
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin master --follow-tags
   ```
3. The [`release` workflow](.github/workflows/release.yml) triggers on the `v*`
   tag, publishes to crates.io in dependency order (skipping `publish = false`
   members), and creates a GitHub Release.

Publishing uses **crates.io Trusted Publishing (OIDC)** — the workflow exchanges
its GitHub identity for a short-lived token via
[`rust-lang/crates-io-auth-action`](https://github.com/rust-lang/crates-io-auth-action),
so there's **no long-lived `CRATES_IO_TOKEN` secret**. This requires a one-time
setup per crate on crates.io: **crate → Settings → Trusted Publishing → add
GitHub Actions**, with owner `klauthed`, repo `rust-libraries`, workflow
`release.yml`.

> Publishing lives only in CI so there's a single, auditable publish path, no
> long-lived token, and no risk of a local double-publish.

## Code of conduct

Participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Do **not** file public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## License

Unless you state otherwise, your contributions are licensed under the same terms
as this project: **MIT OR Apache-2.0** (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)).
