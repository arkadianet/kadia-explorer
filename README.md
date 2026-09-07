# Kadia Explorer

A standalone Ergo blockchain explorer: a Rust indexer + JSON API over an embedded
[redb](https://github.com/cberner/redb) store, and a SvelteKit frontend. It syncs from any
Ergo node's REST API (the Rust node's extra index is not required), keeps its own compact
index (boxes, transactions, addresses, tokens, script templates, register values, storage
rent), and serves it at <https://explorer.kadia.io>.

| Part                   | Where                          | Docs                                                                                    |
| ---------------------- | ------------------------------ | --------------------------------------------------------------------------------------- |
| Indexer + API binary   | `bin/explorer`, `crates/xp-*`  | [`bin/explorer/README.md`](bin/explorer/README.md)                                      |
| Frontend               | `frontend/`                    | [`frontend/README.md`](frontend/README.md)                                              |
| Design specs and plans | `docs/superpowers/`            | start with the [design spec](docs/superpowers/specs/2026-09-05-ergo-explorer-design.md) |
| Changes                | [`CHANGELOG.md`](CHANGELOG.md) |                                                                                         |

## Quick start

```sh
cargo build --release -p explorer
cp explorer.example.toml explorer.toml   # point `url` at an Ergo node
./target/release/explorer --config explorer.toml
# API on http://127.0.0.1:8090/v1/status
cd frontend && npm ci && npm run dev      # proxies /v1 to the API
```

Gates: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`;
frontend `npm test && npm run check && npm run lint && npm run build` and `npx playwright test`.
