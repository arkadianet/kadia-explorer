# Explorer historical balances and storage-rent reporting — Design

Date: 2026-09-10. Status: historical reads implemented in the working tree, awaiting Rust execution gates; rent verification remains gated. See `WORK-REPORT.md` for delivery status.

## 1. Goal and recommendation

Answer two operator questions well: “What did this address hold at block H?” and “Which expired boxes were actually consumed through the rent rule, and what value left them?” Complete the useful reporting routes already promised in the explorer design, without making the 89 GB index resync for these features.

The original investigation below was design-only. The September 10 implementation adds historical reads without changing the core schema; its validation status is recorded in `WORK-REPORT.md`. Paths below are relative to `/home/rkadias/coding/development/arkadianet/ergo-explorer` unless stated otherwise. The inspected checkout is `feat/explorer-core`, commit `3802ace95fc7a3844a7e611fb2ea3364206d8bbc`. The legacy input is `/home/rkadias/coding/archive/kadia.io/docs/explorer_api_contract.md`, including all 16 endpoints; it specifies neither the historical selector parameters nor a detailed collections response. Examples below define a new contract, not undocumented legacy compatibility.

| Rank by value for effort | Gap | Decision | Core schema v3? |
|---|---|---|---|
| 1 | Historical address balance | Build from existing box lifetimes; height first, bounded work, auditable box pages | No |
| 2 | Rent actually claimed | Build verified input-level history, backed by a separately versioned redb reporting file; backfill with evidence | No; backfill is required |
| 3 | Promised summary/daily/24h/miners routes | Build narrow aggregates sharing the reporting file; backlog summary and miners can ship earlier | No |
| 4 | Legacy network aggregates | Do not build a parallel `/v1/network/stats`; use status and the promised stats routes; defer supply and mempool aggregation | No |

Rent history has greater operator value than general charts, but the evidence pipeline makes it more work. No DuckDB, full-chain balance-event index, persistent per-address checkpoints, arbitrary SQL, price data, or collector profit accounting in this proposal.

## 2. What the implementation actually provides

The source of truth is `crates/xp-store/src/{tables,rows,apply,rollback,read}.rs`, supplemented by `genesis.rs`, `lib.rs`, the rollback tests, `xp-wire/src/lib.rs`, `xp-types/src/rent.rs`, and the API handlers/DTOs/router. These differ materially from the September 5 design:

| Existing fact | Design consequence |
|---|---|
| Schema v2 has 26 tables; spent `BOXES` and `TREE_BOXES` entries survive | Historical balances do not need `balance_events` |
| `BoxRow.creation_height` is the box's declared height; its `tx_id` resolves to `TxRow.height` | Balance visibility uses inclusion height, never rent creation height |
| Seeded genesis boxes have zero transaction id and belong to no block | Explicit genesis handling is necessary |
| `HeaderRow.first_tx_gidx`, `TxRow.first_out_gidx` and counts locate contiguous transaction/output ranges | Backfill can walk indexed blocks without scanning all transactions per block |
| `BoxRow.spent` contains spending tx id and inclusion height | A current snapshot can reconstruct earlier end-of-block UTXOs |
| `RENT_MATURES` values are box ids; spent entries are removed | It answers today's backlog, not historical claims or past backlog |
| Header stores `miner_pk`, fees and reward; no output-volume aggregate | Miner keys and fee counts are cheap; economic volume and supply are not available as single reads |
| `DecodedTx` and `TxRow` keep input ids, not proofs/extensions | Exact rent classification cannot be backfilled from v2 rows alone |
| Wire decoding can retain an authoritative box id even when serialization verification fails; the warning flag is not persisted | Stored `size` alone is insufficient evidence for exact historical rent rules |
| `blocking()` uses a semaphore (default 32), fails fast, and holds its permit inside the blocking closure | New reads must retain these lifetime and overload guarantees |
| No reporting task, rolling stats or miners aggregate exists; the router has only rent eligible/upcoming | This document proposes new work, not merely exposing hidden data |

Amounts use the current house convention: integer decimal strings in nanoERG and raw token units, never the old human-readable ERG decimals. Block timestamps in these examples are Unix milliseconds, consistent with stored data; date selectors are UTC calendar dates. Ids are full lowercase hex; addresses remain base58. Example addresses abbreviated as `{address}` are placeholders, and all example figures/ids are illustrative rather than observations.

### 2.1 Reversibility is a release requirement

Core reads add no writes, so balance/history requests have zero effect on apply or rollback. Keep the existing fingerprint tests unchanged.

There is a distinction in the current code that must not be hidden: `Store::fingerprint()` hashes ordered table names, keys and value bytes for `tables::ALL`, **excluding `UNDO`**. It does not hash the physical redb file. Apply prunes old undo records, and rollback does not restore those pruned records. Thus the tests establish indexed-content identity, not literal whole-file or all-undo identity. MVCC allocation also makes physical-file equality a different requirement. This proposal preserves the tested guarantee and requires exact byte restoration of every new reporting data row. If “byte-identical” includes physical allocation or pruned undo history, that is an existing unresolved requirement, not something these features can claim to satisfy.

