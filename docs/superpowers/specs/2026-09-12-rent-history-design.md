# Explorer storage-rent history — Design

Date: 2026-09-12. Status: proposed for review; design only. No implementation, node scan or operational benchmark performed in this round.

Baseline: explorer `838d2cf`. Paths below are relative to this repository unless stated otherwise. This document supersedes the rent recent/daily cut in `2026-09-12-explorer-done-design.md` §1.1 and refines the rent portions of `2026-09-10-history-and-rent-reporting-design.md`. Other completion cuts remain outside this proposal.

## 1. Decision and evidence

Restore `/v1/rent/recent` and `/v1/rent/daily`, with confirmed input evidence, transaction accounting and explicit coverage. Build both forward capture and historical backfill into a separately versioned reporting file. Core `SCHEMA_VERSION` stays **2**; no core row, table, undo encoding, resync or migration changes. An incompatible reporting file must never prevent the 89 GB core store from opening.

The owner's measured domain knowledge is authoritative input. Rent claims have recreated-output and fully-consumed shapes, empty spending proofs and output pointers; fee-free miner self-claims differ from fee-paying bot claims, including CPFP families. The evidence is available by re-reading blocks from the configured node. The earlier argument confused evidence absent from the index with evidence absent from the chain; it does not justify cutting history.

Reuse the independent September 2 census over **1846000..1864008 inclusive: 18,009 blocks and 5,116 rent transactions**, approximately 25 days. Its age-first selection and input-owner-tree accounting are the starting algorithm and external reconciliation target. Do not substitute the September 10 spec's earlier 3,950/1,450 rate or extrapolate a ten-block tip sample. The reviewer's verified observations stand without remeasurement: emission and fee-collection transactions have empty proofs in every block, but their inputs are only 1–2 blocks old; ten recent blocks contained zero mature empty-proof inputs. Claims are episodic.

The implementation inspected here confirms that `DecodedTx` and core `TxRow` retain input ids, not proofs/extensions; retained spent boxes supply input content; `BlockSource::full_block_json` already retrieves `/blocks/{id}`. `xp-store/src/apply.rs` defines the full standard fee tree and sums fee **outputs**. Existing dependencies (`redb`, `reqwest`, `serde_json`, `ergo-lib`, hashing and async tooling) suffice. No new external dependency is proposed.

## 2. Classification contract

### 2.1 Candidate selection and exact positive predicate

For a confirmed transaction T at inclusion height H, resolve every spending input box, including earlier outputs in the same block. Data inputs are excluded. Use declared box creation height for rent age, not the creating transaction's inclusion height.

The census candidate predicate is exactly:

`candidate(T,H) = ANY spending input b with b.creationHeight <= H - 1051200`.

Evaluate this without unsigned underflow: below 1,051,200 there is no candidate for a nonnegative creation height. Apply age before expensive proof/shape work. Keep all candidates in the audit projection, including ordinary old-box spends; this preserves the independently proven census and makes any refinement measurable.

For a supported historical rule set P(H), a **verified storage-rent transaction** is one with at least one input satisfying all of:

1. Age at least 1,051,200 and an explicitly present, zero-length spending proof. Missing JSON is not an empty proof.
2. Context variable 127 decodes with the exact type accepted by P(H) to an in-range output index j. Retain the original encoded constant and j.
3. The applicable rent branch accepts the selected output, using authenticated input serialization, effective parameters and historical arithmetic as described below.
4. The evidence belongs to the canonical confirmed block at H and is complete enough to establish that branch; an ordinary-script fallback is not a positive rent result.

Only **one** input must qualify; additional inputs can have signatures, fund fees or serve ordinary purposes. Requiring every transaction input to have an empty proof would miss mixed claims. Other inputs are classified separately, and one unresolved input does not erase a verified input; it does make the transaction's classification coverage incomplete.

The positive predicate concerns the validation path, not the submitter's intention. A script that could also spend without a signature is still a rent-path spend when the applicable interpreter actually selects and accepts that path. Age or empty proofs alone do not establish that selection.

### 2.2 The two shapes

Let V be the input's nanoERG value, S its canonical serialized byte length, and Q the storage charge calculated under P(H). In the inspected reference rule, Q is the signed 32-bit product of size and effective storage-fee factor, including wrapping. Q is a protocol limit, **not** the measured claimed amount or a transaction fee.

