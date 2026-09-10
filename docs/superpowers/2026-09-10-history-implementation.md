# Work report — 2026-09-10

## Part A — adversarial review of `993a11d`

**Verdict: the constants are correct; the claim that the subtraction gives exact circulating supply or “ERG in existence” is wrong.** Reviewed `git show 993a11d` before starting Part B. Corrections are in the working tree, not deployed. The Rust changes are **not release-ready**: compilation and Rust tests are blocked by the session's read-only default Cargo target (details below).

### Accounting findings

The three values match `tests/fixtures/genesis.json`:

| Allocation | nanoERG | ERG |
|---|---:|---:|
| Original emission reserve | 93,409,132,500,000,000 | 93,409,132.5 |
| Foundation treasury | 4,330,791,500,000,000 | 4,330,791.5 |
| No-premine proof box | 1,000,000,000 | 1 |
| Total | 97,739,925,000,000,000 | 97,739,925 |

The reference node's genesis construction was inspected in the local sibling `../ergo-scala/src/main/scala/org/ergoplatform/nodeView/state/ErgoState.scala`: miner allocation goes to the emission box; founders allocation minus one ERG goes to the treasury; the proof box receives one ERG. The foundation has its own genesis reserve and vesting contract. Describing its early share as paid out of the miner emission box is misleading.

