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

- Reached height **3,848** in **~1.3 s** (~3,000 blocks/s) before ingest halted — see
  "Known issue" below. `mode` was reported as `"bulk"` throughout, as expected this far
  behind the chain tip.
- `du -sh data/explorer.redb` at that point: **28M**.
- Graceful shutdown (`SIGTERM`): confirmed process exits **0**, log shows `shutdown signal
  received` followed by the in-flight batch finishing before exit.
- Cold restart: confirmed the store resumes indexing from the previously persisted height
  (stopped at height 192, restarted, and progressed to height 1280 without re-indexing from
  scratch).
- Ingest halt path: confirmed the process exits **1** and logs `ingest halted: ...` when
  `xp_ingest::run` returns `Err`, and that this happens promptly rather than leaving the
  server up serving a stale snapshot.

### Known issue (out of scope for this task)

Syncing real mainnet data past height ~3,848 currently halts ingest with `corrupt row:
input box missing`. This is not caused by the `bin/explorer` wiring in this task — the
existing `xp-ingest` smoke test (`cargo test -p xp-ingest -- --ignored smoke_local_node`)
only syncs to height 2,000 and passes; bisecting with a smaller `bulk_batch` narrowed the
first failure to somewhere in the 3,841–3,848 range. Likely a real edge case in early
mainnet chain data (or in how it's applied) that the store's genesis-only "missing input"
tolerance (`height == 1`) doesn't cover. Flagged here for the team; not fixed as part of
Task 10, since it lives in `xp-store`/`xp-ingest` (earlier tasks) rather than in this
binary's config/wiring/lifecycle.
