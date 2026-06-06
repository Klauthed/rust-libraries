# klauthed-data

Data-layer connectors for klauthed services. Turns the typed configuration sections from
`klauthed_core::config` into **real, connected resources** — database pools, cache and
messaging clients, object storage — so a service never hand-rolls connection wiring.

Every backend lives behind a Cargo feature, so a service compiles only the drivers it
uses:

| Feature | Provides |
|---------|----------|
| `postgres` / `mysql` / `sqlite` | `db::connect` (implies `sql`) |
| `mongodb` | `db::mongo::connect` |
| `mssql` | `db::mssql::connect` (via tiberius) |
| `redis` | `cache::connect_redis` |
| `cache-memory` | in-process moka cache |
| `nats` / `rabbitmq` / `kafka` | `messaging::connect_*` |
| `storage` / `storage-s3` / `storage-gcs` / `storage-azure` | object storage |

Beyond connectors, it provides the reliability patterns services reuse: **outbox**,
**idempotency**, distributed **locks**, **sagas**, **pagination** (offset/cursor/keyset
+ SQL helpers), a **transaction** abstraction, and an **event bus**.

Errors surface as `DataError` (`impl DomainError`).

---

Part of the [klauthed rust-libraries](../README.md) workspace.
Browse the API: `cargo doc -p klauthed-data --open` (use `--all-features` to see all backends).

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your option.