| Shape | Accepted rent branch | Reporting treatment |
|---|---|---|
| Recreated | `V - Q > 0`; pointed output has creation height H, value at least `V - Q`, and preserves R1/R2/R4–R9, including exact tree, token sequence and register values; R0/R3 may change | Record input → output edge and actual returned value. Equality with the minimum is not required. |
| Fully consumed | `V - Q <= 0`; the pointer must still decode and select an existing output, but that output need not preserve the owner's properties | Record `shape: fully_consumed` and the pointer as branch evidence, **not** as a refund edge. An arbitrary payout/absorber output can be selected. |

Shared pointers are permitted as evidence where the rule permits them; never subtract a shared output once per input. A fully-consumed branch can voluntarily return value to the owner. Its gross input value is therefore not automatically the actual rent withdrawn. A recreated output can be topped up; preserve a signed owner-tree delta and do not invent a positive rent charge. Do not use the current eligible-list `is_rent_claimable` helper as a historical classifier: an economic profitability filter is not the full validation-path predicate.

The local Scala reference at `/home/rkadias/coding/development/arkadianet/ergo-scala`, HEAD `4a7dba059794054fc4c81f5d990dafdeb4f97a49`, contains this rule in `ergo-wallet/src/main/scala/org/ergoplatform/wallet/interpreter/ErgoInterpreter.scala`. Its working tree extracts the multiplication into a helper; record that patch with any future oracle artifact. Its pointer cast is `Short`, and decoding/index exceptions fall back to ordinary verification. The nearby Rust node at HEAD `016533194f94ad95b1a87df70bb9bfce493922e2` accepts both Short and Int in `ergo-validation/src/tx/script/storage_rent_check.rs`. This is an inspected implementation difference, not a disagreement with the owner's observations. Do not union the two acceptance sets or declare either checkout a complete historical oracle. Pin the authoritative validator/version and parameter schedule for each range; an Int-pointer case without that evidence is unresolved. The older remote reference linked by the September 10 spec could not be fetched during this round; these observations come from local source inspection, not a newly executed validator.

Store parameter provenance by epoch/rule interval: effective factor, period, protocol version, source block/extension ids and validator revision. Obtain it from the configured node's historical extensions/state evidence or an independently audited schedule, never current `/info` alone. Unsupported parameter history produces unresolved candidates in affected intervals; it does not stop scanning unrelated supported intervals or justify dropping the feature.

### 2.3 Outcomes and error risks

Each candidate input has `verified_rent`, `ordinary_spend`, or `unresolved`, with a stable reason and evidence reference. An age-qualified signed spend is ordinary under this predicate. A known missing rent selector can establish ordinary-path selection when the complete raw input is present. A decode exception that might select fallback requires the pinned rule; malformed/missing source data is unresolved, not ordinary. Contradictory evidence from a supposedly valid block is an integrity/source error, never a heuristic classification.

False positives from age-only normal owner spends, empty-proof emission/fee collection, and generic empty-proof scripts are removed by the complete predicate. Remaining false-positive risks are wrong historical rules, incorrect serialization/type handling or a dishonest/misconfigured trusted node. No claim of independent consensus validation is made. False-negative risks are missing/pruned bodies, unresolved input content, unsupported rule transitions or decoder defects; expose their counts and affected intervals. Sparse hits and mixed inputs must not become early-stop or all-input filters. A transaction with only unresolved candidate inputs remains visible for audit and contributes to neither certified claim count nor certified rent total.

## 3. Amounts, fees and claimant attribution

### 3.1 Reuse owner-tree accounting

For every census candidate, compute and retain the original census quantity:

`census_owner_delta = sum(all spending input values) - sum(outputs whose exact tree occurs among those inputs)`.

This is a checked, signed integer in nanoERG. Construct a set of exact input trees and count each output once, regardless of repeated input trees or pointers. It is always an observable transaction accounting result when all boxes are available. For the clean claim shapes in the census it measures gross rent leaving the input owners. Preserve it to reconcile the independent census, rather than replacing that method with nominal size-times-factor charges.

For the certified amount, also group **verified rent inputs** by exact owner tree G:

`owner_delta(G) = sum(verified rent input values on G) - sum(all parent outputs on G)`.

