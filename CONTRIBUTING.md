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

### Adding a new crate to the release

A trusted-publishing token **cannot create a new crate** (crates.io returns
`403 … do not support creating new crates`). So a brand-new `klauthed-*` crate
needs a one-time bootstrap before the CI release can manage it:

1. Add it to the `CRATES` arrays in [`scripts/publish-workspace.sh`](scripts/publish-workspace.sh)
   and [`scripts/add-owners.sh`](scripts/add-owners.sh) (in dependency order).
2. Publish it **once, manually**, with a scoped API token, to create it on
   crates.io:

   ```sh
   CARGO_REGISTRY_TOKEN=<crates.io token> cargo publish -p klauthed-<name>
   ```
3. Configure Trusted Publishing for it (the per-crate setup above) and add the
   org owner (`cargo owner --add github:klauthed:owners klauthed-<name>`).

From then on the normal tag-triggered release publishes it via OIDC like every
other crate.

## API conventions

Naming patterns the crates follow, so new code stays consistent:

- **Constructors** — `T::new(…)` for the common case; `T::builder() -> TBuilder`
  for types with many optional fields. Conversions are `from_*` / `try_from_*`.
- **Builder/setter methods** — a consuming setter named after the field
  (`Claims::builder(…).issuer("…").audience("…")`) returning `Self` and marked
  `#[must_use]`. Reserve the `with_*` prefix for configuring an *already
  constructed* value (e.g. `ConfigBuilder::with_provider`) or to disambiguate.
- **Accessors** — a field accessor is named after the field, no `get_` prefix
  (`event.action()`, not `get_action()`). The `get_*` prefix is only for **keyed**
  lookups (`Config::get_string(key)`, like `HashMap::get`).
- **Errors** — one `<Crate>Error` per crate (e.g. `DataError`), with finer-grained
  per-area errors inside larger crates (`ConfigError`, `ValidationError`).
  `klauthed_web::AppError` is the HTTP-facing application error.
- **Connectors** (`klauthed-data`) — `<module>::connect(&config)` when a module
  has a single backend (`db::connect`, `storage::connect`); `connect_<backend>`
  when it has several (`messaging::connect_nats`, `cache::connect_redis`).
- **Implementations & test doubles** — the in-memory implementation of a trait is
  `InMemory<Trait>` (`InMemoryJobQueue`, `InMemoryAuditSink`, `InMemoryMeter`).
  Test doubles for *outbound* senders that record attempts instead of delivering
  are `Recording*` (`RecordingNotifier`, `RecordingWebhookSender`). The
  `klauthed-core` config **provider** family is named by source —
  `EnvProvider`, `FileProvider`, `VaultProvider`, `MemoryProvider` — keeping that
  group internally consistent (so the in-memory provider is `MemoryProvider`, not
  `InMemoryProvider`).
- **Preludes** — every library crate (except the `klauthed-macros` proc-macro
  crate) exposes a `prelude` with its common types, and re-exports its workhorse
  types at the crate root.

## Stability policy

What the suite commits to. These tighten at 1.0; the pre-1.0 caveats note where
they are looser for now.

### Public API & SemVer

A crate's **public API** is everything reachable from its crate root (including
its `prelude`) together with its set of cargo features. Changes are versioned per
[SemVer](https://semver.org):

- **Breaking** (major; pre-1.0 a minor `0.x` bump) — removing or renaming a public
  item, changing a signature or trait bound, removing a feature, or raising the
  MSRV.
- **Additive** (minor; pre-1.0 a patch where it cleanly can be) — new items, new
  features, new builder options.
- **Patch** — bug fixes and docs with no API change.

Not covered: `#[doc(hidden)]` items, private fields, exact error *messages* (the
`DomainError` category and `area.reason` **code** *are* stable), and anything
documented as experimental.

### Deprecation

Before a public item is removed it is first **deprecated** with
`#[deprecated(since = "…", note = "use … instead")]`, kept for **at least one
minor release**, and listed under *Deprecated* in the [CHANGELOG](CHANGELOG.md);
removal follows in a later breaking release. Pre-1.0 this is best-effort; from 1.0
it is a firm contract.

### MSRV

The **minimum supported Rust version is 1.95** (`rust-version` in the root
`Cargo.toml`), enforced by the `msrv` CI job on every change. Raising it is a
**minor** bump (a major bump at/after 1.0) and is called out in the CHANGELOG. We
track a recent stable and do not commit to Rust releases older than the declared
MSRV.

## Code of conduct

Participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Do **not** file public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## License

Unless you state otherwise, your contributions are licensed under the same terms
as this project: **MIT OR Apache-2.0** (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)).
