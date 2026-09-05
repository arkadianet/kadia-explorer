# Ergo Explorer (standalone) — Design

Date: 2026-09-05. Status: draft for review.

## 1. Goal

A standalone Ergo blockchain explorer backend, written in Rust, that serves at
least the data the current kadia.io explorer serves (blocks, transactions,
addresses, boxes, tokens, rich list, chain and daily statistics, miners, token
holders, storage-rent analytics, search, live updates) in a fraction of the
disk and with in-process read latency.

Non-goals for v1: protocol integrations (SigmaUSD, Dexy, Duckpools, Spectrum
tx building). They are planned as consumers of the same block-event stream
(§9) and are out of scope here.

## 2. Decisions locked during brainstorming

| Decision | Choice | Why |
|---|---|---|
| Relationship to the node | **Standalone process, own index** | redb has no multi-process or read-only open (exclusive `flock`, in-memory reader tracking) so the node's live index cannot be shared safely. Standalone keeps zero coupling to the node codebase and lets the explorer follow any node, including public ones. |
| Code reuse from the node repo | **None at runtime; only wire deserialisation via a published library** | Explorer schema is designed for explorer queries, not Scala extra-index parity. |
| Storage engine for the index | **redb** (pure Rust, MVCC, zero-copy reads) | Single writer, unlimited concurrent readers that never block the writer. Ergo tree deduplication replaces block-level compression. |
| Analytics | **Split**: incremental hot aggregates in redb; range analytics in embedded **DuckDB** | Key-value speed where it is measured (front page), SQL where the user picks a range. |
| API contract | **New API**, cursor pagination, bytes-native ids rendered as hex, typed responses | Frontend is rebuilt or adapted afterwards. |
| Disk budget | Index 30–40 GB + analytics ≤ 5 GB; growth ≈ 1–2 GB/month | vs 204 GB today. On a dedicated host the node runs with its own extra index disabled, so total disk ≈ today's node alone. |

## 3. Architecture

```
 node(s)  ──REST──▶  ingest  ──BlockBatch──▶  apply  ──▶  redb index (explorer.redb)
 (rust /             (source                  │              ▲
  scala /             abstraction,             ├──▶  hot aggregates (same redb, same txn)
  public)             fork detection)          │
                                               └──▶  analytics task ──▶ DuckDB (analytics.duckdb)
                                                        (async, own cursor, replayable)
 clients  ◀──axum HTTP + WebSocket──  api  (reads redb snapshots + DuckDB; proxies mempool to node)
```

One binary, three long-lived tasks (ingest, apply, analytics) plus the API
server, all in one tokio runtime. redb writes happen only in the apply task.

## 4. Crates

```
ergo-explorer/
  crates/
    xp-types       # ids (newtypes over [u8;32]), rows, cursors, DTOs, errors
    xp-wire        # block/tx/box (de)serialisation, ergo tree → template hash / address
    xp-source      # BlockSource trait + RustNode, ScalaNode, PublicPool impls
    xp-store       # redb tables, apply/rollback, undo log, hot aggregates
    xp-analytics   # DuckDB schema, per-block extraction, queries
    xp-api         # axum router, handlers, WebSocket hub
  bin/explorer     # config, wiring, lifecycle
```

`xp-wire` depends on `sigma-rust` (`ergo-lib`) for ergo tree parsing and
address derivation. Block and transaction binary decoding is implemented in
`xp-wire` against the Ergo wire format (test vectors in §11) so the explorer
does not depend on the node repo.

## 5. Ingest (`xp-source`)

```rust
#[async_trait]
pub trait BlockSource: Send + Sync {
    async fn best_height(&self) -> Result<u32>;
    async fn header_id_at(&self, height: u32) -> Result<Option<HeaderId>>;
    async fn full_block(&self, id: &HeaderId) -> Result<Option<FullBlockBytes>>;
    fn name(&self) -> &str;
}
```

Implementations use the standard node REST surface every Ergo node exposes:
`/info`, `/blocks/at/{h}`, `/blocks/{id}`, `/blocks/{id}/header`,
`/blocks/{id}/transactions`. The Rust node has no event stream endpoint (404
on `/events`), so the tip is followed by polling `/info` (default 500 ms,
configurable). A `PublicPool` source round-robins over a list with per-host
rate limiting and quarantines hosts on error or on a header id that
disagrees with the majority.

