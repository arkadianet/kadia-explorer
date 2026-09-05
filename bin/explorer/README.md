# explorer

A standalone Ergo blockchain explorer: indexes an Ergo node's blocks into a local
[`redb`](https://github.com/cberner/redb)-backed store (`xp-store`) via `xp-ingest`, and
serves the result over a read-only HTTP API (`xp-api`). No external database, no chain
reference node bundled — point it at any Ergo node's REST API.

## Running

```bash
cargo build --release
cp explorer.example.toml explorer.toml   # edit data_dir / bind / source.url
./target/release/explorer --config explorer.toml
```

`--config <path>` defaults to `explorer.toml` in the current directory. Any other argument
shape prints a usage message and exits.

Logging is via `tracing`; set `RUST_LOG` to control verbosity (defaults to
`info,xp_ingest=info` when unset), e.g. `RUST_LOG=debug,xp_ingest=trace`.

Shut down with `Ctrl+C` or `SIGTERM`: the HTTP server stops accepting new connections, the
ingest task finishes applying its current batch (never leaves the store mid-block), then the
process exits. Exit code is `0` on a signal-triggered shutdown, `1` if ingest halted on its
own (a fork deeper than the rollback window, a corrupt store, or an undecodable block) — a
halted explorer never keeps serving stale data silently, it exits so a supervisor notices.

## Config

TOML file, e.g. [`explorer.example.toml`](../../explorer.example.toml):

```toml
data_dir = "./data"
bind = "127.0.0.1:8090"
[source]
kind = "rust_node"
url = "http://127.0.0.1:9063"
[ingest]
poll_ms = 500
bulk_batch = 64
bulk_concurrency = 8
durable_every = 256
tip_lag_for_bulk = 64
```

| Key | Meaning | Default |
|---|---|---|
| `data_dir` | Directory for the store file (`explorer.redb`); created if missing. | — (required) |
| `bind` | HTTP listen address for the API. | — (required) |
| `source.kind` | Block source backend. Only `"rust_node"` is supported today. | — (required) |
| `source.url` | Base URL of the node's REST API. | — (required) |
| `ingest` | Optional section; each key below defaults independently if the section or key is omitted. | |
| `ingest.poll_ms` | Wait between polls when idle or after a transient source error. | 500 |
| `ingest.bulk_batch` | Blocks fetched/applied per transaction while catching up. | 64 |
| `ingest.bulk_concurrency` | In-flight block fetches while catching up. | 8 |
| `ingest.durable_every` | Force an fsync-ed commit every N heights while catching up. | 256 |
| `ingest.tip_lag_for_bulk` | Lag (best − indexed) above which bulk mode (vs. tip mode) is used. | 64 |

An unrecognized `source.kind` is rejected at startup with a clear error naming the value.

## API

See `xp-api` for the full `/v1` route tree (`/v1/status`, `/v1/blocks`, `/v1/txs`,
`/v1/boxes`, `/v1/addresses`, ...). `/v1/status` reports `{ indexed, best, mode, source,
halted, lag_blocks }` and is the quickest way to watch progress:

```bash
curl -s 127.0.0.1:8090/v1/status
```

## Benchmarks

Bounded smoke run against a local mainnet archive node (`http://127.0.0.1:9063`), syncing
from genesis with the default `[ingest]` settings. *First ~90 s from genesis, early small
blocks — not representative of tip-era blocks.*

- Reached height **6,000** in **~1.8–2.5 s** (~2,400–3,300 blocks/s across runs). `mode` was reported as
  `"bulk"` throughout, as expected this far behind the chain tip.
- `du -sh data/explorer.redb`: **28M** at height 3,848 (measured with the standalone binary).
- Graceful shutdown (`SIGTERM`): confirmed process exits **0**, log shows `shutdown signal
  received` followed by the in-flight batch finishing before exit.
- Cold restart: confirmed the store resumes indexing from the previously persisted height
  (stopped at height 192, restarted, and progressed to height 1280 without re-indexing from
  scratch).
- Ingest halt path: confirmed the process exits **1** and logs `ingest halted: ...` when
  `xp_ingest::run` returns `Err`, and that this happens promptly rather than leaving the
  server up serving a stale snapshot.

### Note on upgrading

The store now seeds Ergo's three chain-spec genesis boxes (from the node's `/utxo/genesis`)
before applying height 1, and no longer tolerates a missing input at height 1. An
`explorer.redb` written before that change was built on the old tolerance and holds wrong
balances for the genesis trees, so **delete any existing `data/explorer.redb` and re-sync**.

The schema version is unchanged — the table layout never changed, only what gets written
into it — so this is not a schema mismatch. It is instead detected at startup: a store that
has indexed blocks while being neither genesis-seeded nor explicitly partial can only have
been written by the old code, and the explorer refuses to open it with

```text
corrupt row: store predates genesis seeding; delete explorer.redb and resync
```
