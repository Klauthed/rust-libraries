# Configuration

Configuration is loaded **once at startup** and read as typed sections. It is the
backbone most other crates build on, so it gets its own chapter.

## The provider chain

`ConfigBuilder` resolves an ordered chain of `ConfigProvider`s and **deep-merges**
their output (later providers win, sibling keys are preserved):

```rust,ignore
use klauthed_core::config::{ConfigBuilder, Profile};

let config = ConfigBuilder::new(Profile::detect())
    .with_provider(/* your own ConfigProvider */)
    .build()
    .await?;
```

Built-in providers:

- **`EnvProvider`** — environment variables, with `_`-nesting and scalar coercion.
- **`FileProvider`** — TOML / JSON files.
- **`MemoryProvider`** — in-code defaults (handy in tests).
- **`VaultProvider`** (feature `vault`) — HashiCorp Vault KV v2, with
  Token / AppRole / Kubernetes auth.
- **`ConfigServerProvider`** (feature `config-server`) — pull config from a remote
  config server (klauthed-native by default, or Spring Cloud Config).

## Profiles gate the sources

A `Profile` — `Local`, `Dev`, `Test`, `Staging`, `Prod` — governs policy. In
**staging and prod, secrets must come from Vault**, never files or env; the builder
enforces this rather than trusting convention.

## Typed sections

Common sections deserialize out of the box — `config.database()?`,
`config.server()?`, `config.cache()?`, `config.messaging()?`, `config.storage()?` —
and you bind your own structs with the **`FromConfig`** derive:

```rust,ignore
use klauthed_core::config::FromConfig;

#[derive(FromConfig)]
#[config(section = "billing")]
struct BillingConfig {
    api_url: String,
    #[config(default = "30")]
    timeout_secs: u64,
}
```

## Hot reload & push-refresh

With the `hot-reload` feature, `ReloadableConfig` re-resolves the chain on an
interval and atomically swaps in the new values, notifying subscribers. For
event-driven updates, `start_with_refresh` also returns a `RefreshTrigger`:

```rust,ignore
let (config, trigger) =
    ReloadableConfig::start_with_refresh(builder, Duration::from_secs(300)).await?;

// On a config-bus event, a webhook, or klauthed-web's POST /refresh endpoint:
trigger.refresh(); // re-resolve immediately (coalesced)
```

## Running as a config server

A service can *be* the config server other services pull from. Enable
`config-server` on `klauthed-web`, mount a `ConfigServer` over a `ConfigSource`
(a directory of TOML/JSON, or in-memory), and it answers
`GET /{application}/{profile}[/{label}]` with the merged tree — a Rust-native
alternative to Spring Cloud Config Server.