**Bootstrap source (optional).** `LocalNodeDb` implements `BlockSource` over a
*copy* of a Rust node's `state.redb`, reading full block bodies from its block
sections table with no HTTP. Constraints: the node's live file is never
opened (redb takes an exclusive lock and a live copy is inconsistent); the
copy is taken with the node stopped or from a filesystem snapshot. The crate
(`xp-source-nodedb`) is the only place that knows the node's on-disk layout
and is not linked into the runtime binary. Build it only if a benchmark shows
loopback REST fetch, not apply, is the sync bottleneck.

**Pipeline.** Ingest produces `BlockBatch { blocks: Vec<FullBlock>, from: u32 }`
into a bounded channel (depth 8). During initial sync a batch is up to 64
blocks fetched with concurrency 8; at tip a batch is one block.

**Fork handling.** Before applying block at height h, apply checks that the
stored header at h−1 equals the block's parent id. If not, ingest walks back
until the ids agree (common ancestor a), the store rolls back to a using the
undo log (§6.6), then resumes from a+1. The rollback window is 1,000 blocks;
a fork deeper than that halts the explorer with an explicit
`reindex required` status rather than guessing.

## 6. Index store (`xp-store`)

All keys are big-endian fixed-width bytes so range scans are ordered.
`Hash32 = [u8;32]`. `Gidx = u64` is a global insertion index (box or tx),
which is the pagination cursor everywhere.

### 6.1 Tables

| Table | Key | Value | Purpose |
|---|---|---|---|
| `meta` | `&str` | bytes | `indexed_height`, `next_box_gidx`, `next_tx_gidx`, schema version |
| `headers` | `height u32` | `HeaderRow` | header bytes + block summary (tx count, size, miner tree hash, difficulty, timestamp, total fees, block reward) |
| `header_by_id` | `Hash32` | `u32` | id → height |
| `txs` | `Hash32` | `TxRow` | height, index in block, timestamp, size, fee, inputs `[Hash32]`, data inputs `[Hash32]`, output count |
| `tx_by_gidx` | `Gidx` | `Hash32` | ordered listing / range |
| `boxes` | `Hash32` | `BoxRow` | value, `tree_hash`, creation height, tx id, index, raw registers bytes, tokens `[(Hash32,u64)]`, size, `spent: Option<(Hash32 tx, u32 height)>` |
| `box_by_gidx` | `Gidx` | `Hash32` | ordered listing / range |
| `ergo_trees` | `tree_hash Hash32` | `TreeRow` | tree bytes (stored once), template hash, address string, kind (P2PK / P2S / miner / fee) |
| `tree_boxes` | `(tree_hash, Gidx)` | `()` | all boxes ever at an address, oldest → newest |
| `tree_unspent` | `(tree_hash, Gidx)` | `()` | current UTXOs at an address; removed on spend |
| `tree_txs` | `(tree_hash, Gidx tx)` | `()` | address transaction history |
| `tree_balance` | `tree_hash` | `BalanceRow` | nanoERG + token map, box count, first/last seen height |
| `rich` | `(u64 nanoERG BE, tree_hash)` | `()` | rich list = reverse range scan; updated on every balance change |
| `templates` | `template_hash` | `TemplateRow` | count, first seen, example tree hash |
| `template_boxes`, `template_unspent` | `(template_hash, Gidx)` | `()` | boxes by script template |
| `tokens` | `Hash32` | `TokenRow` | mint tx/box, name, description, decimals, type (EIP-4 parsed), emission amount, burned so far |
| `token_boxes`, `token_unspent` | `(token_id, Gidx)` | `()` | boxes carrying a token |
| `token_holders` | `(token_id, u64 amount BE, tree_hash)` | `()` | holders sorted by amount; maintained incrementally |
| `token_holder_amt` | `(token_id, tree_hash)` | `u64` | current amount per holder, needed to update `token_holders` |
| `rent_matures` | `(mature_height u32, Gidx)` | `()` | unspent boxes keyed by `creation + 1_051_200`; removed on spend. Drives "eligible at / upcoming" queries. |
| `register_idx` | `(reg u8, blake2b256(serialised value), Gidx)` | `()` | exact-match register search (R4–R9). Optional, on by default. |
| `undo` | `height u32` | `UndoRow` | everything needed to reverse the block (§6.6) |

Row encodings are hand-written, fixed layout where possible, variable-length
sections length-prefixed. No serde in the hot path.

### 6.2 Apply

One redb write transaction per `BlockBatch`. For each block, in order:

