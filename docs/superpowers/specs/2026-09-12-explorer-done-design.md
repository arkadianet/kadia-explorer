# Explorer completion — Design

Date: 2026-09-12. Status: proposed for review; no implementation or operational validation performed in this round.

Baseline: `fix/explorer-exit-code-and-rent-truth`, `8a0e3dc`, three correctness commits above `7516976`. Companion: `docs/superpowers/plans/2026-09-12-explorer-done-plan.md`. Paths and line references below are relative to this repository at that revision.

## 1. What DONE means

DONE means a trustworthy, bounded, recoverable **confirmed-mainnet explorer for the routes already shipped**: discover a block, transaction, box, address, token or script; inspect indexed facts, balances and existing height-based history; inspect current rent eligibility with its existing qualifications. It means the operator can establish what version was tested, distinguish an unavailable answer from zero, diagnose saturation after a restart, and restore the index without rebuilding 89 GB of history. It does not mean completing every endpoint in the September 5 draft.

This is a deliberate reduction of product scope. The September 5 design §1–§3 combines the core explorer, miners, analytics, DuckDB and live updates under one goal, before the store existed. The September 10 history/rent design §1–§2 already replaces major parts: retained box lifetimes replace a balance-event index; a separate reporting redb file replaces broad analytics; exact rent history needs evidence absent from core v2. The ops design explicitly retained full global/block transaction expansion (`2026-09-07-ops-hardening-design.md` §2). Those choices now leave a concrete resource risk in a shipped path, whereas none of the six promised reporting routes has a shipped caller contract to preserve. Closing that risk has greater value than adding another dashboard.

The completion boundary is therefore **all six milestones below passing**, including operational evidence. A green fixture suite alone is not DONE. Neither an all-skipped parity run nor an unexecuted recovery runbook qualifies. Existing legacy pagination remains usable as an explicitly best-effort compatibility interface; the first-party frontend uses the strict continuation contract in §4.2. Erroring on an oversized legacy request is acceptable; returning a successful prefix as a complete block is not.

### 1.1 CUT list — removed from this release's obligations

| Cut | Reason and consequence |
|---|---|
| `/v1/miners` | Header miner keys are available, but availability is not a user requirement for another aggregate service. Existing block miner facts suffice. No miner ranking, pool attribution or miner page. |
| `/v1/stats/24h`, `/v1/stats/daily` | Exact rolling/time-bucket statistics introduce window, timestamp and coverage semantics plus maintenance work. Keep current bounded, honestly labelled sampled homepage views; do not advertise them as exact network analytics. No replacement home-summary service. |
| `/v1/rent/summary` | Current eligibility pages already answer the retained inspection task. An exact global backlog total is useful but not necessary for trustworthy inspection; no hidden aggregate/backfill prerequisite is justified. |
| `/v1/rent/recent`, `/v1/rent/daily` | Remove the verified-claims reporting product from this release, including its reporting file, worker, backfill and classifier. Core v2 cannot prove rent execution from input ids. Age, value loss and an explorer-generated fixture are insufficient evidence. This expressly supersedes the September 10 proposal to deliver claims before aggregates. |
| DuckDB, generic reporting/query engine, full-chain balance-event index, timestamp history selector, historical holder rankings | These are separate products or optimizations without a demonstrated need in retained routes. Height history already exists without a schema change. |
| Mempool and WebSocket | Preserve the explicit deferral; confirmed data and polling remain the product. No pending-state or real-time transport promise. |
| New circulating-supply dashboard, fiat/price data, protocol integrations, token-name discovery, pool attribution, testnet expansion | Existing supply fields retain their current meanings; these expansions do not close the known integrity or operating gaps. |
| Homepage transaction-kind badges that require full box expansion | Remove those presentation badges when the homepage moves to summaries. Detail pages retain their existing transaction information. Do not add an index or infer classification from counts to preserve decoration. |
| A guaranteed 30–40 GB physical index, byte-identical redb files, restoration of pruned historical UNDO | Retire unsubstantiated targets. Replace the disk promise with measured attribution/headroom and logical identity with an explicit retention contract. This does not waive disk measurement or undo testing. |
| Legacy canonical-chain guessing | Stop supporting ingestion through first-id `/blocks/at` selection when `chainSlice` is unavailable. Fail closed rather than implement a second canonicality protocol for old nodes. Body-only fallback remains. |