Do not quietly exclude new indexes from identity checks. The separate reporting store gets its own full logical-content fingerprint covering every table and metadata key. Compare before apply and after rollback, including absent versus zero-valued keys. Operational counters and timing live in memory, not in those tables.

## 3. Shared API and resource contract

Use axum `/v1`, existing per-IP limits, `blocking()` and one core `Reader` snapshot per request. New reporting reads also run under bounded blocking admission; do not open the live core file from another process. No network calls in HTTP request handlers. Use checked wide accumulators, deterministic token ordering, and decimal strings on output. Overflow or a dangling indexed reference is an internal integrity failure, not a clamped balance.

Lists return `{items, next_cursor}` plus documented snapshot/coverage metadata. Default `limit=50`, valid range 1–500; apply the existing parser's house behavior for oversized values. New structured cursors are versioned, opaque base64url encodings with strict length/type validation. They bind the route, normalized filters, ordering, anchor height and block id, reporting generation/classifier version where applicable, and the exclusive last examined key. They contain no accumulating monetary totals. Reject malformed/mismatched cursors with 400. A replaced or unavailable anchor is 409 `snapshot_changed`, requiring a restart. An anchor outside the currently published reporting generation/coverage also requires a restart, even if its core block still exists. New blocks above an unchanged anchor do not invalidate it. Reused gidx values on a fork cannot silently join two histories.

Use the existing RFC 7807-shaped `type`, `title`, `status`, `detail` object. Proposed typed failures add stable `code` and relevant bounded metadata; publish problem responses as `application/problem+json` for these routes (the current generic renderer uses JSON). Preserve 429 and 503 with `Retry-After`. The five-second outer timeout is not cancellation of `spawn_blocking`: every scan also checks its work budget and a cooperative deadline, at least every 128 rows. Historical scalar scans have a four-second cooperative deadline (below the five-second outer timeout); pages yield after 250 ms, after consuming at least one candidate. Exhaustion returns a problem, never an apparently exact truncated sum.

Suggested rejection:

```json
{
  "type": "about:blank",
  "title": "Unprocessable Content",
  "status": 422,
  "detail": "Historical balance exceeds the synchronous work budget; sum the anchored box pages.",
  "code": "history_scan_limit"
}
```

Add an expensive-read admission ceiling of 2 within the existing global ceiling, rather than allowing 32 historical scans to pin old MVCC pages simultaneously. Initial response budget is 2 MiB; a single oversized result receives 422, not silent token omission. These are proposed operating limits, not measured performance claims.

## 4. Historical address balances

### 4.1 Question and endpoint

“What confirmed ERG and tokens did this address own after all transactions in block H?” This supports auditing a rent collection and comparing balances around it. Mempool, intra-block balances and fiat values are excluded.

`GET /v1/addresses/{address}/balance/at?height=1866000`

```json
{
  "address": "{address}",
  "at": {
    "height": 1866000,
    "block_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "indexed_height": 1867450,
  "balance": {
    "nano": "868750000",
    "box_count": 1,
    "tokens": [
      {
        "token_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "amount": "42"
      }
    ]
  },
  "complete": true
}
```

Require `height` as u32. Accept height 0 only for a genesis-seeded full store, with `block_id: null` and explicitly defined pre-block genesis state. A future height is 400 `height_not_indexed`. An uninitialized or partial store is 503 `history_unavailable`; partial ingestion tolerates missing inputs and cannot establish exact ownership, even at its current tip. A valid unseen address has a zero balance; malformed/wrong-network addresses receive 400. This deliberately improves on the current helper that conflates invalid and unseen addresses as 404; validation must derive the tree hash independently of whether `ERGO_TREES` contains it. A tree first seen after H also has zero balance at H.

**Implementation clarification (September 10):** This explorer currently encodes and validates mainnet addresses and has no persisted network identity. Exact history therefore requires the three recognized retained mainnet genesis records as well as the genesis-seeded flag and absence of `partial_from`. Other genesis identities, including testnet, receive `history_unavailable`; a generic seeded flag alone cannot authenticate zero-creator boxes. This adds no metadata or migration. Token lists are sorted by token id. `history_response_limit` is the stable 422 code for an oversized result, distinct from `history_scan_limit`. The shared error renderer now uses `application/problem+json` for existing errors as well as these routes.

An optional `block_id` binds the requested height to the caller's intended canonical block; mismatch returns 409. The response anchors even single-shot calculations so clients can interpret subsequent changes.

**Timestamp lookup is deferred**, rather than pretending timestamp order equals height order. The old contract mentions timestamps but gives no precise semantics. A later `timestamp` selector should resolve to the greatest canonical height whose header timestamp is at or before the requested instant, returning that height/id and explicitly saying it is header time. Do not binary-search heights on an unproven monotonicity assumption. The reporting timestamp index in §6 could support a bounded resolver later; this release rejects `timestamp` with 400 and documents height-only support.

### 4.2 Algorithm and retained data

For each historical box belonging to the derived tree, define birth as the creating `TxRow.height`; verified seeded genesis boxes have birth 0. The box contributes exactly when `birth <= H` and `spent` is absent or `spent.height > H`. Sum its value and token amounts, and count it once. A box created and spent in H contributes zero. Declared `creation_height` may predate inclusion by years and must never make an unincluded box visible.