1. Insert header, `header_by_id`.
2. For each tx: allocate tx gidx; insert `txs`, `tx_by_gidx`.
3. For each input: load `BoxRow`, set `spent`, remove from `tree_unspent`,
   `template_unspent`, `token_unspent`, `rent_matures`; subtract from
   `tree_balance`, `rich`, `token_holder_amt`/`token_holders`; add
   `tree_txs` for the input's tree.
4. For each output: allocate box gidx; upsert `ergo_trees` and `templates`
   if new; insert `boxes`, `box_by_gidx`, `tree_boxes`, `tree_unspent`,
   `template_*`, `token_*`, `rent_matures`, `register_idx`; add to
   balances; add `tree_txs`. If a token id equals the first input box id,
   parse EIP-4 registers and insert `tokens`.
5. Detect burns: token amount in − amount out > 0 → update `tokens.burned`.
6. Write `undo[height]`, update `meta.indexed_height`.

Durability: `Durability::Immediate` at tip. During initial sync
`Durability::None` with a forced `Immediate` commit every 256 blocks, so a
crash loses at most 256 blocks of work and the store is never corrupt.

### 6.3 Reads

Every handler opens one `ReadTransaction` (a consistent snapshot) and reads
from it; snapshots never block the writer. Pagination is `(Gidx cursor,
limit ≤ 500, direction)` and is O(limit) via range scans on the composite
tables. Address pages need no sort step because `Gidx` is creation order.

### 6.4 Hot aggregates (redb, maintained in the same write txn)

- `rich` (above) — top-N is a reverse range scan.
- `stats_24h` under `meta`: rolling tx count, fees, volume, block count,
  recomputed from the last 720 `headers` rows on each apply (cheap).
- `miners_recent`: `(miner_tree_hash) → count` over the last 4,320 blocks
  (window maintained by adding the new block and subtracting the block that
  fell out).
- `rent_upcoming` is a range scan on `rent_matures` from `indexed_height`
  to `+N`, no extra table.

### 6.5 Storage-rent semantics (must match consensus)

A box is claimable at `creation_height + 1_051_200`. Rent due =
`box_size_bytes × 1_250_000 nanoERG`, capped at the box value. `rent_matures`
uses the on-chain `creationHeight`, not inclusion height. Rent *claims* are
detected per tx in `xp-analytics` (§7) as inputs whose creation height ≤
`block_height − 1_051_200`; rent taken = value in − value returned to the
same ergo tree.

### 6.6 Rollback

`UndoRow` stores, per block: the list of box ids created (delete them and all
their secondary entries), and for each spent input the previous `BoxRow.spent`
(always `None`), plus the previous `tree_balance`, `token_holder_amt`, and
`templates`/`tokens` deltas. Rollback of height h reverses steps 6→1 inside
one write txn and is unit-tested as `apply(b); rollback(b)` yielding a
byte-identical store (§11). Undo rows older than the 1,000-block window are
pruned each apply.

## 7. Analytics (`xp-analytics`, DuckDB)

Runs as its own task with its own cursor (`analytics_height` in DuckDB). It
consumes `BlockApplied { height, block }` / `BlockRolledBack { height }`
events from the apply task over a channel and can lag behind the index
without affecting API correctness for index-backed pages. On restart it
replays from its cursor by reading blocks from the redb store, so it never
needs the network.

Tables (all keyed by `height`, so rollback is `DELETE WHERE height > a`):

| Table | Columns |
|---|---|
| `blocks` | height, timestamp, tx_count, size, fees, reward, miner_tree_hash, difficulty |
| `rent_claims` | height, tx_id, claimer_tree_hash, boxes_claimed, rent_nanoerg, claim_kind (recreate/absorb) |
| `token_transfers` | height, tx_id, token_id, amount_moved, from_count, to_count |
| `daily` (materialised by a scheduled statement at 00:00 UTC and on demand) | day, tx_count, fees, volume_nanoerg, active_addresses, new_addresses, rent_claimed, blocks |

Served endpoints: daily series, rent daily/historical/windows breakdown,
token transfer volume, miner share over arbitrary ranges. DuckDB compression
is on by default; expected size for the full chain is a few GB.

## 8. API (`xp-api`)

axum, JSON, all ids as lowercase hex, amounts as decimal strings for nanoERG
and token amounts (no i64 precision loss in JS). Every list endpoint returns
`{ items, next_cursor, total? }`; `total` only where it is a single read.