When the parent contains only verified rent inputs and has no owner/payout role collision, these deltas sum to the census quantity and give exact gross rent withdrawn. Pointers establish classification and recreation obligations; owner-tree accounting measures actual retained value, including voluntary returns on the fully-consumed branch. A shared recreation output is subtracted once at tree/transaction level; a per-input allocation may remain null even when the total is exact.

Ordinary funding on separate trees does not prevent exact accounting of isolated rent-owner trees. If ordinary and rent inputs share a tree, a payout tree is also a rent-owner tree, or external funding returns to those owner trees, the net flow is still computable but its allocation to rent versus ordinary activity requires an assumption. Set `rent_nano: null`, `amount_status: ambiguous_flows`; expose the census and owner deltas. Do not divide change proportionally or silently subtract unrelated funder inputs. Negative deltas are retained as `owner_top_up_nano`/signed audit deltas; report zero positive withdrawal only where the separated owner flows justify it. Do not cancel one owner's top-up against another owner's withdrawal without showing both. Tokens are preserved/checked as branch evidence; no token-to-ERG valuation is proposed.

### 3.2 Parent fees and the miner/bot discriminator

`parent_fee_nano = sum(parent outputs locked by the full standard fee contract)`.

Match the complete canonical tree bytes (using the existing pinned fee-tree definition), not the reviewer's abbreviated prefix. Fees are explicit outputs: `sum(inputs) - sum(all outputs)` is a conservation check, normally zero, **not** the fee. Fee-collection spending of those outputs must not count their value again.

Gross rent withdrawn is measured before fees. For a clean, solely rent-funded parent without a child, retained collector proceeds are `gross_rent - parent_fee`. For example, 1 ERG removed and 0.01 ERG in fee outputs means 1 ERG claimed and 0.99 ERG retained. These are illustrative arithmetic examples, not chain fixtures.

Use the owner's structural discriminator as a first-class field: a verified claim with fee outputs is `bot_fee_paying`; a claim with no parent fee output is provisionally `fee_free_parent` until CPFP resolution. A fee-free parent in a confirmed fee-paying family is a bot-family claim, not a miner self-claim. Only after complete family analysis finds no such family, classify the fee-free structure as `miner_self_claim` under the supplied domain rule, exposing `mode_basis: owner_structural_rule`. Unresolved family analysis leaves the mode `fee_free_parent`, never a guessed miner classification. Parent fee zero is exact; miner identity or proof of who constructed/submitted it is not. Direct miner payout is rent proceeds, not an invented transaction fee. Funding inputs can make fee source allocation uncertain even though the exact fee value is known.

### 3.3 Confirmed CPFP families

Build a dependency graph from confirmed output spends, including later transactions in the same block. Exclude owner recreations and standard fee-collection edges from collector-family linkage. Start from the parent's non-owner, non-fee outputs, and record direct children consuming those outputs. `0008d3` is a generic absorber, never a claimant identifier or sufficient family/identity test by itself.

The initial recognized CPFP shape is a parent claim and a same-block child spending its collector/absorber output and creating explicit fee output(s), optionally with additional funding. Follow a bounded same-block connected component for multiple parents/children and grandchildren; retain every txid and edge. A child may itself contain a rent input; deduplicate that claim at transaction level. Exclude the shared fee-collection transaction, which would otherwise join unrelated claims into one enormous false family. A later-block spend cannot boost an already-confirmed parent's inclusion; expose it as a later payout spend, not CPFP. Chain data proves dependency and fees, not that the miner used package selection or that boosting was the submitter's intent; label `family_basis: confirmed_dependency` and `cpfp_intent: not_determinable`.

For the resolved component, `family_fee_nano` is the sum of standard fee outputs from its unique parent/child transactions. Each fee output is counted once. Internal parent→child outputs cancel when computing boundary inputs/outputs; an absorber transfer is not extra rent. Gross rent is counted once at each verified rent parent, with any recognized family-boundary refunds to rent-owner trees shown separately. Record both parent withdrawal and family owner retention when these differ.

