# klauthed-fuzz

Coverage-guided [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html)
harnesses for klauthed's untrusted-input parsers. This is a **standalone crate**
(its `Cargo.toml` has an empty `[workspace]` table) so it stays out of the main
workspace and its stable-toolchain gates — fuzzing needs nightly + libFuzzer.

## Targets

| Target | Exercises |
|--------|-----------|
| `jwt_decode` | `JwtVerifier::decode` on arbitrary token text |
| `aead_decrypt` | `aead::decrypt` / `decrypt_from_base64` on arbitrary ciphertext |
| `oauth2_token_response` | `serde` deserialization of an OAuth2 `TokenResponse` |
| `config_tree` | `ConfigMap::expand_dotted` + `merge` on arbitrary JSON trees |

Each target asserts only the universal contract: parsing attacker-controlled
input may return `Ok` or `Err`, but must never panic, abort, or trip a sanitizer.

## Running

```sh
cargo install cargo-fuzz                 # one-time
rustup toolchain install nightly         # one-time

cargo +nightly fuzz list                 # show targets
cargo +nightly fuzz run jwt_decode       # fuzz until a crash (Ctrl-C to stop)
cargo +nightly fuzz run config_tree -- -max_total_time=60   # time-boxed
```

A crash writes a reproducer to `fuzz/artifacts/<target>/`; replay it with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<input>`.

CI runs every target time-boxed on a weekly schedule (and on demand) via the
`Fuzz` workflow.
