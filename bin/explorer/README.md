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
process exits. A halted explorer never keeps serving stale data silently — it exits so a
supervisor notices.

| Exit code | Meaning |
|---|---|
| `0` | Clean shutdown after `SIGTERM`/`Ctrl+C`. |
| `1` | Ingest halted (a corrupt store, an undecodable block, ...). Restarting may help. |
| `2` | Bad arguments, or the config/store/listener could not be set up at startup. |
| `3` | Ingest halted needing a full reindex: a fork deeper than the rollback window. Restarting **cannot** help — the store must be deleted and re-synced. |

Under systemd, exclude code 3 from the restart policy so the unit doesn't loop forever on a
store that can only be fixed by hand:

```ini
Restart=on-failure
RestartPreventExitStatus=3
```

## Config

TOML file, e.g. [`explorer.example.toml`](../../explorer.example.toml):

```toml
data_dir = "./data"
bind = "127.0.0.1:8090"
[source]
kind = "rust_node"
url = "http://127.0.0.1:9063"
# fallback_url = "https://node.ergo.watch"
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
| `source.fallback_url` | Optional second node, used **only** for block bodies the primary announces but will not serve. See [Stalls and the fallback source](#stalls-and-the-fallback-source). | unset (no fallback) |
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
halted, lag_blocks, stalled }` and is the quickest way to watch progress:

```bash
curl -s 127.0.0.1:8090/v1/status
```

```json
{
  "indexed": 545683,
  "best": 1866002,
  "mode": "bulk",
  "source": "http://127.0.0.1:9063",
  "halted": null,
  "lag_blocks": 1320319,
  "stalled": {
    "height": 545684,
    "since_secs": 912,
    "reason": "source http://127.0.0.1:9063 announced a header at height 545684 but serves no block body"
  }
}
```

`stalled` is `null` whenever ingest is progressing or merely idle at the tip.

## Stalls and the fallback source

A node can announce a header at a height (`/blocks/at/{height}`) and then 404 its body
(`/blocks/{id}`) — the Rust node does exactly this for some blocks, e.g. height 545684. Ingest
cannot skip the hole (the store needs a contiguous run of blocks), so it retries forever.

Two things make that survivable:

- **Visibility.** The freeze is published as `stalled` on `/v1/status` (`height`, `since_secs`,
  `reason`), logged as a `warn!` when it starts and at most once a minute after that, and the
  retry interval backs off to `max(poll_ms, 5s)` so the node is not polled twice a second for
  hours. A stall is *not* a halt: `halted` stays `null`, the process stays up, and it clears by
  itself the moment a batch applies. A `stalled` that never clears is the signal to act.
- **A fallback body source.** Set `source.fallback_url` to a second node. Block *bodies* the
  primary cannot serve are then fetched from it; `/info`, `/blocks/at/{height}` and
  `/utxo/genesis` still come from the primary alone, so the chain being indexed is entirely the
  primary's. The fallback is asked by *header id* — the id the primary announced — so it cannot
  substitute a different block. `source` in `/v1/status` reads `"<primary> (+fallback)"` when
  one is configured, and every fallback hit is logged at `info!`.

```toml
[source]
kind = "rust_node"
url = "http://127.0.0.1:9063"
fallback_url = "https://node.ergo.watch"
```

Leave `fallback_url` unset (the default) to talk to one node only.

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

### Note on fees (stores written before fee computation was fixed)

Fees used to be computed as `value_in - value_out`, which on Ergo is identically zero: a
transaction pays its fee by creating an *output* locked by the miner-fee contract, so inputs
and outputs always balance. Every `fee` and `fees` in a store written before that fix is
therefore `0`.

Nothing about the store format changed and the schema version is unchanged, so such a store
opens and keeps indexing normally — but its historical rows are not backfilled. **Re-sync to
get correct fees for already-indexed heights**; blocks applied after the upgrade are correct
either way.