These are cuts, not tasks parked at the end of this plan. Reintroduction requires a new scope decision, costs and independent evidence. Existing routes and fields are not removed. Existing rent classification rules and labels are not redesigned by this cut. No reporting file is created speculatively.

### 1.2 Baseline and evidence boundaries

The reviewer's inputs are accepted, including absent CI, the missing `WORK-REPORT.md`, the 89 GB footprint and unrun restore/resync drill. They were not remeasured. The four listed recent defect fixes are closed baseline work and have no implementation milestone here.

Additional source references explain the remaining design, rather than reopening closed findings:

| Evidence at baseline | Consequence |
|---|---|
| `crates/xp-source/src/rust_node.rs:32`, `:62`, `:127`, `:148` | The legacy canonicality downgrade is distinct from body fallback. Remove the downgrade, not the body retrieval facility. |
| `crates/xp-api/src/dto.rs:566` accepts an absent input box; `crates/xp-store/src/read_tokens.rs:250` documents skipped metadata rows for partial stores | Missing dependencies need full-store versus partial-store classification. Do not indiscriminately turn every optional enrichment into corruption. |
| `crates/xp-api/src/handlers/blocks.rs:83` and `handlers/txs.rs:11` | Full expansion remains; summaries alone do not contain callers of the old routes. |
| `crates/xp-store/src/extras.rs:146` | Every present R4–R9 value creates a register index entry. Six registers per box is not a total index-cardinality limit. |
| `crates/xp-store/src/lib.rs:122`; `crates/xp-store/src/apply.rs:146` | Existing fingerprint excludes UNDO; apply prunes it. Stronger testing needs a retention model, not an assertion of impossible full-history equality. |
| `crates/xp-api/src/handlers/history.rs:70`, `:204`; `crates/xp-api/src/handlers/blocks.rs:42` | History already has structured anchors; ordinary lists still parse bare continuation keys. Reuse validation concepts, not assumptions that all list rows are immutable. |
| `crates/xp-api/tests/parity.rs:177`, `:202`; `scripts/parity.sh:1` | Coverage floors and nonzero inconclusive outcomes already exist. Enforce and execute these facilities; do not repeat the older audit's implementation recommendation as open work. |
| `frontend/src/lib/api/endpoints.ts:28`; `frontend/src/routes/+page.svelte:339` | Frontend currently requests expanded transactions and derives kind badges from them. Summary migration needs an explicit presentation decision. |

No production HTTP, remote host, database open, disk statistics, benchmarks, restore or reference-node validation was performed for this document. No outside protocol claim is newly established here. Historical source statements in older documents are evidence leads, not new measurements.

## 2. Ordered milestones and release conditions

Sizes are engineer-days for one maintainer familiar with the workspace, including focused tests and review, excluding CI provisioning queues, obtaining reference captures and the final seven-day observation period. Total: approximately 21–32 engineer-days plus operational elapsed time. These are planning estimates, not a completion date.