For a family solely funded by unambiguously measured rent, with identifiable owner returns and terminal payout outputs, `net_proceeds = family_gross_rent - family_fee` (adjusting any additional owner refunds exactly once) is exact for the family. Mixed child funding, several rent parents or unrelated activity still allow exact total fee and boundary flows, but allocation of that fee to one claim or payout key is **not uniquely determined**. Per-claim allocated fee/net is null by default. An optional future allocation policy must be named and versioned; it must never overwrite exact family fee. Do not label a known subset of fees as a complete family total when graph bounds, missing evidence or unresolved roles interrupt traversal.

Resolve families for the whole block before publishing its claims. Initially cap analysis at 10,000 graph edges and 16 MiB owned evidence per work item; oversized work is explicitly `family_status: unresolved_limit`, not silently truncated. Different limits can be adopted after measurement. Same-block closure keeps records immutable under appends; delayed payout observations are separate facts anchored to their own height.

Report actual terminal payout P2PKs and amounts, with `attribution_status: observed_payout`. They identify recipient keys, not necessarily a legal person, collector operator or miner. Non-P2PK terminal outputs remain visible with unknown attribution. Any owner-approved collector-name mapping must cite payout keys and be separately versioned. Neither `0008d3`, absence from a live ledger, collector-template resemblance, nor mempool sightings establishes a rival. The measured median 15 competing txids per contested box represents re-staging; unmined txids and mempool-derived identity never enter this projection.

## 4. Evidence acquisition and sequencing

Implement both paths with one classifier and one reporting writer:

1. Establish pinned chain/validator/parameter provenance and an evidence corpus, then replay the census window **1846000..1864008**. Require exact txid-set agreement with the independent census export for all 5,116 transactions in that inclusive window, plus agreement on owner-tree amounts; matching counts alone is insufficient. Its count is accepted now; the detailed export is a future validation artifact, not something reconstructed from explorer classifications.
2. Enable forward capture from committed core blocks. Pass raw proof/extension evidence through a bounded in-memory handoff, or re-read it in the worker. Do not alter persisted `DecodedTx` projections/`TxRow` encoding. Core apply has no network wait and no reporting-file transaction. Wakeups can be dropped safely because committed core heights/ids are the durable replay queue.
3. Generate historical candidates locally from retained `BOXES`, then verify only their distinct spending heights from the configured node (§5), newest useful interval first, then older chunks. Forward work has priority so historical work does not create a growing tip gap. Publish completed intervals with honest coverage while older chunks run.

Backfill defaults to **local candidate generation followed by targeted block verification**. Forward capture evaluates each newly committed block; historical verification fetches only candidate heights after the local pilot gate. A full body scan is the fallback described in §5. Read canonical ids/header anchors from short core snapshots and reconcile with the configured primary's canonical selection. Fetch raw full block bodies by those ids, parse spending proofs/extensions and transaction outputs, then resolve input boxes from retained core content. For missing or unverified box serialization, fetch the creating confirmed block (or verified genesis content), reconstruct the full box and verify its id. A present spent box does not require a live UTXO endpoint. Source fallback may supply a requested primary-selected block, never select another fork. A pruned/missing block is a recorded gap with retries; no resync of core is requested.

Verify header/body linkage and applicable transaction/proof commitments using supported serialization. Transaction ids alone may not bind proof bytes. Retain raw evidence digest, source identity, block id, input bytes, proof/extension bytes, selected outputs and effective rule provenance for candidate transactions and linked children. Evidence unavailable for cryptographic binding must say `source_assurance: trusted_node_response`; it cannot claim independently authenticated proof bytes. Publish certified classification only at the declared, tested assurance level; unsupported binding/decoding is surfaced explicitly. The explorer trusts the configured validating node's canonical chain; it does not become another consensus engine.

Every covered block, including zero-candidate blocks, gets a durable receipt with an explicit basis: `body_verified`, `excluded_by_local_age_scan`, or `excluded_by_age`. A completed, trustworthy local scan can certify zero candidates at heights absent from its candidate set without fetching their bodies; pending candidate heights remain uncovered. Local exclusion receipts carry the scan generation, target anchor and completeness provenance, and never claim body inspection. A fetched block with unresolved candidates can be scan-complete but not classification-complete. A failed fetch is not scan-complete. Persist these distinctions atomically with facts. Recheck the core anchor before publication and at request time because two files cannot commit atomically.

## 5. Forward capture, two-phase backfill and operating budget