Walk `TREE_BOXES` ascending, resolve through `BOX_BY_GIDX` and `BOXES`, and look up the creator in `TXS`; cache creator lookups within the request. gidx is inclusion order, so once a non-genesis birth exceeds H, stop. Do not assume the largest box's declared creation height gives this bound. Treat a missing creator as corruption unless the box is one of the seeded genesis records; a zero id alone in an arbitrary partial store is insufficient. The scan ignores current token metadata and the current holder indexes: raw historical quantities need no metadata enrichment, and minted/burned-now values are not historical supply.

At the snapshot's tip, use `TREE_BALANCE` directly after full-history validation. This is O(number of held token entries), not necessarily constant response size. Initially avoid the alternative “current balance minus subsequent transaction deltas”: it adds same-tree, multi-input, token and data-input edge cases without eliminating worst-case work. It can be added only if measurements justify it.

No new table, row encoding, undo record, backfill or schema v3. Per applied block: **zero additional work**. Existing rollback deletes fork-created boxes and restores spent markers, so new readers see the winning history automatically.

### 4.3 Large-address escape hatch

The scalar endpoint examines at most 100,000 candidate boxes and aggregates at most 100,000 qualifying token entries, also subject to time/memory limits. It either returns the complete balance or 422 `history_scan_limit`. A cap is acceptable only with a complete, bounded way to obtain the result:

`GET /v1/addresses/{address}/boxes/at?height=1866000&limit=500`

```json
{
  "at": {
    "height": 1866000,
    "block_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "items": [
    {
      "box_id": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "inclusion_height": 1700000,
      "nano": "868750000",
      "tokens": []
    }
  ],
  "next_cursor": null
}
```

This is a minimal historical UTXO projection, not today's `BoxDto` with potentially misleading current spent/rent fields. Scan at most 1,000 candidate boxes per page, returning at most `limit` qualifying boxes; token/response budgets can end a page earlier. At the indexed tip, pages scan `TREE_UNSPENT`; older anchors scan `TREE_BOXES`. Candidates spent by H are skipped before creator lookup; candidates born after H are skipped without ending the walk. Deadlines yield a continuation rather than a scan-limit error. Cursor advances over the **last examined** candidate, even if none qualified. Empty `items` with a non-null cursor is valid. `next_cursor: null` alone means the address's candidate range through H is exhausted. Before consuming a candidate, ensure it fits the page's token budget; if a single box cannot fit, return a typed size error. This prevents dropping part of a box or livelock.

Clients sum all pages at the same anchor. Later spends above H do not change membership at H; any reorg changing H invalidates the anchor. No HTTP session holds an MVCC snapshot open across pages. This is preferable to resumable scalar sums embedded in client-controlled cursors.

### 4.4 Cost and tests

For B examined address boxes and K token entries: O(B + K) decoding and aggregation, roughly two box-index point reads plus up to one creator lookup per box, and O(distinct tokens) memory. B is address-local, never the whole chain. Typical small-address target is p95 ≤50 ms; a 10,000-box scan may miss 250 ms on cold production storage and must terminate predictably. Neither target follows from today's unrelated p95 of 25 ms; benchmark cold and warm reads separately.

Test balances against an independently maintained fixture UTXO set after every height: genesis, unknown addresses, late inclusion with old declared height, same-block creation/spend, spends exactly at H and H+1, multiple inputs/outputs to one tree, mint/transfer/burn, empty balances and large amounts. Test scalar versus summed pages, sparse empty pages, token budgets, invalid selectors, partial stores, cancellation/permit release, and forked/reused gidx cursors. Re-run existing apply/rollback fingerprints unchanged and compare historical results after rollback/reapply with a fresh winning-chain store. Walk at least 10,000 historical candidates with no duplication or omission.

## 5. History of rent actually claimed

### 5.1 Question, granularity and route

“Which boxes were spent through the storage-rent validation path, in which transactions/blocks, and how much value was removed?” Use the existing design's **`/v1/rent/recent`**, not a competing `/storage-rent/collections` route. Despite its name it is a cursor-paginated historical feed. One item is one expired input, so recreation and amount attribution remain auditable; it is not one opaque transaction total.

`GET /v1/rent/recent?from_height=1866000&to_height=1867450&limit=50`

Both bounds are inclusive. Default `from_height` is the beginning of verified coverage and default `to_height` is the published rent watermark. Optional `miner_pk` filters by block producer, not by claimed collector identity; its index is specified below. Descending only, by `(height, tx_gidx, input_index)`. Do not offer a collector-address filter before collector attribution exists.

```json
{
  "as_of": {
    "height": 1867450,
    "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
  },
  "coverage": {
    "from_height": 1,
    "through_height": 1867450,
    "classifier_version": 1
  },
  "items": [
    {
      "height": 1866000,
      "timestamp": 1788566400000,
      "tx_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "input_index": 0,
      "box_id": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
      "kind": "recreate",
      "recreated_box_id": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "input_nano": "1000000000",
      "returned_nano": "868750000",
      "rent_nano": "131250000",
      "amount_status": "exact",
      "evidence": "verified_rent_path",
      "miner_pk": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
      "collector": null,
      "collector_status": "not_attributable"
    }
  ],
  "next_cursor": null
}
```