| Order | Delivers | Why here | Observable acceptance | Size |
|---|---|---|---|---|
| M1 — Make gates unavoidable | PR/push CI, a reproducible local gate entry point, delivery-status authority and retained artifacts | Highest protection per unit of work; all later correctness changes need an automatic gate | Exact Rust/frontend gates green on the candidate revision; a disposable failing gate makes CI red; required-check configuration evidenced; delivery ledger distinguishes implemented, tested and released | 1–2 d |
| M2 — Fail closed and prove reversibility | Reject unsupported canonicality; required-row integrity errors; randomized apply/rollback tests including UNDO semantics | Stops plausible but wrong data before adding read interfaces; strongest correctness risk | Orphan-first/legacy fixtures never apply an unverified header; corruption fixtures return typed errors; 256 reproducible generated histories pass indexed-state and explicit UNDO checks | 4–6 d |
| M3 — Bound and observe public reads | Summary pages, frontend migration, cooperative legacy limits, bounded telemetry with durable collection | Directly contains the still-open transaction-expansion risk and supplies measurements for M4/M6 | Summary routes answer without box resolution; resource-bound tests pass; mixed-load run records latency/bytes/status/worker drain; pre-restart saturation remains queryable after restart | 5–7 d |
| M4 — Bound growth and prove recovery | Per-table attribution, register-cardinality circuit breaker, headroom policy, consistent backup and full-size restore drill | M2 protects state transitions; M3 supplies operating evidence. Recoverability precedes more pagination work on the 89 GB asset | Accounted file/allocated/logical bytes; measured growth; cap refuses an entire transaction atomically; verified full-size restore meets the recovery budget | 4–6 d |
| M5 — Make continuation honest | Additive strict pagination, explicit legacy consistency, frontend restart behavior | Requires bounded reads; avoids holding snapshots or creating a reporting store to solve a UI continuation problem | Multi-page fixture sets have no omissions/duplicates, or explicitly reject changed snapshots; mutable rankings reject any tip change; old JSON shape/cursors remain compatible | 4–6 d |
| M6 — Close the release with evidence | Independent parity, real-API browser smoke, capacity/recovery evidence and final delivery record | Must exercise the final behavior and artifacts, not an earlier passing commit | All gates at candidate SHA, existing parity floors met with outcome `pass`, seven-day soak within §5 budgets, restore evidence, no unexplained integrity mismatch | 3–5 d + 7 elapsed days |

The sequence is intentionally not reporting-first. CI precedes new code; integrity precedes optimization; bounds and telemetry precede capacity conclusions; recovery precedes a more convenient browsing contract. M5 could technically follow M3 before M4, but recoverability of the sole full index retires greater loss risk. Operational collection for M4/M6 can begin after M3 while other work proceeds. Book backup space and reference access during M1 so this dependency does not become an avoidable last-week surprise.

## 3. What the milestones do NOT do

| Milestone | Scope boundary |
|---|---|
| M1 | No deployment automation, production credentials in CI, hosted node requirement for PRs, dependency upgrade campaign or reconstruction of fictional past gate results. |
| M2 | No new consensus interpreter, multi-node voting, node-version adapter, store repair, schema migration or resurrection of pruned UNDO. Existing source-health and exit-code fixes are not rebuilt. |
| M3 | No reporting routes, analytics database, general metrics framework, distributed tracing or tuning concurrency upward to hide expensive reads. No renaming/removal of full transaction fields. |
| M4 | No production resync, automatic deletion of register entries, silent loss of search coverage, speculative compression, database-engine replacement or mandatory compaction. |
| M5 | No historical ranking index, server-held snapshot session, promise of immutable expanded box fields across new blocks, or silent reinterpretation of old cursor formats. |
| M6 | No new feature wave or visual redesign; any release-blocking failure returns to its owning milestone. No waiver based on a mock UI or a green build alone. |

## 4. Required implementation contracts

### 4.1 Integrity and logical reversibility

A source lacking a working canonical `chainSlice` lookup is unsupported for canonical selection, including when `/blocks/at` returns only one id. A single stored header is still not proof of best-chain membership. Preserve transient error handling and the primary-selected-id body fallback. Validate requested height/id/body linkage; contradictory or duplicate canonical responses must fail closed. This establishes consistency with the trusted primary's advertised chain, not independent consensus validation. Surface the capability failure through existing failure/status machinery; do not add a second competing source-health state machine.

Create an inventory of required references across list expansion, detail DTOs, token enrichment and apply/rollback. On a full-history store, a missing indexed primary row, indexed input or required token row returns `StoreError::Corrupt` and a 500 problem with an additive stable `code: integrity_error`; no successful partial DTO. Unknown user-requested ids remain 404. Optional name/decimals fields remain optional when the token exists but has no valid metadata. A partial store can legitimately lack pre-seed inputs/mints; preserve that explicit limitation and make completeness available additively on enclosing responses or headers where an existing bare array cannot be wrapped. A missing row that should have been created inside the indexed range is corruption even in partial mode. Do not claim retrospective detection of every missing pre-seed reference.

Keep `fingerprint()` as indexed logical identity; add test-only full logical snapshots including UNDO. Without pruning, apply/rollback restores every table byte including UNDO. Across a pruning boundary, require exact indexed identity plus exactly the retention-model-predicted UNDO keys/bytes; the keys pruned by apply stay absent. Assert the remaining rollback horizon and failure outside it. Do not weaken the existing fingerprint or exclude a new table. Random generation uses existing `proptest`, an independent simple UTXO/token model, deterministic seed recording and minimized replay cases; no production helper may compute the oracle result.