Accept the two-phase approach for a complete, trusted core index. `BoxRow` retains declared `creation_height` and `spent: Option<(Hash32, u32)>` (`crates/xp-store/src/rows.rs:300`, `:306`). The decoder reads JSON `creationHeight` directly (`crates/xp-wire/src/lib.rs:138`), and box insertion preserves it (`crates/xp-store/src/apply.rs:481`). Spending writes the transaction id and inclusion height back into `BOXES` (`crates/xp-store/src/apply.rs:231`); it removes only unspent/maturity index entries, not the box row. Retention pruning removes `UNDO` records (`crates/xp-store/src/apply.rs:155`), while rollback deletes boxes created on the reverted fork and clears reverted spends (`crates/xp-store/src/rollback.rs:190`, `:229`). Thus local age selection uses the same declared height as the census's GraphQL `inputs{box{creationHeight}}`; no block body is needed for that predicate.

### 5.1 Phase 1 — local candidate generation and pilot gate

Make one complete pass over `BOXES`, including retained spent rows. For each spend within the fixed target interval through anchor A, evaluate `spend_height >= 1_051_200 && creation_height <= spend_height - 1_051_200`, avoiding unsigned underflow. Emit `(spending_tx_id, spend_height)` and retain qualifying box ids for audit; deduplicate transactions and then heights. This is the complete candidate superset under the pinned age rule: nothing outside it can be a rent claim. Ordinary old-box spends remain candidates until phase 2.

Cost is **O(number of local BOXES rows)** in local reads/decoding, plus candidate deduplication and reporting writes, with **zero network requests**. It scans the box table, not necessarily all 89 GB of the core file. Elapsed time and bytes read are unmeasured; this is the comparatively cheap first pilot, not a promised instant operation. Use bounded key-range batches through the existing Store Reader, checkpoint into the reporting generation and release each snapshot promptly. Fix the target height/id before starting; ignore later spends and outputs. Appends preserve candidates through that anchor, but a reorg invalidating it invalidates the pass and its exclusion receipts; restart against the new anchor. Never infer absent candidates from an unfinished pass.

Also read local canonical headers/timestamps to anchor coverage and daily buckets. On completion report rows/bytes scanned, local elapsed time, candidate input/transaction counts and **D, the distinct candidate-height count**. D is **UNKNOWN until phase 1 runs**. Claims are episodic, so targeted fetching may save a large factor, but the ten recent zero-claim blocks neither measure D nor exclude ordinary mature spends elsewhere. Do not invent or extrapolate D from claim density. The completed local scan is the natural pilot gate before committing resources to phase 2: measure D, check index completeness/anchors and derive a budget from targeted verification samples.

### 5.2 Phase 2 — targeted verification and forward priority

Fetch full bodies for **only the D distinct candidate heights** in the historical target. Verify empty proofs, pointer shape/type and recreated/fully-consumed branches using §2; resolve all inputs needed for accounting and same-block family closure. Fetch creating blocks or historical parameter evidence additionally only where local/cached evidence is insufficient. Do not fetch noncandidate spending heights merely to establish zero claims. Forward capture continues on newly committed blocks with priority over historical work, using the same classifier and reporting writer.

If r is measured sustained candidate-block verification throughput (including input resolution, accounting and reporting writes), phase 2 time is approximately **D / r seconds**, plus pauses/retries not represented in r. At illustrative rates of 2, 10 or 25 candidate blocks/s, this is D/2, D/10 or D/25 seconds; these are formulas, not measured rates or an ETA. Base body transfer is **D × mean fetched body size**, plus missing-input/parameter fetches and retries. Record these extra requests separately and include their observed cost in the pilot rate; never equate node latency with end-to-end throughput.

Before the main targeted run, verify candidate heights in the census window, a sparse recent interval and an older interval, and fetch the recent negative-control sample even if phase 1 excludes it. Record blocks/inputs/bytes per second, cache state, body/creator fetch counts, unresolved counts, reporting allocation, API latency and ingest lag. ETA uses remaining candidate heights and measured sustained rate. Resume from atomic checkpoints; pause under operator-defined API/ingest/disk thresholds and expose the reason. Sparse hits never justify stopping the local pass or dropping candidate heights.

### 5.3 Full-scan fallback and storage budget

