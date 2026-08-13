# chat-storage

E2EE local storage, multilingual search, personal archive, backup, storage offload, rehydration, and knowledge/threat detection for KChat — built on [kdrive](../kdrive) and [kdrive-rust-sdk](../kdrive-rust-sdk).

## Build & test

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Architecture

Built on KDRV1 crypto from `kdrive-rust-sdk`. The kdrive Go gateway serves as the backend, extended with archive, search shard, backup, and message delivery endpoints.

## Layout

- `crates/cs-core` — Platform-agnostic Rust core (crypto, search, archive, backup, media, offload, knowledge).
- `gateway-ext/` — Go gateway extensions for kdrive (archive, search, backup, delivery, multi-tenancy).