### 4.2 Additive lists and resource bounds

Add `GET /v1/tx-summaries` and `GET /v1/blocks/{height_or_id}/tx-summaries`, returning `PageDto<TxSummaryDto>` with the additional metadata below. Defaults remain 50, house limit at most 500. Global order is gidx; block order is the existing in-block transaction order, bounded by the header's contiguous range. Use existing `tx_summary_dto`. No input/output, tree or token-name expansion. Move global, block and homepage lists to these routes; fetch full details only for a user opening a transaction. Drop homepage classification badges as specified in the cut list.

The old `/v1/txs`, block transaction array and single-transaction route retain all existing fields and successful response shapes. Full block arrays are all-or-error: never truncate or replace them with a page. Apply explicit aggregate budgets during expansion, before unbounded allocation: initially 10,000 box resolutions and 2 MiB encoded JSON per response, four-second cooperative deadline below the existing five-second HTTP timeout, checks at most every 128 work units including token/register expansion. Enforce bytes during construction/serialization, not only after building an enormous value. A single too-large item is a 422 `response_limit`; cumulative work exhaustion is 422 `read_work_limit`. Include a bounded summary-route hint where applicable. Existing 429/503 and `Retry-After` remain. All expensive loops, including previously existing history routes, must retain their stricter existing budgets. A blocking worker retains admission until it actually ends; HTTP timeout is not cancellation.

These proposed defaults intentionally trade success for bounded work on very large legacy requests. Fixture and full-store results must report how often this occurs. If normal detail navigation cannot work within these bounds, DONE is blocked pending a measured limit adjustment or an additive detail projection, not a hidden omission of inputs.

For ordinary paged routes preserve existing `items` and `next_cursor` formats. Add `consistency` (`best_effort` or `strict`) and observed height/id metadata. Clients opt into strict mode with `consistency=strict`; strict responses add `next_snapshot`, a bounded opaque versioned continuation token to send as `snapshot` alongside the existing next cursor. Bind version, route, normalized filters, direction, anchor height/id and expected exclusive cursor. Reject a mismatched cursor/token pair or malformed token with 400; unavailable/replaced anchors with 409 `snapshot_changed`. Strict continuation with a cursor but no token is 400. Empty initial stores carry explicit unavailable/empty metadata, never invented height-zero block ids. Do not hold a Reader between requests.

Two policies are required:

- **Immutable projection:** blocks and transaction-summary lists (including address tx summaries) may continue after appends if the anchor block remains canonical; cap membership at the anchor's height/gidx. Block summaries also bind the selected block id. Verify every returned field is immutable before using this policy.
- **Current-state projection:** rich list, holder rankings, token lists with mutable supply/holder fields, unspent sets, rent eligibility, register/template/box lists and full transactions require the current tip height/id to equal the token's tip. Any advance, rollback or replacement is 409. Even all-time box membership has mutable spent/rent/enrichment fields. An old block id by itself cannot reconstruct these answers.

Existing `/balance/at` and `/boxes/at` retain their stronger historical semantics and existing cursor contract. The frontend carries token/cursor pairs and clears accumulated rows on 409, explaining that the chain changed and offering restart; do not automatically loop on a fast-moving tip. Legacy callers remain best-effort, explicitly documented and tagged; do not claim they receive strict snapshots. Requiring strict pagination unconditionally would change their contract. This compatibility exception is part of DONE, not an undisclosed unresolved promise.

### 4.3 Register growth and recovery

Measure register entry count and growth before selecting a production ceiling. Cap **total entries**, not only raw-value size, per-box register count or distinct values. Read the existing index cardinality using the pinned redb API; account for proposed batch additions in the same write transaction and checked arithmetic. A cap breach aborts the whole batch, preserving all tables/UNDO/meta and indexed tip. It must visibly pause/fail ingestion with a local capacity reason, without falsely blaming the source. Alert at 80% of the configured cap. Raising the cap after adding capacity can resume from the unchanged store. If the current count already exceeds a proposed cap, refuse deployment of that configuration; never drop old rows or silently skip new ones. The cap is an operator circuit breaker, not a consensus validity rule.

