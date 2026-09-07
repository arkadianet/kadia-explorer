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
`/v1/boxes`, `/v1/addresses`, `/v1/tokens`, `/v1/templates`, `/v1/registers`, ...).

`GET /v1/addresses/{addr}/txs?cursor&limit&dir` returns lightweight `TxSummaryDto` items
(`id`, `height`, `index`, `timestamp`, `size`, `fee`, `input_count`, `data_input_count`,
`output_count`) rather than the full `TxDto` — the handler resolves no input/output boxes,
so a wallet address with tens of thousands of boxes pages in milliseconds instead of timing
out. `GET /v1/txs/{id}` and the block/global transaction lists are unaffected and still
return the full `TxDto` with resolved inputs and outputs.

`/v1/status` reports `{ indexed, best, mode, source, halted, lag_blocks, stalled,
inflight_reads, rate_limited_total }` and is the quickest way to watch progress:

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
  },
  "inflight_reads": 0,
  "rate_limited_total": 0
}
```

`stalled` is `null` whenever ingest is progressing or merely idle at the tip.
`inflight_reads` is the number of blocking store reads in flight right now (bounded by
`api.max_inflight_reads`, see [Limits](#limits) below); `rate_limited_total` is a
process-lifetime count of requests rejected with `429`. Neither is persisted — both reset
to `0` on restart.

## Limits

An optional `[api]` section in `explorer.toml` bounds concurrent blocking store reads and
rate-limits clients per IP. The whole section may be omitted; every key below then takes
its default:

```toml
[api]
max_inflight_reads = 32
trusted_proxies = ["127.0.0.1", "::1"]

[api.rate_limit]
per_second = 10
burst = 30
allowlist = ["203.0.113.7", "2001:db8::/32"]
```

| Key | Meaning | Default |
|---|---|---|
| `api.max_inflight_reads` | Max concurrent blocking store reads. A request that finds the pool full gets `503` immediately instead of queuing. | 32 |
| `api.trusted_proxies` | IPs/CIDRs allowed to set `X-Forwarded-For`. Behind a trusted proxy, the client key is the right-most address in the header that is *not* itself a trusted proxy; a missing or unparsable header falls back to the peer address (a proxy can never be spoofed into bypassing the limit). Requests from any other peer are keyed on the peer address, ignoring the header entirely. | `["127.0.0.1", "::1"]` |
| `api.rate_limit.per_second` | Token-bucket refill rate per client key, in tokens/s (one token per request). `0` disables rate limiting entirely. | 10 |
| `api.rate_limit.burst` | Token-bucket capacity per client key. | 30 |
| `api.rate_limit.allowlist` | IPs/CIDRs (v4 and v6) exempt from the bucket entirely — not counted, never limited. Put the fleet's arms and the operator's IP here. | `[]` (empty) |

**429 (rate limited).** A client key with no tokens left gets `429 Too Many Requests` as an
RFC 7807 problem JSON body (`type`, `title`, `status`, `detail`), with a `Retry-After`
header giving the whole seconds until a token is available (minimum 1). Each `429`
increments `rate_limited_total` on `/v1/status`.

**503 (overloaded).** A request that arrives when `max_inflight_reads` blocking reads are
already in progress gets `503 Service Unavailable`, same problem JSON shape, `Retry-After:
1`, without occupying a blocking-pool thread. The existing 5 s request timeout is unchanged
and still applies to reads that do get a permit.

Neither case is logged per request — a flood must not also flood the log — so the counters
on `/v1/status` are the way to see rate limiting or overload happening.

**Allowlisting the fleet.** Add each collector/bot IP (and the operator's own) to
`api.rate_limit.allowlist` before exposing the API publicly, otherwise a normal polling
cadence from the fleet can trip its own rate limit. Caddy in front of the API must be listed
in `api.trusted_proxies` (it already forwards `X-Forwarded-For`) or every request will be
keyed on Caddy's own address instead of the real client's.

## Tokens, templates and register search

Schema v2 adds token, script-template and register-value indexing. New routes:

- `GET /v1/tokens?sort=newest|holders&cursor&limit` — all indexed tokens, newest mint first
  or most-held first. `newest` cursors on the mint's global index; `holders` cursors on
  `<holder_count>:<token_id_hex>`.
- `GET /v1/tokens/{id}` — one token's facts (name, description, decimals, `kind`, emission,
  burned, supply, holder/box counts). 404 if the id was never minted.
- `GET /v1/tokens/{id}/holders?cursor&limit` — current holders by amount held, descending.
  Cursor is `<amount>:<treehex>`.
- `GET /v1/tokens/{id}/boxes?unspent&cursor&limit&dir` — boxes carrying the token.
- `GET /v1/templates/{hash}` — a script template's box/unspent-box counts. 404 if unseen.
- `GET /v1/templates/{hash}/boxes?unspent&cursor&limit&dir` — boxes on that template.
- `GET /v1/registers/{R4..R9}/{valueHex}/boxes?cursor&limit&dir` — boxes whose register holds
  exactly that serialised sigma constant. The server hashes `valueHex` with blake2b-256 the
  same way the indexer keys the register table; an unindexed value is an empty page, not a
  404 (there is no row asserting the value ever existed).
- `/v1/search` also resolves a 64-hex term as a token id or template hash, tried after
  header id, tx id and box id, in that order.

Token `kind` is derived from the EIP-4 `R7` type tag: `0101` → `nft-picture`, `0102` →
`nft-audio`, `0103` → `nft-video`, `0201` → `membership`, anything else (including no `R7`
at all) → `token`. Burns are tracked per transaction and token as `burned += max(0, in −
out)` on the token's total input/output amounts for that tx.

`AddressDto` also gained `tx_count`, and `BoxDto.tokens[]` entries gained `name`/`decimals`
(both `null` when the token's mint predates the store's start height — see the upgrade note
below).

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

### Note on upgrading to schema v2 (tokens, templates, registers)

`SCHEMA_VERSION` is now `2` (`crates/xp-store/src/tables.rs`). Unlike the two notes above, this
*is* a table-layout change: new tables back the token/template/register-search endpoints.
There is no in-place migration path. `Store::open` compares the store's recorded version
against `SCHEMA_VERSION` and refuses to open a mismatch:

```text
corrupt row: schema version mismatch
```

To upgrade: stop the service, delete `data/explorer.redb`, and start it again — it resyncs
from genesis, this time populating the new tables as it goes. There is no way to add the new
tables to an existing v1 store short of a full reindex.

Because indexing starts fresh, tokens minted before a *partially* re-synced store's current
height simply have no token row yet (the mint block hasn't been reached). Readers tolerate
this: `BoxDto.tokens[].name`/`decimals` come back `null` and `/v1/tokens/{id}` 404s for such
a token until ingest catches up to its mint height.