```
GET  /v1/status                       indexed height, analytics height, sources, lag
GET  /v1/blocks?cursor&limit          newest first
GET  /v1/blocks/{height|id}           header + summary
GET  /v1/blocks/{id}/txs?cursor&limit
GET  /v1/txs/{id}                     tx with resolved inputs (BoxRow) and outputs
GET  /v1/txs?cursor&limit             newest first (global tx range)
GET  /v1/boxes/{id}
GET  /v1/boxes/{id}/rent              maturity height, due amount, tier
GET  /v1/addresses/{addr}             balance, counts, first/last seen
GET  /v1/addresses/{addr}/boxes?unspent=bool&cursor&limit
GET  /v1/addresses/{addr}/txs?cursor&limit
GET  /v1/addresses/{addr}/rent        unspent boxes with maturity, sorted by maturity
GET  /v1/templates/{hash}/boxes?unspent&cursor&limit
GET  /v1/tokens?cursor&limit&sort=holders|minted
GET  /v1/tokens/{id}                  metadata, supply, burned, holder count
GET  /v1/tokens/{id}/holders?cursor&limit
GET  /v1/tokens/{id}/boxes?unspent&cursor&limit
GET  /v1/registers/{reg}/{valueHex}/boxes?cursor&limit
GET  /v1/richlist?cursor&limit
GET  /v1/miners?window=blocks
GET  /v1/stats/24h
GET  /v1/stats/daily?from&to
GET  /v1/rent/summary | /upcoming?blocks | /eligible?cursor | /recent?cursor | /daily?from&to
GET  /v1/search?q=                    height | header id | tx id | box id | token id | address | template hash
GET  /v1/mempool/*                    proxied from the configured primary node (unconfirmed txs, fee histogram)
WS   /v1/ws                           events: block_applied, block_rolled_back, address:{addr} tx notifications, mempool tx (from node poll)
```

Errors: RFC 7807 problem JSON. Limits: `limit ≤ 500`, request timeout 5 s,
per-IP token bucket. Response cache: none for index-backed reads (they are
already microseconds); DuckDB range queries cached 60 s keyed by query.

## 9. Extension point for protocol integrations (v2)

Every protocol module is a consumer of the same `BlockApplied`/`BlockRolledBack`
channel used by analytics, with its own DuckDB tables and its own cursor.
Nothing in v1 needs to change to add SigmaUSD, Dexy, Duckpools or AMM
trackers later; they are additive crates.

## 10. Operations

- Config: one TOML (`sources[]`, `data_dir`, `bind`, `poll_ms`,
  `rollback_window`, `register_index`, `rate_limits`).
- Data dir: `explorer.redb`, `analytics.duckdb`, `explorer.log`.
- Initial sync from an owned node: expected several hours on the EPYC-class
  box (bounded by node block fetch throughput; redb apply is not the limit).
  From public nodes only: days; supported but documented as slow.
- Health: `/v1/status` includes `lag_blocks`; systemd unit with
  `Restart=on-failure`; the store is crash-safe by construction (§6.2).
- Upgrades: `meta.schema_version`; incompatible schema change = reindex,
  stated in the changelog. No in-place migrations in v1.

## 11. Testing

- **Wire**: golden vectors for header, transaction, box, ergo tree, EIP-4
  token metadata, including the genesis block and a block with data inputs
  and a token mint. Decoded structs compared field-by-field.
- **Store**: in-memory redb; every table op unit-tested; property test
  `apply(batch) ; rollback(batch) == identity` (store bytes hashed before and
  after) over randomly generated valid block sequences.
- **Consensus constants**: a test asserts `1_051_200` and `1_250_000` against
  the values in the storage-rent spec and a known mainnet claim tx.
- **Parity**: an integration test (needs a node) samples 200 random addresses,
  tokens and templates, and compares balances, unspent box sets and tx
  counts against the Rust node's `/blockchain/*` responses. This is the
  "at least equivalent" gate.
- **Fork**: a source stub that serves a 3-block fork; assert rollback to the
  common ancestor and identical state to a fresh index of the winning chain.
- **API**: per-route tests over a small fixture chain; cursor pagination
  covers first/middle/last page and empty results.

## 12. Open items (not blocking v1)

- Whether to persist full ergo tree source (decompiled) — no; derive on demand.
- Register index cardinality for very common values (e.g. empty R4) —
  skip indexing values with more than 1M entries via a per-value counter.
- Publishing `ergo-ser` from the node repo as a library would let `xp-wire`
  reuse tested decoders; kept optional to preserve the no-coupling decision.