No row-codec/schema version change is expected; configuration plus existing table counts suffice. Verify count retrieval cost and atomicity using local redb sources before implementing. If it requires a scan, perform a bounded in-process initialization/maintenance pass; do not put a full-index scan on every block. Any cached count must survive rollback/restart by reconstruction or transactionally maintained compatible metadata and be tested accordingly. A migration that forces core resync is vetoed; stop and redesign rather than add it as an estimate contingency.

Attribute logical key/value bytes, table allocation/overhead, UNDO, free/reusable pages where exposed, apparent file bytes and allocated filesystem bytes. Record unreconciled categories explicitly; do not divide payload bytes by file size and call the remainder waste. Gather cheap statistics inside the owning explorer process; long scans use a stopped, consistent clone with one Store owner, not the live file through another process. Physical compaction is not assumed necessary.

Use a clean shutdown and confirmed process exit to copy the database and required config/identity manifest, then restart the original. Hash the completed copy; restore onto separate storage and open it with one isolated explorer process. Never `cp` a live redb file. Record capture anchor, schema, revision, checksums, copy duration, downtime, restore duration and catch-up. Verify restored sampled API facts against a capture manifest at the same anchor; then apply new blocks and demonstrate a shallow fork on a disposable restored fixture/clone as appropriate. A full-size restore is required; a tiny fixture proves only mechanism. No production resync, and no feature depends on one. A future isolated resync benchmark can be commissioned separately; successful restore plus catch-up is this release's recovery requirement.

## 5. Operating evidence and verification