Fall back to fetching every rent-era body for any interval whose local index is partial, missing retained rows, corrupt or otherwise distrusted: local absence then cannot exclude candidates. Partial mode explicitly skips missing inputs (`crates/xp-store/src/apply.rs:214–226`), so it cannot establish the complete superset. Reconstruct missing input evidence from creating blocks or verified genesis content; unavailable evidence remains a coverage gap. This rebuilds reporting only, with no core resync. If the pinned historical age rule cannot justify excluding earlier heights, expand the fallback to the full chain and leave unsupported classification unresolved.

At an illustrative tip of 1,870,000, a full rent-era fallback covers **818,801 blocks** starting at 1,051,200; a full-chain fallback covers 1.87 million. At 2/10/25 blocks/s, those fallback scans take respectively 4.74 days/22.7 hours/9.1 hours or 10.82 days/2.16 days/20.8 hours, before unrepresented pauses/retries. At illustrative 20–100 KiB bodies, transfer is 15.6–78.1 GiB or 35.7–178.3 GiB. These are **fallback arithmetic scenarios, not the default backfill budget**. The earlier default “1–5 days, potentially over ten days” framing overstated required body work; the normal budget is the local scan plus D-dependent verification, whose saving is not yet measured.

Size the reporting file from phase 1 candidate counts and sampled evidence sizes, including ordinary/unresolved candidates, input multiplicity, family records, indexes and redb overhead. Claim count alone cannot size it. At an illustrative 160–240 bytes per full-chain header/time receipt, raw header coverage adds about 285–428 MiB. Actual allocation remains unmeasured; staging a replacement doubles the relevant reporting-file space, not the 89 GB core store. No implementation-time estimate is hidden inside these operating costs.

## 6. Separate storage, rebuild and rollback

Use `rent-reporting.redb` with its own `REPORTING_FORMAT_VERSION`, `classifier_version`, `accounting_version`, parameter-manifest digest, network/genesis identity and generation id. This refines the September 10 separate reporting-file proposal. No write to core `tables::ALL`, `SCHEMA_VERSION`, `UndoRow` or metadata is needed. Unsupported reporting format disables only reporting routes and emits status; opening core remains successful.

| Reporting records | Purpose |
|---|---|
| Generation manifest and interval coverage | Versions, chain identity, interval boundary ids, scan/classification/amount/family coverage, unresolved counts |
| Block receipts keyed by height | Id/parent, header timestamp, zero or nonzero contribution, per-block candidate/claim counts and accounting status |
| Candidate/input evidence keyed by height, transaction position, input index | Raw/normalized evidence, outcome/reason, recreation pointer and rule provenance |
| Claim transactions keyed by height, position | Deduplicated claims, census/owner deltas, exact/unknown amounts, parent fee, payout observations |
| Family records and membership edges | Same-block closure, unique fee outputs, funding/payout boundaries, attribution limitations |
| Timestamp index and block contributions | UTC daily reads without assuming timestamp monotonicity; exact totals plus unknown counts |

Store immutable per-block contributions rather than mutable global daily totals. One reporting writer owns the file in the explorer process; all core access uses the existing Store's Reader, never a second process/handle on live core. Use short snapshots, initially at most 100 blocks/50 ms of extraction and 16 MiB owned batches, with two node fetches in flight. Release core snapshots before network I/O. Bound evidence reads and API work separately.

Rebuild into `rent-reporting.<generation>.redb` while the old compatible generation remains readable. Check manifests, census reconciliation, logical fingerprints and coverage before atomically switching the active generation in the owning process. Keep the old file for an operator rollback with disk headroom for both. A classifier/accounting change makes a new generation; it cannot silently reinterpret existing cursor results. Cached evidence may avoid node fetches if sufficient; otherwise repeat local candidate generation and targeted verification, using the full-scan fallback only where required. Core needs neither deletion nor reindexing. Downgrading the binary can reopen a compatible retained generation or run without reporting.

On reorg, compare reporting receipts with core headers, find the common ancestor, and remove reverted block facts, candidates, claims, families and time keys in one reporting transaction, restoring interval metadata exactly. Family records are confined to their block, avoiding hidden mutations to older claims. A reorg crossing a chunk's start invalidates that chunk until its boundary is rebuilt. If reconciliation is too large, hide the generation and rebuild reporting only. Do not override core's own rollback/halt policy.

