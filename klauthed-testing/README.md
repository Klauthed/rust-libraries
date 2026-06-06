# klauthed-testing

Test utilities for klauthed services — a small toolkit pulled in as a
**dev-dependency** to make unit and integration tests deterministic and terse. It builds
on the `klauthed-core` primitives, so fixtures use the same types as production.

- **clock** — a fixed/advanceable clock (re-exporting `FixedClock`) plus terse
  constructors like `fixed_clock(ms)` and `epoch_clock()`.
- **ids** — deterministic id generation for stable test data.
- **context** — pre-built `RequestContext` fixtures.
- **assertions** — domain-aware assertion helpers.
- **repository** — an in-memory repository helper for exercising store traits.

```toml
[dev-dependencies]
klauthed-testing = { path = "../klauthed-testing" }
```

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-testing --open`.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