Every milestone uses the exact gates below, first locally until M1 is active, then as required checks. PR fixtures use temporary databases, in-process router tests and loopback stub sources. No test opens the production file. CI is network-independent after dependency/browser installation; public reference-node access is a separate release gate.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
```

From `frontend/`:

```sh
npm test && npm run check && npm run lint && npm run build
npx playwright test --workers=4
```

Use locked installs and record the resolved Rust/Node/npm/Playwright versions. Existing `rust-toolchain.toml` says `stable`; M1 pins the tested Rust version rather than letting the required gate drift silently. Existing Playwright configuration starts its mock API and production preview; override workers with the specified command. Existing external crates and npm packages suffice; `proptest`, `tempfile`, `tracing`, atomics, axum and standard-library tooling are already available. No new package is authorized. GitHub workflow actions must be pinned and justified as build orchestration, not runtime additions.

| Milestone | Verification beyond the common gates |
|---|---|
| M1 | Deliberately fail one check on a disposable CI branch; verify merged changes require the named checks and store logs/artifact hashes. Missing permission to configure required checks is a reported operational blocker, not a reason to call YAML enforcement complete. |
| M2 | Full/partial corruption matrix; source HTTP stubs including orphan ordering and unsupported endpoints; randomized histories with spends, mint/burn, same-block spends, forked gidx reuse, batching, reopen and pruning boundaries. Deliberately mutate UNDO in a temporary store and demonstrate test failure. |
| M3 | Compare summary fields with fixture rows/full DTOs; prove zero box lookups via a test counter. Check exact budget boundary and one-over cases, cancellation and worker permit drain. Fixed route-template metric labels only; no ids, addresses, queries or unbounded error text. |
| M4 | Just-below/at/above-cap tests including rollback/reopen; byte-for-byte logical snapshot unchanged on rejection. Full-size stopped-copy restore and capture-anchor API comparisons. Store inventory and recovery artifacts retained outside the sole database disk. |
| M5 | More than two pages in each route family; ascending/descending, changed filters, forged/malformed/mismatched tokens, exact-cap and empty pages; append, reorg and mutable-key reordering. Frontend 409 must clear stale rows; legacy payload compatibility tests stay green. |
| M6 | Run existing parity wrapper at explicit endpoints with matched tip/hash and preserve machine-readable verdict and coverage. Existing floors: 20 each for balance, tx count and unspent ids; 5 each for token emission, name, internal supply and token unspent ids. Internal supply checks are not independent consensus evidence. Run real-API browser tests separately from mock e2e. |

M3 adds an operator-only metrics endpoint or equivalent bounded exposition using existing libraries: route-template request counts by status, fixed-bucket request and blocking-read duration histograms, response bytes, active readers, admission rejection, timeout, integrity failures, ingest lag/progress and register capacity. Bound metric cardinality with an explicit route/status whitelist. A process-start identifier/time distinguishes counter resets. Scrape every 15 seconds into an operator-owned collector retained at least 14 days and outside the explorer process; demonstrate retrieval across a deliberate restart. If no collector exists, a rotated external sampler using existing system tooling is sufficient, with disk limits and missed-sample detection. An in-memory endpoint by itself does not meet acceptance.

Proposed release budgets (not measurements): warm summary list p95 ≤100 ms and status p95 ≤100 ms at 10 requests/second for ten minutes on the recorded deployment-class hardware; zero unexpected 5xx at that nominal load. Separately run concurrency 1/8/32/64 with at least 1,000 requests per level, report successful latencies separately from 429/503/timeout counts, peak RSS and response bytes. After load stops, in-flight workers must return to baseline within five seconds on this test hardware; deadline tests also pin work-count limits independent of disk timing. Slow system calls cannot be preempted by a cooperative deadline, so a failing drain measurement remains a release blocker. Record cold-start and warm runs separately; do not flush shared production caches.

M4/M6 require a measured daily-growth series over seven days; free capacity must cover 30 days at the largest observed daily growth plus 20% of current allocated store bytes, with backup/restore staging separately provisioned. Logical-plus-allocator categories must reconcile apparent file bytes within 5%, or explain the residual with an independently checked engine/filesystem category. Recovery targets: backup capture downtime ≤30 minutes, restored service including catch-up ≤4 hours, backup age/RPO ≤24 hours. These are proposed minimum operating commitments, not guesses that the hardware will pass. If unmet, adjust storage/procedure or explicitly revise the specification with evidence before declaring DONE. An 89 GB index is acceptable only with attributed cost and these recovery/headroom commitments; merely changing the budget number is not acceptance.

## 6. Risks and unknowns

1. Production source capability is not checked. Removing the legacy downgrade may stop ingestion on its configured primary; probe the canonical endpoint before rollout and choose a supported primary if necessary. Building compatibility is cut, not an implicit follow-on.
2. Current database integrity is unmeasured. Fail-closed code may expose existing missing rows. No automatic repair or production resync is authorized by this plan. A detected defect blocks DONE until a separately reviewed recover/repair procedure resolves it.
3. Disk allocation, growth, backup bandwidth, independent storage and available maintenance windows are unknown. redb's exact statistics API must be checked against the locked local version; physical bytes cannot be inferred from logical payload alone.
4. Register ceiling may halt all ingestion because it protects an index in the core transaction. This is a conscious fail-closed tradeoff. If the measured limit is routinely reached, capacity changes or a separately designed, explicitly incomplete search product are required; silent skipping is forbidden.
5. Strict current-state pagination may restart frequently near tip. Preserving old wire shapes leaves a best-effort legacy mode. This release chooses explicit restart over expensive historical rankings; user experience must be tested on the real block cadence.
6. The four-second/2 MiB/read-count budgets, latency goals and recovery objectives need hardware evidence. They may reject unusually large legitimate details. Limits cannot be relaxed without recording new resource measurements.
7. CI runner capacity, repository required-check access, artifact retention and a durable metric collector are unverified. Workflow files alone cannot establish operational enforcement or telemetry retention.
8. Independent reference data may be unavailable at a stable comparable anchor. Existing parity is stronger than the old audit describes, but a passing artifact is still required. Missing evidence is inconclusive, not a pass.
9. Rent remains consensus-adjacent despite the reporting cut. Preserve existing behavior and evidence; any newly discovered inconsistency must be checked against a pinned authoritative implementation and captured inputs/parameters. Do not let a generated explorer fixture certify the rent rules. Verified historical claims stay cut unless a future evidence pipeline supplies proofs/extensions and parameter provenance.

The three least certain choices are cutting all six reporting routes despite the September 10 operator-value argument, using exact-tip restarts for mutable lists, and the proposed capacity/recovery budgets including the register circuit breaker. They are explicit review decisions; none is a claim about measured production behavior.