Test full logical identity including every reporting table and manifest byte before apply and after rollback; physical redb allocation equality is not promised. Crash between core commit and reporting commit is repaired by replay; crash after reporting commit is safe because request-time anchor checks reject stale forks. Deterministic keys make replay idempotent. No coverage advance without its matching rows, including zero-hit receipts.

## 7. Additive routes and completeness

Existing eligible/upcoming routes and all existing fields retain their contract. Add recent, daily, candidate audit and reporting status; summary/backlog aggregation is outside this design. NanoERG values are decimal integer strings; ambiguous values are null, never zero. Header timestamps are Unix milliseconds; dates are UTC. All examples below describe fields, not measured values.

### 7.1 Recent claims

`GET /v1/rent/recent?from_height=1846000&to_height=1864008&limit=50`

Inclusive heights; defaults are the last 720 blocks through the core snapshot tip, explicitly returned as `requested_range`. Do not move the default to whatever interval happens to be backfilled. Descending `(height, transaction_position)` order, one item per verified claim transaction; input evidence can be expanded through bounded detail/audit reads. Return `{items, next_cursor, as_of, reporting, coverage}`. Each claim includes:

- Transaction/block ids, height/time, verified input count and shapes, unresolved-input count and evidence references.
- `rent_nano`, `amount_status`, census/owner deltas, parent fee, mode and its basis.
- Family id/status, exact family fee when available, nullable allocated fee/net, observed payout P2PKs and attribution status.

Coverage names the requested range, covered intervals with boundary ids, missing intervals, generation and all rule/accounting versions. Return separate `scan_complete`, `classification_complete`, `amounts_complete` and `families_complete`, plus unresolved candidate/amount/family counts. `complete` is true only when scan and classification cover the entire requested range. It never means that all pages have already been returned or all identities are known. Amount/family booleans remain separate qualifications.

Default requests require complete scan coverage: a missing block range returns 503 `rent_history_not_ready` with coverage, never a successful empty answer. `allow_partial=true` opts into covered contributions with `complete: false`. Scan-complete but unresolved classification returns available verified claims with `classification_complete: false` and audit counts; `require_certified=true` instead returns 503 `rent_classification_incomplete`. Wholly unavailable reporting returns 503 even with partial mode. An empty complete range means no verified claims and no unresolved candidates; an incomplete empty range means nothing about claim absence.

Default limit 50, maximum 500 with existing house parsing. Opaque versioned cursors bind route, filters, range, direction, anchor height/id, generation, interval-manifest revision and exclusive last key. A backfill filling holes below the anchor changes the manifest revision and invalidates old cursors; appending above an unchanged anchor does not. Malformed cursors are 400; changed fork/generation/coverage is 409 `snapshot_changed`. `next_cursor: null` means pagination exhausted within the stated coverage, never that missing history was scanned.

`GET /v1/rent/candidates` uses the same range/coverage contract with an outcome filter, including unresolved-only audit pages. It exposes reason codes and exact deltas, not guessed rent. Evidence detail is bounded; excessive expansion gets a typed 422 rather than omitted inputs. This companion ensures unresolved transactions absent from the verified recent list are inspectable.

### 7.2 Daily history

`GET /v1/rent/daily?from=2026-09-01&to=2026-09-02&limit=50`

Use `[from 00:00, to 00:00)` by header time, ascending UTC days; max 366 requested days and 31 days/page. Each day includes candidate/verified-transaction/input counts, recreated/fully-consumed counts, known gross rent sum, unknown amount count, unresolved candidates, deduplicated known family fees, unknown family count and the same completeness booleans. Never sum a family fee once per member claim. Null total `rent_nano` when amounts or classifications are incomplete; `known_rent_nano` remains an explicitly qualified subtotal. Fees also carry membership/coverage qualifications and are not called collector profit.

The full header timestamp index establishes which heights through `as_of` belong to each day; do not infer completeness from interval endpoint timestamps or assume monotonic time. A day is scan-complete only when every such height has a body-verification receipt or a justified local/global age exclusion, and the timestamp index itself is complete through the anchor. Missing prefix/suffix coverage can make an apparently old day incomplete. Until that index is ready, return partial buckets only by opt-in. Zero-fill only demonstrably covered zero days; otherwise null totals and missing coverage, or the default 503 policy. The current UTC day has `day_closed: false`; historical bucket completeness is always as of the anchor and may change with later backdated timestamps or reorgs.

