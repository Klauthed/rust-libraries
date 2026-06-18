# Releases & versioning

## One version, shipped together

All `klauthed-*` crates **share a single workspace version** and are released as a
set, so a given `klauthed` release pins a coherent matrix of crate versions. The
project follows [Semantic Versioning](https://semver.org); while pre-1.0, minor
versions may carry breaking changes (noted in the changelog).

- **[CHANGELOG](https://github.com/Klauthed/rust-libraries/blob/master/CHANGELOG.md)** — every release's changes.
- **[ROADMAP](https://github.com/Klauthed/rust-libraries/blob/master/ROADMAP.md)** — what's shipped and what's next.

## How a release is cut

Publishing uses **crates.io Trusted Publishing (OIDC)** — no long-lived token. A
maintainer tags `vX.Y.Z`; the `Release` workflow then:

1. checks out the tag,
2. mints a short-lived crates.io token via the workflow's GitHub OIDC identity,
3. runs a dependency-ordered, idempotent publish of every crate, and
4. cuts a GitHub Release.

Each crate has a per-crate Trusted Publisher configured on crates.io. The publish
script is idempotent and backs off on rate limits, so a large release (or a re-run)
completes reliably.

## Stability & MSRV

- **MSRV: Rust 1.95**, edition 2024 — checked in CI on every change.
- Quality gates on every change: `rustfmt`, `clippy -D warnings`, a feature
  powerset build, `cargo-deny`, an OSV scan, a coverage floor, doctests, and
  live-infra integration tests; untrusted parsers are fuzzed on a nightly schedule.

See [CONTRIBUTING.md](https://github.com/Klauthed/rust-libraries/blob/master/CONTRIBUTING.md)
for the development workflow.