Return 503 `rent_history_not_ready` if the requested range is outside published coverage; expose available coverage in the problem. Defaulting to a recent coverage start is always visible, never described as all-time. A complete empty range means zero claims. A missing evidence range does not.

### 5.2 Classification and honest attribution

Replace September 5 §6.5's age-only detector. Old inputs can be spent normally by their owner. Same-tree change outputs, multiple expired inputs, and ordinary inputs mixed into a rent transaction also invalidate “input minus all value returned to the same tree.”

The reference interpreter checks age, empty proof and context variable 127, decodes an output index, then validates the selected output. Its recreation checks preserve registers except R0/R3, require current creation height, and impose a lower bound on returned value; the insufficient-value branch differs. See the inspected [pinned reference interpreter](https://github.com/ergoplatform/ergo/blob/49b9f0fe7d0eba1a5ff81e524353acdd9a3cc6dd/ergo-wallet/src/main/scala/org/ergoplatform/wallet/interpreter/ErgoInterpreter.scala). This is evidence for the design, not proof that this historical commit covers every currently active consensus version.

The classifier must reproduce the applicable rent-path decision for the block's protocol version and effective parameters. Empty proof or extension 127 alone is not enough; exceptions can fall back to ordinary script validation. Exclude ordinary spends and data inputs. Retain decoded extension type/index, proof emptiness, verified serialized input size, applicable parameter values, version and selected-output evidence. Preserve register/token comparisons using canonical values, not JSON whitespace.

Rent parameters are versioned inputs, not timeless constants. The current explorer hard-codes period 1,051,200 and fee factor 1,250,000. Official [fee documentation](https://docs.ergoplatform.com/mining/rent/rent-fees/) describes a voted fee factor and a proposed version-gated repair; do not treat a proposal as activated consensus. Establish parameter history from validated epoch extensions or an audited pinned schedule before publishing verified coverage. If that cannot be established, this feature remains unavailable for the affected range.

`rent_nano` means value removed from expired boxes under the rent path, **not the configured maximum charge, transaction fee, miner revenue or collector profit**. For a verified unique recreation mapping, use input value minus the selected output value when nonnegative. A top-up has zero removed value, with `amount_status: "topped_up"` and its actual `returned_nano`; count it as a rent-path input, not positive rent. For a verified absorb branch, the input's full value is released, with no recreated box and `returned_nano: "0"`. Value equal to the charge belongs in the boundary fixtures.

Do not independently subtract a shared recreation output from every input. If multiple inputs reference one output, or mixed flows prevent a unique amount allocation, retain verified classification but set `rent_nano: null`, `amount_status: "ambiguous"`; keep the evidence for transaction-level examination. Aggregate these as unknown-amount claims. This conservative definition avoids manufacturing exact totals from fungible flows; finer attribution is deferred.

“Who collected” is not generally derivable as a single person or address. `miner_pk` identifies the header's producer key; it is not a pool label, transaction submitter, or proof of the rent recipient. Keep `collector: null` in the initial contract. The transaction id links to all actual output recipients for audit. A future collector attribution rule needs separately tested evidence and an attribution status; never guess the collector as the first/last output or first input address. This is an explicit limit on the old endpoint's implied promise.

### 5.3 Why a backfill and separate file

Age filtering can find candidates using existing inputs and boxes, but v2 discarded the decisive proofs/extensions and parameter evidence. Adding a boolean to future `TxRow`s would both change core encoding and leave history missing. Do not do that.

Use a separately versioned `reporting.redb` owned inside the explorer process. It is a rebuildable projection, with a single reporting writer and its own atomic transactions. It never opens `explorer.redb` through a second database handle and never writes the core file. The process passes bounded core snapshots/owned records to the reporting task. This is an explicit replacement for the unbuilt DuckDB worker in §7, not an assumption that such a worker already exists.

Backfill scans canonical block/transaction ranges, resolves inputs, and filters candidates by the applicable maturity rule. Fetch original canonical block transactions from an explicitly configured archival source only when needed for candidate evidence, plus parameter history. Verify fetched block identity and transaction commitment using the appropriate protocol serialization/commitment rules; matching transaction ids alone may not authenticate proof/extension bytes. Missing or unverifiable evidence stops rent coverage before the gap. No node calls are made as part of this design task; no production access is needed to review it.

At live tip, extend the in-memory wire/source handoff to retain this evidence, or refetch through the reporting worker. This is future implementation work and does not change v2 disk encoding. Validate box serialization against the authoritative box id; do not trust the discarded `id_verified` flag. Keep sufficient normalized evidence to rerun the chosen classifier; changing it may still require original blocks, which must be documented in the reporting format version.

No v3 or full core reindex. Full rent coverage still requires a potentially substantial sequential core scan and archival reads. Start with a clearly labeled recent coverage interval if desired; build older history as a separate generation and publish only a contiguous range. Never skip missing blocks and call the result complete.

### 5.4 Cost and tests

Measured rate from the brief is 3,950 claims / 1,450 blocks ≈2.72 claims/block. At an assumed 720 blocks/day that is roughly 1,961 claims/day or 716,000/year **if the sampled rate persists**. The brief does not establish whether “claims” counts inputs or transactions; confirm before sizing. At 0.5–1 KiB per normalized event plus indexes/evidence overhead, budget approximately 0.35–0.70 GiB raw events/year and provision 1–3 GiB/year initially. Real evidence size and fragmentation must be measured.

A feed page is one ordered scan of at most 501 event rows, O(limit); miner-filtered reads use a prefix index, not sparse global filtering. Per block the reporting worker does O(inputs + outputs + evidence bytes) classification, O(claims) event/index writes and one block contribution. Core apply adds only bounded handoff overhead and no new database writes; reporting stalls must not block ingestion. A missed handoff is repaired by replay, not silently lost. No network latency is hidden in the per-block CPU estimate.

Tests must include real, pinned recreate and absorb fixtures; a normally signed old-box spend; empty-proof ordinary scripts; invalid/missing/wrong-type extension and fallback; maturity minus one/exact/plus one; insufficient/equal/sufficient value; preserved and changed registers/tokens; top-ups; shared output mappings; mixed ordinary/rent inputs; fee outputs; wrong serialized size; parameter/version transitions. Validate classifier decisions against the reference validator for the applicable version, not against the old heuristic. Test forks, repeated delivery, missing evidence, crash points and coverage publication under §6. Compare independently summed exact event amounts with rent daily/summary results and retain unknown counts.

## 6. Reporting storage, backfill and rollback

### 6.1 Minimal projection

All keys use ordered big-endian fields. No change to `tables::ALL`, `SCHEMA_VERSION`, `UndoRow` or other core encodings.

| Reporting table | Key | Value / purpose |
|---|---|---|
| `report_meta` | fixed names | format/classifier versions; immutable coverage start; contiguous stats and rent watermarks with block ids |
| `block_facts` | height | block id/parent, timestamp, tx count, fees, byte count, miner pk; one compact row for every covered block |
| `block_time` | timestamp, height | empty; exact time-range index, irrespective of timestamp order by height |
| `rent_blocks` | height | block id, claim-input count, distinct claim-tx count, recreate/absorb counts, known removed value, unknown amount count; includes zero-claim blocks |
| `rent_time` | timestamp, height | empty; time index for verified rent contributions |
| `rent_events` | height, tx gidx, input index | identifiers, amount fields, normalized verification evidence and classifier version |
| `rent_by_miner` | miner pk, height, tx gidx, input index | event key |

Separate stats and rent watermarks allow header-only reporting to progress when rent evidence is unavailable. Each domain has one immutable coverage start per generation. This avoids a full interval-coverage engine. Do not persist daily totals initially: summing compact block contributions is sufficient and makes forks much easier. No global `balance_events`, timestamp index in the core, or per-token analytics table.

With roughly 1.87 million blocks and an illustrative 160–240 bytes of block facts plus time key, raw header reporting is about 300–450 MB, before redb overhead. Rent blocks can begin at verified rent coverage, rather than duplicating irrelevant early history. Plan disk headroom from a sample, not the old 30–40 GB index estimate, which the current 89 GB store has already exceeded.

### 6.2 Writer, restart and publication

The worker reads committed core state in short bounded snapshots, extracts a bounded batch of owned records, releases the snapshot, and only then does archival I/O. One worker serializes all reporting writes, backfill installation and rollback. Notifications are wakeups, not the durable record of applied blocks: catch-up compares stored heights/ids against the core after startup and after every wakeup.

Before committing a batch, recheck its canonical anchor and contiguous parent chain using a fresh core snapshot. Stale work is discarded. A core reorg can still occur after that check because the two files have no shared transaction: **request-time anchor validation is mandatory**, even for a freshly published projection. Commit data rows, secondary entries and the domain watermark atomically in the reporting file. A crash before commit leaves none; a crash after commit leaves the whole batch. Events have deterministic keys; replay of the same block is idempotent, while a differing block id triggers reconciliation rather than overwrite-in-place.

For an API request using reporting, open a reporting snapshot and a core snapshot within the bounded blocking closure. Select a reporting anchor A, ensure A is no higher than the core snapshot tip and its id equals `HEADERS[A]`, and read only reporting rows through A. Their persisted parent linkage and contiguous coverage establish the prefix. A mismatch returns 503 `reporting_reconciling`, not a mixed-fork response. The answer is consistent with those snapshots even if a fork commits after they open. Bind subsequent cursors to A. A request combining current backlog and historical claims requires rent watermark equal to that core snapshot's tip; otherwise return 503 with lag/coverage instead of combining different “now” values.

Expose an additive `reporting` object in `/v1/status`: availability, each coverage start/height/id, lag and sanitized last error. This is operational visibility; monetary endpoints validate database snapshots rather than trusting watch-channel status.

### 6.3 Exact logical rollback

Find the common ancestor by comparing stored reporting block ids with core headers. For every reverted block, descending: remove its event keys and miner keys, remove its timestamp secondary keys using retained timestamps, remove its contribution/fact row, and restore the appropriate previous watermark. Keep the coverage start/version metadata unchanged. Absent pre-first-block watermarks must become absent again, not height zero. No running totals or clocks need arithmetic reversal. Perform the reconciliation in a single reporting write transaction, or hide the generation until a larger rebuild completes.

At each domain's first covered block the predecessor is its explicit coverage boundary, not an invented zero-count block. If a reorg crosses the boundary, mark the generation unavailable and rebuild with a new canonical anchor. A deep core reorg remains subject to the existing 1,000-block halt policy; reporting does not override it. If reporting is too far behind or cannot find a common ancestor, rebuild reporting only.

Fingerprint the entire reporting logical state before applying a block and after reversing it. Include secondary indexes and metadata. Test batches, zero-claim blocks, same-block create/spend, a reorg changing dates/miners, restart after either file commits, and replay of already indexed blocks. Test byte equality of preexisting encoded rows as well as aggregate query equality. Do not repair a failure by weakening fingerprints.

### 6.4 Backfill operating limits

One low-priority worker; initially ≤100 blocks per core snapshot with a 50 ms extraction target, shrinking batches when needed. Bound owned batch bytes (initially 16 MiB), archival concurrency (2), and retry/backoff. The worker yields between batches and pauses when API load or disk latency exceeds configured thresholds. No chain-long read transaction pins the 89 GB file's pages.

At 100/1,000/5,000 blocks per second, a 1.87-million-block header pass takes approximately 5.2 h / 31 min / 6 min. Rent input resolution and network evidence can be much slower; these are throughput scenarios, not promised completion times. Benchmark a 10,000-block sample and project from measured bytes/second, candidate density and source fetch latency. Compare this cost with a 19 h local or roughly two-day production **full resync**, not with a fictitious free migration. A reporting version change may rebuild this file; it never requires deleting the core index.

## 7. Complete the promised reporting routes

### 7.1 Rent summary

Question: “What is available now, what matures next, and what was actually removed recently?”

`GET /v1/rent/summary?blocks=720`

`blocks` defaults to 720, valid 1–4,320. Recent is inclusive `[max(1,H-blocks+1),H]`; upcoming is `[H+1,H+blocks]`, matching the existing upcoming interval. A block window is not labeled 24 hours.

```json
{
  "as_of": {"height": 1867450, "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
  "window_blocks": 720,
  "eligible": {"box_count": 274, "estimated_due_nano": "35962500000", "complete": true},
  "upcoming": {"box_count": 80, "estimated_due_nano": "10500000000", "complete": true},
  "recent": {"claim_inputs": 1960, "claim_transactions": 1100, "known_rent_nano": "257250000000", "unknown_amount_inputs": 0, "amounts_complete": true}
}
```

Scan the entire relevant `RENT_MATURES` ranges in the same core snapshot, resolve box value/size, and use the current explorer rent estimator. Label values **estimated due**, with estimator version/fee factor available as response metadata in the final DTO: v2 does not retain serialization verification and its constant factor may not track voted values. These are gross protocol estimates, not economically collectible profits. Add an explicit `estimator` object containing `version: 1`, `period_blocks: 1051200`, `fee_per_byte_nano: "1250000"`, and `parameter_source: "explorer_constants"`. The example above abbreviates this metadata only.

Cap each backlog/upcoming scan at 10,000 entries with lookahead. If exceeded, return counts/value as lower bounds with `complete: false` and `lower_bound: true`; never use a 500-item page as an exact total. Recent aggregates require complete verified coverage for the whole recent window; until ready return 503 rather than implicit zero. Unknown-amount claims set `amounts_complete: false` while `known_rent_nano` stays the sum of known values.

No core v3. Core apply cost zero; reporting per-block cost is §5/§6. Request cost O(eligible + upcoming + window blocks); today's backlog means roughly 274 box resolutions before upcoming. Target p95 ≤50 ms for that measured scale, benchmarked separately. Test backlog changes at exact maturity, spent removal/reorg restoration, 10,000/10,001 boundaries, differing core/reporting tips, zero claims and ambiguous amounts.

### 7.2 Rent daily

Question: “Is collected rent increasing, and which days have incomplete attribution?”

`GET /v1/rent/daily?from=2026-09-01&to=2026-09-02&limit=50`

```json
{
  "as_of": {"height": 1867450, "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
  "items": [
    {"day": "2026-09-01", "claim_inputs": 1960, "claim_transactions": 1100, "recreated_inputs": 1900, "absorbed_inputs": 60, "known_rent_nano": "257250000000", "unknown_amount_inputs": 0, "amounts_complete": true, "complete": true}
  ],
  "next_cursor": null
}
```

UTC interval is `[from 00:00,to 00:00)`, `to` exclusive; ascending day order. Require both dates, `from < to`; max requested span 366 days, at most 31 output days per page even if limit is larger. Cursor binds the date range and anchor and resumes at the next day. Sum `rent_blocks` through `rent_time`; count each rent transaction once per block, independent of input count. Zero-fill only covered zero days. Coverage clipping produces `complete: false` for a boundary/current day; wholly unavailable history receives 503. `complete` concerns temporal coverage; `amounts_complete` separately concerns attribution. Include coverage metadata from §5 in every response.

Dates use header timestamp, not arrival time; no monotonic timestamp assumption. Every value is “as of” its canonical anchor. A later block may have an earlier timestamp; thus even a previously complete calendar bucket can be revised at a later anchor, and a reorg can revise any affected bucket. For a coverage interval starting after height 1, do not infer temporal completeness from its first timestamp. Mark buckets incomplete unless the full `block_time` index proves that no excluded prefix height has a timestamp in the bucket. Likewise a rent watermark behind the selected stats anchor cannot certify the uncovered suffix. Expose covered contributions as partial only when some coverage exists; never zero-fill an uncertified gap as an exact zero. Apply the same temporal-coverage check to stats windows.

No new tables beyond §6, no v3; O(block contributions in requested page) reads, roughly 22,320 rows for 31 nominal days. Cap at 50,000 contributions/time budget; return 422 asking for a narrower range if exceeded. No writes per request. Per-block writes are already covered by rent projection. Test daily sums against events, UTC boundary milliseconds, out-of-order timestamps, empty/partial days, pagination, rent coverage gaps and reorg across midnight.

### 7.3 Network activity: 24h and daily

Question: “How much confirmed activity and fee demand did the chain see?” Start with blocks, transactions, bytes and fee outputs. Do not publish a misleading `volume` field that sums change, emission remainder and fee-collection churn. Active/new addresses and economic transfer volume from old §7 are deferred; neither is a header sum. Do not sum per-block distinct-address counts to obtain daily active addresses.

`GET /v1/stats/24h`

```json
{
  "as_of": {"height": 1867450, "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
  "from_timestamp": 1788566400000,
  "to_timestamp": 1788652800000,
  "blocks": 720,
  "transactions": 42000,
  "fees_nano": "84000000000",
  "block_bytes": "72000000",
  "complete": true
}
```

Define `to_timestamp` as the reporting anchor header timestamp, inclusive; window is `(to_timestamp−86,400,000,to_timestamp]`, restricted to heights ≤anchor. This is explicitly **chain-tip anchored**, not wall-clock 24h; clients use `/v1/status` lag to judge freshness. A stale index must not look like a live wall-clock interval. Do not substitute the last 720 blocks. Query `block_time`, which handles non-monotonic header timestamps correctly. Limit 50,000 scanned contributions/time budget, returning a typed limit error rather than a partial exact sum.

`GET /v1/stats/daily?from=2026-09-01&to=2026-09-02`

```json
{
  "as_of": {"height": 1867450, "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
  "items": [
    {"day": "2026-09-01", "blocks": 720, "transactions": 42000, "fees_nano": "84000000000", "block_bytes": "72000000", "complete": true}
  ],
  "next_cursor": null
}
```

Share rent daily's date, anchor, coverage and pagination rules, using stats coverage. Header transaction counts include emission/fee-collection transactions; bytes mean stored `HeaderRow.size` (decoded block size), not a claim about network bandwidth. Fees sum outputs locked to `FEE_TREE_HASH`, as apply already does; input/output differences are not transaction fees.

One `block_facts` and `block_time` entry per applied reporting block, O(1) relative to block size; no core writes or v3. Header-only backfill is local, not archival. Roughly 720 compact contributions per nominal 24h, 22,320 per maximum day page. Initial warm-read targets: p95 ≤50 ms for 24h and ≤150 ms for 31 days. Do not recompute 720 headers on every core apply as old §6.4 proposed. A bounded in-memory result cache is optional, keyed by range, anchor block id and reporting format; eviction/TTL is never the reorg-correctness mechanism.

Test exact header sums, fees from fee outputs, empty blocks, timestamp regressions, long/short block intervals, stale tip, partial coverage, scan caps, midnight forks and crash recovery. Keep reward and circulating supply absent: current `block_reward()` explicitly assumes emission is transaction zero and carries an emission-end TODO.

### 7.4 Miners

Question: “Which producer keys found the last W blocks, and what was their observed share?”

`GET /v1/miners?window=720&limit=50`

```json
{
  "as_of": {"height": 1867450, "block_id": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
  "window_requested": 720,
  "window_actual": 720,
  "items": [
    {"miner_pk": "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798", "blocks": 720, "share_pct": "100.00"}
  ],
  "next_cursor": null
}
```

Here `window` is an integer block count, default 720, range 1–4,320. Scan `HEADERS` for `[max(1,H-W+1),H]` in one snapshot; group by the full 33-byte miner pk, sort count descending then pk ascending, paginate that result. Cursor includes anchor, window and exclusive `(count,pk)` key. `share_pct` uses actual covered block count, rounded to two decimals; an empty initialized history returns empty items. A partial store must report its actual covered window and `complete: false`, never infer missing producers. Add this completeness field to the final DTO.

No name/address guessing, no hashrate inference from short-window shares, no arbitrary windows. Core redb alone suffices; zero new writes, backfill, undo changes or v3. O(W) header reads and O(M log M) sorting for M producer keys, capped at 4,320. Optional bounded memory cache uses anchor id/window; validate the anchor on every page. Target p95 ≤100 ms at max window, pending measurement. Test ties, one/many producers, partial/short windows, 500-item pages, invalid windows, forked cursors and shares rounding independently (not necessarily totaling exactly 100.00).

## 8. Legacy network stats: intentionally retire the shape

The old `/api/v1/network/stats` mixes height, supply and mempool counts with different sources and freshness. The user question is a dashboard, not a reason for another overlapping backend resource.

The replacement client sequence is `GET /v1/status` for indexed/best/lag/source health and `GET /v1/stats/24h` for confirmed activity, with optional `/v1/stats/daily` and `/v1/miners?window=720`. Example status projection (existing fields; the real response contains additional operational fields):

```json
{"indexed":1867450,"best":1867452,"lag_blocks":2,"source_error":null}
```

Do **not** implement `/v1/network/stats`, including as an alias. If called, it remains 404; during migration document the mapping rather than redirecting a compound response to a different payload. No store data or per-block work is required for a route that is not built. Tests assert the documented composition fields, independent freshness, and that no legacy-style aggregate route is accidentally introduced.

Defer circulating supply until there is a precise emission/re-emission/treasury/locked-funds definition and a verified implementation; current balance sums include emission reserves, and summing current reward fields is not a substitute. Defer mempool counts to the originally promised `/v1/mempool/*` source-proxy work, with explicit node observation time and failure state. Neither is provided by pretending a missing analytics engine already exists.

## 9. Reconciliation with the September 5 design

| Original section/route | This proposal |
|---|---|
| §6.4 rolling stats/meta and miners writes | Read compact indexed contributions; bounded core header scan for miners; no new core hot aggregates |
| §6.5 age-only rent claims, same-tree subtraction | Superseded by input-level verified rent-path classification and explicit unknown amounts |
| §7 DuckDB range analytics | Replace this feature subset with separately versioned redb reporting; broader token analytics remain deferred |
| `/v1/rent/recent` | Canonical verified historical feed (§5) |
| `/v1/rent/summary` | Current estimated backlog/upcoming plus verified recent removals (§7.1) |
| `/v1/rent/daily` | Daily verified contributions and attribution completeness (§7.2) |
| `/v1/stats/24h`, `/v1/stats/daily` | Defined confirmed header activity; volume/address metrics explicitly deferred (§7.3) |
| `/v1/miners?window=` | Bounded integer block window, header producer keys (§7.4) |
| `/v1/rent/eligible`, `/upcoming` | Keep current purpose; they are not historical UTXO queries |
| Missing historical balance | Add `/v1/addresses/{addr}/balance/at` and bounded `/boxes/at` (§4) |
| Legacy `/network/stats` | No parallel endpoint (§8) |

These bounds and few numeric sums do not justify a SQL engine: month pages touch tens of thousands of compact rows, claim lists need ordered keys, and historical balances already have an address index. DuckDB could become useful for arbitrary multi-year/token joins and exact distinct-address analytics; none is in this release. Introduce it only after a measured workload shows these bounded redb reads cannot meet their budget and an independent disk/backfill/reorg plan beats the small projection here.

## 10. Delivery gates and unresolved questions

The task's strict implementation priority supersedes the earlier suggestion to ship miners/header statistics ahead of rent:

1. Ship historical balance and box pages first. Validate against a fixture UTXO oracle, including logical apply/rollback fingerprints, bounded scans and forked cursors. Run all required Rust/frontend gates before declaring this feature release-ready. Benchmark small and worst-case addresses; no migration.
2. Establish historical rent validation rules, effective parameters and authenticated evidence acquisition. Pin recreate/absorb fixtures and classifier versions. Then build the separately versioned reporting lifecycle and fixture-proven backfill, before exposing `/v1/rent/recent`. Missing authenticated evidence is a blocking accuracy gate, not permission to publish an age heuristic. A human runs every real backfill.
3. Only after rent history, implement `/v1/rent/summary`, `/v1/rent/daily`, `/v1/stats/24h`, `/v1/stats/daily`, and `/v1/miners`, with their coverage and rollback gates.
4. Do not implement `/v1/network/stats`.

The supply review is a prerequisite outside that feature sequence: `/v1/supply` reports explicitly defined gross allocation outside the original emission reserve, retains `emitted_nano` only as a deprecated alias, and leaves `circulating_nano` null. The rich list includes protocol reserves and labels its percentage against total genesis allocation. Neither is the deferred circulating-supply dashboard.

Acceptance measurements must include cold/warm p50/p95, rows/bytes examined, cancellation completion time, blocking-pool occupancy, writer throughput and redb growth under concurrent ingestion. Use the brief's existing p50 11 ms / p95 25 ms as the regression baseline for existing routes, not as evidence for the new routes. Initial rollout gate: no more than 20% p95 regression on the same existing-route workload during throttled backfill; tune or pause the worker if exceeded. Verify ≥10,000-row cursor walks, no duplicate keys, and repeat with append and fork events.

Open issues are specific: current consensus-version coverage of the pinned rent reference; trustworthy historical parameter/transaction-proof commitments from the configured source; real input-versus-transaction claim density; box-size reconstruction failures; prevalence of shared recreation mappings; archival retention and projected backfill duration; cold production scan latency; and whether literal file-byte equality is intended beyond existing logical fingerprints. Collector identity remains unavailable without additional evidence. Timestamp balance lookup, economic volume, exact daily active addresses, supply and mempool dashboards remain deliberately deferred.

If a future design adds persisted birth height, balance checkpoints, proofs to `TxRow`, or new undo fields directly to the core under its current no-migration policy, that proposal **does force schema v3 and the full resync**. None of the selected designs does. Separately versioning and rebuilding reporting is a real deployment/backfill cost, but it preserves the expensive v2 index.