`GENESIS_TOTAL_NANO - balance(original_emission_tree)` has a defensible, narrower meaning: **gross genesis allocation outside that tree**. It does not double-count re-emission coins numerically: they are counted once, but counted before they become available for future mining rewards. It also includes the full treasury allocation before vesting. EIP-27 redirects rewards through a proxy to a distinct re-emission contract, and some obligations remain attached to unspent miner rewards. Subtracting only the original reserve cannot net these out. See the primary [EIP-27 specification](https://github.com/ergoplatform/eips/blob/master/eip-0027.md).

All native ERG is allocated to boxes at genesis. Fees move existing ERG between boxes; they do not mint additional ERG or warrant a separate subtraction from this gross allocation. Sending ERG to an unspendable script is not a token-burn counter, and storage rent complicates claims of permanent unspendability. Lost keys and other locked funds are not measured here. The subtraction establishes neither spendability nor a definition of circulating supply. The previous test's 999-ERG difference from a hardcoded schedule did not establish either claim.

The rich list includes protocol reserves, including the very emission balance omitted from its former denominator. Consequently the column was not a partition of the stated supply and could exceed 100% early in history. Its old footnote also still said it ignored EIP-27. A coherent percentage for this list uses the entire genesis allocation and labels that choice explicitly.

### Fix and edge cases

- `/v1/supply` now declares `definition: "genesis_allocation_minus_original_emission_reserve"` and exposes `outside_emission_nano`. `emitted_nano` remains a **deprecated compatibility alias**, with its semantics documented. `circulating_nano` is explicitly null. `complete` certifies only the defined gross accounting.
- The rich list now divides by `genesis_total_nano` only when the API recognizes a complete mainnet store. The column is **“% of genesis allocation”**, with a visible explanation that reserves are included and this is not circulating supply. An emission balance of 93,409,132.5 ERG gives 95.56% under this definition.
- Above-genesis/partial stores still return unavailable amounts (`complete: false`). A partial marker takes precedence even if genesis records exist.
- The database has no persisted network identity. Recognizing all three retained mainnet genesis identities and amounts prevents applying mainnet accounting to an arbitrary/testnet seed merely because it supplied a largest box. Other identities return unavailable amounts; the constant `genesis_total_nano` continues to describe mainnet, not that other chain.
- At pre-block genesis (height 0 conceptually, `indexed_height: null`), gross allocation outside the original reserve is 4,330,792.5 ERG. This is deliberately not represented as circulating or vested supply.
- A recognized mainnet seed with missing/mismatched emission-tree metadata is an integrity error. A missing emission balance row is also an integrity error: apply retains balance rows at zero, so absence is not legitimate exhaustion.
- `checked_sub` replaces `saturating_sub`; a balance exceeding the genesis total produces 500, rather than hiding inconsistency as zero. A present zero row remains valid. Deposits to the emission tree are included in the definition; this is a tree balance, not a singleton-box tracker.

Tests added in `crates/xp-api/tests/routes.rs`: `supply_defines_gross_reserves_without_claiming_circulation`, `supply_does_not_apply_mainnet_constants_to_other_genesis`, and `supply_missing_balance_and_impossible_total_are_errors_but_zero_is_valid`. The existing partial-store test remains. These Rust tests are written but **not executed**.

Frontend tests call the actual shared percentage helper instead of duplicating its implementation. The new helper test failed before the implementation (missing helper), then both assertions/tests passed. Two browser tests exercise the actual rich-list page with known and unavailable mainnet accounting. The full browser suite passes. No schema, core write path, undo encoding, migration or resync changes were made for Part A.

## Part B — feature 1: historical address balances and box pages

**Implemented in the working tree, but stopped before declaring the feature complete: Rust compilation/tests and performance acceptance remain outstanding.** No later feature was implemented ahead of this gate.

Endpoints:

- `GET /v1/addresses/{address}/balance/at?height=H[&block_id=ID]`
- `GET /v1/addresses/{address}/boxes/at?height=H[&block_id=ID][&limit=50][&cursor=CURSOR]`

The new `handlers/history.rs` uses one core `Reader` snapshot per request. It derives the tree from a valid mainnet address independently of indexed address existence. A valid unseen address returns zero. Full recognized genesis history is required; partial/uninitialized/other-network stores receive 503 `history_unavailable`. Height is a required u32; timestamp and unknown selectors are rejected. Future single-shot heights are 400 `height_not_indexed`; unavailable/replaced pagination anchors are 409 `snapshot_changed`. Optional block ids bind single-shot requests too.

Historical membership uses the creator transaction's inclusion height and the spending inclusion height. Declared box creation height is not used for ownership. Recognized retained genesis boxes have birth 0; another zero creator or a missing creator is an integrity failure. A same-block create/spend contributes nothing to that block's end state. At the current tip, the validated current balance index is used directly.

The new store method streams address-local `TREE_BOXES` candidates in gidx order, resolving through `BOX_BY_GIDX` and `BOXES`. It validates index agreement. Creator lookups are cached within each scan. Historical quantities use checked u128 arithmetic and sorted raw token ids, with decimal strings in responses. No current token metadata is joined.

Scalar scans allow 10,000 candidates and 100,000 token entries. Pages allow 1,000 candidates, at most the requested qualifying count (house limit capped at 500), and token/response budgets. An unconsumed candidate is never skipped when a page fills. Empty sparse pages carry a cursor over the last examined candidate. A cursor has a version, route, tree filter, height/block anchor, and exclusive gidx; it has strict decoding/length/type checks and no monetary totals. Appends above H preserve an anchor; a fork at H invalidates it. Minimal box projections omit misleading present-day spent/rent fields.

Each scan checks a cooperative 250-ms deadline. Responses are capped at 2 MiB. Typed 422 errors distinguish scan exhaustion from response-size exhaustion. Expensive reads have a separate admission ceiling of two within the existing global ceiling; permits remain inside the blocking closure after request cancellation. Shared error responses now use `application/problem+json`; retry headers remain for overload/unavailability.

### Tests written and what they cover

All tests below are **unexecuted because Cargo cannot access its default target**; this table describes assertions, not claimed passing evidence.

| Test | Behaviour pinned |
|---|---|
| `history_genesis_unseen_and_invalid_selectors` | Genesis state, unknown-address zero, height bounds, unsupported timestamp, malformed address, scalar/page agreement, reads preserve logical fingerprint |
| `history_rejects_partial_store_and_legacy_network_stats_stays_absent` | Partial-store 503 and `/v1/network/stats` remaining absent |
| `history_matches_utxo_oracle_at_every_height_and_after_forks` | Independently maintained fixture UTXOs at every height; old declared creation height; same-block create/spend; exact spend boundary; multiple same-tree inputs/outputs; mint/transfer/burn; large integer values; summed box membership; append-safe cursors; forked/reused gidx rejection; winning-chain results agree with a fresh store |
| `history_walks_ten_thousand_candidates_and_sparse_empty_pages` | 10,001-candidate walk with no duplicate/omitted boxes; scalar cap; sparse empty pages progress; final qualifying box retained; apply/rollback fingerprint |
| `historical_tip_response_size_is_bounded_and_problem_headers_are_typed` | Large token response fails with typed 422 and problem content type |
| `cursor_validation_binds_route_filter_anchor_and_width` | Cursor version/route/type/width/unknown-field validation; mismatched height; scalar cursor rejection |
| `cooperative_deadline_overflow_and_output_bounds_fail_closed` | Deadline, checked arithmetic overflow and serialized response bound |
| `historical_admission_survives_cancellation_and_releases_on_errors` | Two-read ceiling, permit lifetime beyond handler cancellation, global admission and release after an error |

Existing fingerprint tests and their exclusions were not edited or weakened. New fixture assertions compare before apply and after rollback, including fork/reapply scenarios. **They cannot yet be reported as passing.** As the design already explains, `Store::fingerprint()` hashes indexed logical table bytes excluding `UNDO`, not physical redb allocation or pruned undo history. This work does not claim physical-file byte equality.

Schema/rollback impact: no table additions, schema version change, persisted fields, undo changes, or per-block work. No backfill or resync is needed for these read endpoints on an existing complete mainnet v2 store. Corruption fixtures edit only newly created temporary files while no Store owns them.

Acceptance still outstanding: compile/clippy; red/green Rust execution (the first attempted regression run was blocked before compilation); all new and existing rollback assertions; broader token-budget and malformed-reference coverage; cold/warm p50/p95 and concurrent-writer impact. The 10,001-box test includes elapsed-time output, but no measurement is claimed without running it. This is not a completed release gate.

## Features 2–4 — deliberately not started

| Priority | Feature/endpoints | State |
|---|---|---|
| 2 | Verified rent history, `/v1/rent/recent`, separately versioned reporting file and fixture backfill | Not implemented. No reporting file, worker, schema, classifier or backfill scaffolding left behind. Feature 1's Rust gate blocked further implementation in the required order. |
| 3 | `/v1/rent/summary`, `/v1/rent/daily` | Not implemented; depend on verified rent coverage. Existing eligible/upcoming routes are unchanged. |
| 3 | `/v1/stats/24h`, `/v1/stats/daily` | Not implemented; no header reporting/backfill worker added ahead of rent. |
| 3 | `/v1/miners` | Not implemented; did not use the old spec's “can ship earlier” suggestion to bypass the requested priority. |
| 4 | `/v1/network/stats` | Deliberately absent. No alias or redirect. A negative route assertion was added. |

The next rent-specific accuracy gate also remains real: v2 input rows omit proofs/extensions and verified input serialization state; there is no authenticated historical parameter/evidence pipeline here. The inspected reference interpreter has an extension-selected output, register comparisons, an insufficient-value branch, exception fallback, and parameter-dependent arithmetic. Age alone cannot prove rent-path execution. This investigation did not establish all active historical protocol versions or authenticated parameter coverage and therefore does not claim a verified classifier. No production or real-data backfill was attempted.

## Design changes

Updated `docs/superpowers/specs/2026-09-10-history-and-rent-reporting-design.md` explicitly:

1. Current delivery status is implementation awaiting Rust gates, not a completed release or a design-only document.
2. The old §10 early-miners/header-stats ordering now follows the task's strict history → verified rent → aggregates sequence.
3. Historical reads are explicitly mainnet-only until network identity/genesis support is designed; the generic seeded flag alone is insufficient to recognize arbitrary zero-creator records.
4. Documented the response-size error code and shared problem content type.
5. Distinguished corrected gross supply accounting and the rich-list denominator from the still-deferred circulating-supply dashboard.

## Gates and operational boundaries

| Required gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | BLOCKED before compilation |
| `cargo test --workspace` | BLOCKED before compilation; no Rust passing count claimed |
| `frontend: npm test` | PASS — 120 tests |
| `frontend: npm run check` | PASS — zero errors/warnings |
| `frontend: npm run lint` | PASS |
| `frontend: npm run build` | PASS, including bundle budget (~59 KiB gzip / 120 KiB budget) |
| `frontend: npx playwright test --workers=4` | PASS — 74 tests, four workers |
| `git diff --check` | PASS |

Both Rust execution gates fail with:

```text
error: failed to open: /home/rkadias/.cache/cargo-target/debug/.cargo-build-lock
Caused by:
  Read-only file system (os error 30)
```

That path is the configured default in `/home/rkadias/.cargo/config.toml`. The session may write only this workspace and `/tmp`, and escalation is unavailable. I asked whether `/tmp/ergo-explorer-cargo-target` may be used as an exception to “use the default”; no answer has arrived. I did not change `CARGO_TARGET_DIR`, write into the shared default cache, or silently override that instruction. Logs are in `/tmp/ergo-explorer-{fmt,clippy,rust-tests,playwright}.log` for this session.

**Human action before merging/deploying:** authorize an external writable test target for continuation, or run the Rust gates in a session with a writable default cache. Resolve any compile/test/lint failures and complete the missing acceptance checks before declaring Part A's backend or historical reads release-ready. No resync or real backfill is required for the changes present. The future rent feature will require a separately implemented, fixture-proven backfill and a human-run real backfill; there is no backfill command to run from this change.

Only read-only HTTP requests were made to `127.0.0.1:18091`: status reported indexed/best 1,869,747; `/v1/supply` returned no usable body there. The running instance was not restarted, its database was not opened or written, and no real data was backfilled. No SSH, production domain, or rent-collector VPS access occurred. No commit was attempted; all work remains in the tree.

## Feature 1 repair — real-chain follow-up (2026-09-10)

This section supersedes the earlier feature-1 gate status and scan-budget description. Part A was accepted and was not changed in this repair. Features 2 and 3 remain unstarted. No commit, service restart, production database access, migration, or backfill occurred.

### Findings and changes

1. **Confirmed stop/skip bug:** both handlers returned `false` for a candidate born after H. `visit_history_candidates` interprets that as termination. They now consume/skip that candidate and continue. Pages advance the cursor over skipped candidates, including future births. A successful terminal page now requires exhaustion of the chosen index range; encountering a nonqualifying box cannot assert exhaustion.
2. **Confirmed independent production failure:** the scalar and paged handlers shared a 250 ms fail-with-422 deadline, and scalar charged spent boxes against its 10,000-candidate cap. An address with a small current UTXO set can have tens of thousands of spent candidates. Creator lookups happened even for boxes already spent by H. The page route could therefore repeatedly fail at the same cursor instead of providing an escape hatch. Repeated read-only HTTP requests to the unchanged service returned `history_scan_limit` in 251–257 ms for the reported scalar queries and both addresses' tip pages. Raw evidence: `/tmp/history-running-baseline.json`.
3. **Real data changed the budget decision:** the first address has 36,139 retained candidates and 33 current unspent boxes in the captured data, rather than 3. It held **12,290** boxes at height 1,800,000. Even a separate 10,000-qualifying-box cap would reject this requested case; the initial replay caught that and the cap was removed. Scalar work is now bounded by 100,000 examined candidates, 100,000 qualifying token entries, the existing 2 MiB response bound, and a four-second cooperative deadline below the existing five-second HTTP timeout. Spent-by-H candidates are discarded before creator lookup or token aggregation. An actual budget exhaustion still fails closed with 422.
4. **Pages yield instead of failing on elapsed time:** a page consumes at least one candidate, then yields on its 250 ms deadline, 1,000-candidate cap, item limit, or token/response budget. The cursor names the last consumed candidate; the lookahead candidate is retried. Empty pages carry a continuation when candidates remain. A single unrepresentable box still gives a typed size error. At the indexed tip, pages use `TREE_UNSPENT` instead of walking the spent history. After a tip append, the same anchored cursor can continue over `TREE_BOXES`: earlier consumed gidx values stay excluded, and lifetime filtering preserves ownership at H.

**The reported empty-success response at tip was not reproduced.** During this repair the running service returned 422, including when queried with its freshly read tip. Exported creator inclusion heights were all at or below the capture tip (maximum 1,869,736 for the first address and 1,869,045 for the second). Thus there is no evidence that a future birth caused the reported tip-empty result, and this report does not present that as its cause. The independent deadline/candidate-budget failures above are reproduced, not inferred from that report. The changed handler's real-data replay does establish correct tip membership.

### Regression coverage and real-data verification

- Red run before changing the handlers: `history_spent_candidates_do_not_exhaust_scalar_budget` failed with 422 instead of the expected 3,000 nanoERG; `history_future_birth_before_live_candidate_does_not_end_walk` failed with zero items instead of one. `/tmp/history-red.log` records both failures. The former uses 36,140 candidates, 36,137 spent. The latter was subsequently strengthened from two candidates to 1,001 future-birth candidates preceding a qualifying box, explicitly requiring an empty first page with a cursor. Both pass after the fix.
- The existing 10,001-box test now requires a successful exact scalar result, and retains complete membership, uniqueness, sparse-page progress, and rollback assertions. The shared page helper rejects cursor cycles. Deadline unit coverage verifies that an expired page consumes its first candidate and yields after progress. Existing every-height UTXO, append/fork, and fingerprint checks still pass.
- Added `scripts/capture-history-fixture.py`: reads only public GET endpoints on `127.0.0.1:18091`, exports box rows, creator inclusion heights, and independently paginated current unspent IDs; refuses an export if indexed height changes during capture. No database file is opened. Capture `/tmp/history-mainnet.json` completed at height **1,869,757**. Full histories were exported for both reported addresses; a third address supplies a current-UTXO comparison.
- Added opt-in `history_http_snapshot_replay`. It imports those public box lifetime/token facts into a **new temporary** redb store and drives the actual changed Axum handlers. The import uses compact monotonic fixture gidx values preserving each address's exported order and synthetic anchor headers. It is not a full mainnet database clone or a measurement of the serving database's cache/I/O behavior. It compares every paged box ID against the exported current route at tip, and against an independent lifetime filter at older heights; also compares nanoERG totals and scalar token maps. All scalar historical queries below returned 200. No duplicate output IDs occurred.

| Address (prefix) | Height | Boxes | nanoERG |
|---|---:|---:|---:|
| `9gD9khJaxi3S…` | 1,500,000 | 0 | 0 |
| `9gD9khJaxi3S…` | 1,600,000 | 4 | 3015203326894283 |
| `9gD9khJaxi3S…` | 1,800,000 | 12,290 | 4647629928410164 |
| `9gD9khJaxi3S…` | 1,869,757 | 33 | 3611423476132324 |
| `9iKFBBrryPhB…` | 1,500,000 | 17,881 | 4742035356738393 |
| `9iKFBBrryPhB…` | 1,600,000 | 21,616 | 4711970840863093 |
| `9iKFBBrryPhB…` | 1,800,000 | 24,643 | 5153327386962349 |
| `9iKFBBrryPhB…` | 1,869,757 | **25,050** | 4945413087406407 |
| `9i51m3reWk99…` | 1,869,757 | 150 | 525069876929 |

Replay command:

```sh
python3 scripts/capture-history-fixture.py /tmp/history-mainnet.json
HISTORY_HTTP_FIXTURE=/tmp/history-mainnet.json cargo test -p xp-api --test routes history_http_snapshot_replay -- --ignored --nocapture
```

Replay passed in 56.60 s including temporary import and all comparisons; `/tmp/history-replay.log` contains per-case elapsed times. These include complete pagination and oracle/scalar comparisons, not per-request latency. They are not cold/warm mainnet p50/p95 or concurrent-writer benchmarks. The unchanged service necessarily continues serving its already loaded executable; verification of the new code was through temporary-store replay, not a claimed hot update at port 18091.

### Re-run gates

All used the configured default Cargo target, now writable in this session.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — **222 passed**, 3 ignored (including opt-in HTTP replay) |
| Explicit HTTP snapshot replay | PASS — **1 passed**, separately executed |
| `npm test` | PASS — **120 frontend unit tests** across 16 files (the current runner reports 120, rather than the 121 in the task) |
| `npm run check` | PASS — zero errors/warnings |
| `npm run lint` | PASS |
| `npm run build` | PASS — bundle budget 58.96 KiB / 120 KiB |
| `npx playwright test --workers=4` | PASS — **74 e2e tests** |
| `git diff --check` | PASS |

Logs: `/tmp/history-{fmt,clippy,rust-tests,frontend-unit,frontend-check,frontend-lint,frontend-build,e2e}.log`. The previously fixed `ReadableTable` imports were preserved. All changes remain uncommitted in the tree.