Use timestamp-indexed block contributions, with at most 50,000 contributions per page and a cooperative four-second deadline; 422 requests a narrower range when necessary. No node calls from handlers. All new routes use existing bounded blocking admission, rate limits, 2 MiB response ceiling and RFC 7807 errors; blocking permits survive HTTP cancellation until the work exits.

### 7.3 Status and snapshot validity

Add `/v1/status.reporting.rent` with availability, active/staging generations, covered/missing intervals, forward lag, backfill target/progress, unresolved counts, measured rate/ETA basis and sanitized last error/pause reason. Status is operational metadata, not monetary evidence.

Within each request, open bounded core and reporting snapshots and validate each selected interval's anchor against core. Reject mismatched publication with 503 `reporting_reconciling`; never combine a new core fork with old report totals. Recheck cursors at every page. An unavailable reporting file affects these routes only.

## 8. Acceptance evidence and limits

Before publishing certified results, retain real confirmed fixtures for recreated and fully-consumed branches, miner fee-free claims, parent-only bot fees and same-block parent/child fees, with raw block ids/bodies, creating boxes, node/version provenance and independently recorded expected amounts. Use **1846000..1864008 inclusive** as the reconciliation window: require exact equality of the explorer candidate txid set and the independent census's **5,116-transaction txid set**, plus owner-tree totals. Compare missing and extra txids explicitly; matching counts alone cannot pass. Also compare the verified-claim set against that export and enumerate every ordinary/unresolved refinement with chain evidence; any difference must be resolved or explicitly reported as unreconciled, never hidden by force-fitting the count. Obtain the independent export during implementation; its absence limits reconciliation evidence, not the design's acceptance of the supplied finding.

**NEGATIVE CONTROL — required for every classifier build:** replay an independently identified sample of recent blocks, including the measured ten-block interval once its block ids are supplied, and require **zero rent claims**. Every block contains at least two empty-proof spends: the emission transaction and the fee-collection transaction (standard fee contract prefix `1005040004000e36100204a00b08cd0279...`; match the full pinned tree). Their inputs are only 1–2 blocks old and must fail the age rule. Assert both controls are present and individually excluded; a build that tags either as rent fails acceptance even if its claim totals look productive. Run the full classifier on these fetched blocks, not just the phase 1 filter, and retain block/txids and expected exclusions independently of explorer output.

Use the authoritative validator at the relevant historical parameters to check boundary/fallback behavior. Current node acceptance alone establishes confirmed validity, not which branch ran. Real fixtures and node/reference execution must establish that branch. Expected rent/fees come from independently audited chain input/output sums and fee-output identities; no explorer helper or explorer-generated rent fixture can be its own oracle. Synthetic mutations may test parsing, limits and rollback, but cannot certify consensus behavior.

Cover maturity minus one/exact/plus one; signed old spends; the already verified young emission/fee-collection controls; missing/empty proofs; Short/Int/missing/out-of-range selectors; insufficient/equal/sufficient value; shared pointers; voluntary returns/top-ups; register/token changes; mixed funding; arithmetic overflow and historical parameter transitions. Family tests cover same-block ordering, generic absorber shared by distinct payout keys, several parents, externally funded children, fee-collection exclusion, later-block spends and bounded unresolved traversal. Test partial coverage, zero-hit chunks, timestamp disorder, page restarts after backfill, stale forks, reopen, crash between file commits and independent reporting downgrade/rebuild. Subsequent implementation runs the repository's applicable Rust/API gates; no code tests are claimed for this design-only change.

This design will not claim submitter identity, rival identity from templates, mempool success rates, guaranteed CPFP intent, exact per-parent fee allocation in mixed families, nominal rent as actual withdrawal, exact totals across gaps, or full historical consensus support without pinned evidence. It will surface those limits without treating reconstructible chain evidence as impossible.

The three least certain parts are **historical rule/parameter and pointer-type coverage**, **how often real mixed CPFP flows permit unique fee/payout attribution**, and **distinct candidate-height count, sustained local/targeted verification throughput and resulting reporting size**. The census reconciliation, real family fixtures and measured pilot respectively resolve them. None requires a core schema change or a resync.
