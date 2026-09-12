# Explorer completion — Implementation Plan

Date: 2026-09-12. Status: proposed; checkboxes describe future work, not work executed in this planning round.

## 1. What DONE means — and what is cut

**Goal:** Finish the shipped confirmed-mainnet explorer as a trustworthy, bounded and recoverable service, with automatic gates and reviewable release evidence. Completion means M1–M6 have met their acceptance criteria; adding every route in the September 5 draft would delay that outcome without establishing trust.

**Scope decision:** Cut `/v1/miners`, `/v1/stats/24h`, `/v1/stats/daily`, `/v1/rent/summary`, `/v1/rent/recent` and `/v1/rent/daily` from this release. Also cut their reporting file/worker/backfill, DuckDB, general analytics, new supply dashboards, historical rankings, timestamp history, token-name discovery, testnet and protocol expansion. Keep mempool/WebSocket deferred. Remove only the homepage presentation badges that require transaction expansion. Stop legacy canonicality guessing. Do not promise a 30–40 GB file or restoration of pruned UNDO. These cuts and their individual evidence/reasoning are in the companion design §1.1; none becomes a final “optional” implementation task here.

**Why this is defensible:** The core v2 data already supports useful explorer/history reads; it cannot prove rent execution from stored input ids. The shipped full transaction lists still have a concrete expansion problem. The store has no demonstrated recovery path and every gate has been manual. Protect and substantiate the existing product before introducing another persisted data pipeline.

**Spec:** `docs/superpowers/specs/2026-09-12-explorer-done-design.md` (normative contracts, acceptance budgets, exceptions and cuts).

**Baseline:** `fix/explorer-exit-code-and-rent-truth` at `8a0e3dc`. The four recent defect fixes are closed and excluded from implementation scope. Older plans are structural examples, not authorization to execute their resync, dependency-installation or deployment instructions.

**Architecture:** Existing Rust workspace, core schema v2/redb, bounded in-process Readers, axum `/v1`, static SvelteKit frontend. Add CI, read budgets, summary projections, strict continuation metadata and operator evidence. No second reporting database and no feature-driven core resync.

**Tech stack:** Existing dependencies only. Use workspace `proptest` for generated histories, `tempfile` for isolated stores, axum/tracing/atomics for bounded instrumentation, existing Vitest/Playwright and shell/Python tooling. No new external crate or npm dependency is needed. Any later proposal must name the package, missing capability and why existing facilities are insufficient before it can enter scope.

## Global constraints

- No field renames/removals on existing routes. Preserve bare block transaction arrays; use new routes for summary pages. Additive error codes and explicit resource failures may replace unsafe success, but never emit partial successful detail bodies.
- Integer decimal strings for nanoERG/raw token units; lowercase hex ids; base58 addresses. Frontend arithmetic stays `BigInt`.
- No core schema change requiring resync. If implementation discovers one is required, stop that approach and redesign. No migration loophole through “rebuild the production index once”.
- Production redb is owned by one process. Live observations use its HTTP/in-process instrumentation. Offline work uses stopped consistent copies, each with one owner. No uncoordinated live copying or second-process opens.
- Rent algorithms are not reimplemented. Any consensus-adjacent change discovered during verification requires independent, pinned evidence and parameter provenance, not a self-oracle.
- A check has passed only with a revision-stamped artifact and exit status. No missing infrastructure, skipped parity or unrun live test can be marked passing.
- The delivery sequence is M1 → M2 → M3 → M4 → M5 → M6. Book operational dependencies in M1; start growth collection after M3. No implementation or commit is authorized by this planning document itself.

### Common gates (G)

At repository root:

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

Use these exact commands in CI and the local entry point; orchestration may run independent Rust/frontend jobs concurrently, but the overall result requires every command to pass. Install from the committed lockfiles; record tool versions and artifact hashes. The frontend `build` already invokes its bundle budget. The Playwright CLI worker setting must override the existing CI config default of one.

Focused tests below precede G for their task. Run G once on the resulting milestone candidate; repeat if code changes or a check fails, not as an empty ritual. Operational gates supplement G and remain separate from deterministic PR tests.

## File structure and ownership

Proposed new paths are intentional; existing files are extended where named. Confirm exact function boundaries at implementation time.

| Area | Files |
|---|---|
| Gate orchestration/status | New `.github/workflows/ci.yml`, `scripts/check.sh`, `WORK-REPORT.md`; `rust-toolchain.toml`, `bin/explorer/README.md`, `CHANGELOG.md` |
| Canonical source/integrity | `crates/xp-source/src/rust_node.rs`, `tests/rust_node.rs`; `crates/xp-store/src/{read,read_tokens,apply,rollback}.rs`; `crates/xp-api/src/{dto,error}.rs` |
| State-model verification | New `crates/xp-store/tests/state_machine.rs`; existing `tests/{rollback,apply,genesis}.rs`; test-only logical snapshot helper in store test support |
| Bounded projections/telemetry | `crates/xp-api/src/{lib,dto,error}.rs`, `handlers/{blocks,txs}.rs`; new API budget/metrics modules; `crates/xp-store/src/read.rs`; `bin/explorer/src/{main,config}.rs` |
| Storage capacity/recovery | `crates/xp-store/src/{lib,extras,apply}.rs`, `bin/explorer/src/{config,main}.rs`; new operator measurement/recovery scripts and `docs/operations/explorer-recovery.md` |
| Continuation/frontend | Paged API handlers/DTO parsers; `frontend/src/lib/api/{types,endpoints}.ts`, `lib/pager/pager.svelte.ts`, `routes/+page.*`, `routes/txs/+page.*`, `routes/blocks/[id]/+page.*` and affected list pages |
| Release evidence | `scripts/parity.sh`, `crates/xp-api/tests/parity.rs` (reuse existing gate); real-API Playwright configuration/tests alongside `frontend/tests/live/`; `WORK-REPORT.md` links to immutable artifacts |

Only the two new planning documents are written in this round. The other paths above are proposed future implementation outputs.

---

### M1: Make gates unavoidable and status auditable

**Delivers:** Automatic Rust/frontend checks, reproducible local runner, required-check setup and a real delivery-status ledger. **Size:** 1–2 days.

**Why first:** It addresses the structural failure that let defects escape and protects every following milestone. A feature wave before this would repeat the same release process.

**Does not do:** Deploy, add public-node dependencies to PR tests, claim prior gates passed, or upgrade packages unrelated to running the existing gates.

**Files:** Gate/status row above; inspect existing `frontend/playwright.config.ts`, package lockfile and toolchain before choosing runner images.

**Interfaces:** One exit-nonzero local command running G; named CI checks with retained logs; `WORK-REPORT.md` entries with scope, revision, state (`planned`, `implemented`, `verified`, `released`), exact commands, results, artifact links and blockers. Keep the old history design's reference to that authority resolvable without changing its historical text.

- [ ] **Step 1: Establish the actual gate baseline.** Run G locally with writable target/cache paths if needed; record versions and failures. Do not treat sandbox/toolchain provisioning failures as code regressions or passes. Pin the Rust version that passes (current file says `stable`), and a Node version compatible with the existing mock server's type stripping. Use `npm ci`; no dependency version churn.
- [ ] **Step 2: Add `scripts/check.sh` and PR/push workflow.** Separate Rust and frontend jobs, locked dependency installation, required Chromium installation and `--workers=4`. Pin workflow actions to reviewed revisions; justify checkout/toolchain/setup/artifact actions as CI orchestration. Preserve logs and Playwright failure artifacts for at least 14 days. Set job timeouts so hung fixtures cannot produce indefinite checks.
- [ ] **Step 3: Create the delivery ledger.** Record baseline fixes as already implemented with commit provenance, not as new work. Record known unrun operational gates honestly. Link all M1–M6 acceptance artifacts and record the CUT decisions. Do not copy stale “awaiting tests” statements forward as current evidence.
- [ ] **Step 4: Prove enforcement.** On a disposable branch, introduce an intentionally failing check, see it turn CI red, then remove it. Configure branch rules/required statuses for the target integration branch and retain read-only configuration evidence. If account permissions prevent configuration, report that exact blocker; do not equate workflow creation with enforcement.
- [ ] **Step 5: Provision later evidence inputs.** Identify a supported primary/reference source, deployment-class test host, independent backup destination, CI administration access and external telemetry retention owner. Record availability and scheduling, not invented measurements.

**Acceptance/verification:** G green on candidate SHA, deliberate red-check artifact, required-check evidence, no production secret required by PR jobs, and a ledger whose unverified items are visibly unverified. Until CI is active, run the exact same local commands. This milestone is incomplete if the workflow never executes.

---

### M2: Stop unverified answers and test state transitions

**Delivers:** Fail-closed canonical selection, full-store required-row errors and generated apply/rollback identity including retention-aware UNDO checks. **Size:** 4–6 days.

**Depends on:** M1. **Why now:** Wrong but plausible data is more damaging than slow data; this also protects the later capacity rejection path and recovery drill.

**Does not do:** Add legacy node adapters, change consensus rules, repair production rows, rebuild source-health reporting, or change core row encoding.

**Files:** Source/integrity and state-model rows above; extend `crates/xp-api/tests/routes.rs` and temporary store fixtures.

**Interfaces:** Unsupported canonical lookup is a source capability error, never first-id selection; corruption maps to 500 `integrity_error`; optional absent data versus required references has an explicit matrix; test-only snapshots cover all table bytes.

- [ ] **Step 1: Add discriminating source tests.** Stub missing `chainSlice` with a single id and with orphan-first competing ids in `/blocks/at`; neither may yield an ingestible canonical id. Cover transient 503, malformed/contradictory canonical entries and height mismatch. Preserve primary-id body fallback tests. Verify body id/height/parent agreement in existing ingest checks and close any tested gap without adding a consensus verifier.
- [ ] **Step 2: Remove the legacy selection downgrade.** Use the existing source failure propagation. Probe production primary capability over HTTP before rollout; a missing capability blocks rollout until a supported primary is configured.
- [ ] **Step 3: Write the required-reference matrix before changing helpers.** Inventory all ordinary Readers, DTO expansion, metadata enrichment and apply/rollback paths that use `None`, `continue` or defaults. Classify: unknown requested entity → 404; required indexed row → corruption; absent EIP-4 name on an existing token → optional; pre-seed input/mint on declared partial store → incomplete. Record why partial-mode absence is allowed and where it cannot be distinguished. No blanket removal of optional metadata.
- [ ] **Step 4: Add missing-row fixtures and implement fail-closed reads.** Remove each required reference in an isolated temporary store with no other owner. Assert affected full-store list/detail requests fail, never omit one item or silently zero a total. Test partial-mode permitted gaps and in-range dangling indexes separately. Keep existing fields/nullability; add completeness metadata or headers where necessary. Confirm unrelated unknown-id requests still return 404.
- [ ] **Step 5: Build the independent transition model.** Use simple maps of fixture UTXOs, token quantities and expected secondary membership, not the production apply functions. Generate at least 256 seeded histories of at least 32 transitions, covering batched/single apply, same-block spend, mint/burn, rollback/reapply on a competing branch, reused gidx, close/reopen, genesis and partial starts. Save seed/minimized reproductions on failure.
- [ ] **Step 6: Test UNDO explicitly.** Preserve existing fingerprint assertions. Compare all logical tables including UNDO without pruning; across pruning, compare indexed bytes and the independently expected retained undo interval/bytes. Add deterministic `ROLLBACK_WINDOW-1`, `ROLLBACK_WINDOW`, `ROLLBACK_WINDOW+1` cases and attempt rollback at/just outside retained coverage. Deliberately damage an undo row to prove the new oracle detects it. No physical-file equality assertion.
- [ ] **Step 7: Run focused source/store/API tests, then G.** Retain seed, table-difference and test artifacts in the ledger.

**Acceptance/verification:** All source/integrity matrix cases pass; 256 histories and retention boundaries pass; intentional undo mutation is detected. Fixtures use only temporary stores. Production integrity remains unmeasured until M6; the tests establish behavior on corruption, not that the live file has none.

---

### M3: Bound public transaction work and retain operating evidence

**Delivers:** Paginated summaries, frontend use of them, budgets for existing full responses and durable saturation evidence. **Size:** 5–7 days.

**Depends on:** M2. **Why here:** Contains audit rank 5 without breaking successful response shapes; telemetry is necessary to set defensible capacity and release limits.

**Does not do:** Build any cut reporting route, retain classification badges at the cost of new indexing, add a metrics package, or increase concurrency as the primary fix.

**Files:** Bounded-projection/telemetry and frontend rows above; new focused budget tests; existing API cancellation/permit tests and frontend transaction/block/home tests.

**Interfaces:** `/v1/tx-summaries` and `/v1/blocks/{height_or_id}/tx-summaries`; existing `TxSummaryDto`; old `TxDto`/array unchanged; explicit 422 limits; bounded-cardinality operator telemetry, externally retained. M5 adds strict continuation to these pages.

- [ ] **Step 1: Instrument before/after measurements.** Add fixed route/status counters, duration buckets, bytes, timeout/overload, blocking-worker lifetime and ingest/capacity metrics per design §5. Route labels use router templates, never raw URLs. Test rejection/timeout/error accounting once each, and a process-start marker. Protect the endpoint at the operator network boundary; it does not scan the database.
- [ ] **Step 2: Connect durable collection.** Use the existing collector if available, otherwise an external standard-tool sampler with rotation, retention, bounded disk use and gap detection. Scrape at 15-second intervals, retain 14 days. Generate saturation in a fixture service, restart it, retrieve the earlier episode and distinguish the new counter epoch. Endpoint-only instrumentation is not completion.
- [ ] **Step 3: Add summary range reads/handlers.** Page the block's contiguous range with exclusive cursor and deterministic order; global summaries reuse gidx traversal. Validate counts with checked conversion instead of silently clamping newly exposed summaries. Compare every summary field with fixture rows; a test lookup counter must remain zero for box/tree/token enrichment. Preserve all existing routes.
- [ ] **Step 4: Bound legacy full expansion.** Shared work/deadline/serialization accounting must cover cumulative input/output, token and register work before large allocation. Initial budgets: 10,000 box resolutions, 2 MiB JSON, four-second cooperative deadline, checks at most every 128 work units. Preserve stricter existing history limits. Test at/one-over each threshold and one oversized transaction. Old block arrays succeed completely or return a problem; never truncate them. Ensure cancellation/timeout retains permits until actual worker completion.
- [ ] **Step 5: Move first-party lists to summaries.** Update API types/endpoints, global/block/home loaders and mock fixtures. Block pagination becomes “Load more”; full details load on navigation. Remove homepage kind badges derived from full boxes; preserve remaining transaction facts and detail behavior. Test empty blocks, multi-page blocks, errors and amounts above JS safe integers.
- [ ] **Step 6: Measure and run G.** Use deployment-class isolated service/clone, warm/cold-start runs and the design's nominal/mixed-load protocol. Record successes separately from fast overload rejection, response bytes, peak RSS, blocking duration and drain time. Do not present a mock server's latency as store performance. Begin seven-day growth/operating collection for M4/M6.

**Acceptance/verification:** New routes answer fixture and real-store requests; summary box-resolution count is zero; old success JSON compatible; budgets/cancellation tests pass; earlier saturation remains inspectable after restart. Under the specified nominal load warm summary/status p95 ≤100 ms, no unexpected 5xx, and mixed-load workers drain within five seconds. A slow hardware result is an explicit blocker/limit-review input, not a reason to blend 503s into success latency. No production database file access is needed for these requests.

---

### M4: Attribute storage, cap register growth and restore the full index

**Delivers:** Measured storage budget, atomic register cardinality ceiling and a proven stopped-copy backup/restore procedure. **Size:** 4–6 days plus growth observation already started.

**Depends on:** M2 integrity/model tests and M3 measurements. **Why before pagination polish:** The sole 89 GB history asset needs a demonstrated recovery path before more interface work.

**Does not do:** Resync production for a feature, auto-compact, prune existing register rows, skip new register entries, change database engines or assert an unexplained compression target.

**Files:** Storage-capacity/recovery row; capacity tests extend the M2 model and rollback suites.

**Interfaces:** In-process cheap table/storage statistics; configured total register-entry ceiling, 80% alert and distinct local capacity failure; versioned capture manifest and isolated recovery procedure. No schema bump expected.

- [ ] **Step 1: Inspect locked redb capabilities.** Read locally available redb API/source for table counts, allocation statistics and transaction behavior. Determine which stats are cheap and which require a scan. Avoid importing a second database library or guessing allocator semantics. Record logical/physical categories and units.
- [ ] **Step 2: Produce the footprint inventory.** Gather live cheap stats via the owning process. Use a stopped clone for expensive key/value accounting. Include every table, UNDO, metadata, allocator overhead/free pages where available, apparent bytes and filesystem allocated bytes. Reconcile within 5% or explain residual with evidence. Attribute register growth without assuming it caused the 89 GB footprint. Record seven daily growth intervals and 30-day headroom calculation.
- [ ] **Step 3: Implement the register circuit breaker.** Use total existing entries plus exact prospective insertions within the write transaction. If cheap table count is unavailable, implement the design's bounded initialization/correct rollback-aware count rather than rescanning per block. Configure an observed-capacity-derived ceiling; reaching it aborts the whole batch, exposes a local capacity reason and preserves the last good tip. Raising the ceiling resumes. Reject a deployment configuration already below current occupancy.
- [ ] **Step 4: Test atomic rejection.** Below/at/above ceiling; multi-block batch; repeated register values still create distinct gidx entries; genesis; rollback; process restart; count overflow. Compare all logical table and UNDO bytes before/after rejected apply. Assert capacity failure is neither an accepted block nor a source-health error. Verify an 80% alert in the external collector.
- [ ] **Step 5: Capture a full-size consistent backup.** Book the maintenance window and independent destination. Stop explorer, confirm process exit/closed ownership, copy and hash the store/config manifest, restart original. Record real downtime and anchor. No live `cp`, no second owner. Do not run a speculative full-store mutation to prepare the copy.
- [ ] **Step 6: Restore separately and verify.** Open the restored file with one isolated explorer process. Compare capture-anchor block/transaction/address/token/history facts against the manifest, run integrity samples, and measure catch-up to a recorded source target. Exercise fork/recovery mechanics on a disposable fixture or disposable restored clone without modifying the original. Record corrupt/truncated backup detection in small fixtures and checksum verification on the full copy.
- [ ] **Step 7: Publish the runbook and capacity result; run G.** Include restore commands, required free space, identities, service stop/start checks, checkpoint verification, failed-check recovery and rollback of the binary. Store evidence outside the original database disk. Set recurring backup cadence and verify backup-age monitoring.

**Acceptance/verification:** Design §5 headroom (30 days at maximum measured daily growth plus 20% current allocation), separately provisioned staging, attributable footprint, atomic cap tests, full-size restore. Proposed limits: capture downtime ≤30 minutes, restore-plus-catch-up ≤4 hours and backup age ≤24 hours. Record actual values. If unmet, provision storage/change procedure or revise the specification with measured evidence. A documented runbook without a completed drill is not acceptance. No resync is required or performed; isolated resync benchmarking is cut from this release's obligations.

---

### M5: Add strict continuation without rewriting legacy wire formats

**Delivers:** Snapshot/cursor pairs and frontend restart behavior covering every ordinary paged route. **Size:** 4–6 days.

**Depends on:** M3 bounded projections; follows M4 by risk priority. **Why here:** Cursor consistency needs the final list projections; implementing it before summaries would duplicate work. It must precede final parity/browser validation.

**Does not do:** Keep MVCC readers alive between requests, implement historical holders, change legacy cursor encoding, or guarantee that expanded boxes are immutable.

**Files:** All ordinary paged handlers, shared DTO/query parsing, frontend pager/endpoints/types, API route tests and frontend pager/e2e tests. Existing history handlers are a reference and regression target, not a rewrite.

**Interfaces:** Add `consistency`, observed anchor and `next_snapshot`; requests carry `consistency=strict`, `snapshot` and existing `cursor`. Follow design §4.2 for immutable-versus-current-state policy, 400/409 semantics and token binding.

- [ ] **Step 1: Commit a route-policy matrix as documentation/tests.** Enumerate blocks, global/block/address summaries, full transactions, address/token/template boxes, token listings (both sorts), holders, rich list, register search and rent lists. Mark current-state policy for any projection with mutable enrichment. Leave existing historical endpoints on their own contract; nonpaged capped samples remain explicitly samples.
- [ ] **Step 2: Add bounded token codec/validation.** Reuse existing serialization/hash utilities; choose an opaque encoding implementable with existing dependencies (hex-encoded structured bytes is sufficient; base64 is not required). Limit token size, reject unknown versions/fields, bind route/filter/order/anchor/exclusive cursor. No monetary sums or secret trust in cursor contents. Validate the token and anchor inside the same Reader used for the page.
- [ ] **Step 3: Apply the two policies.** Immutable summaries/blocks continue below original membership bound after a new block if anchor survives. Mutable projections reject every tip change, including ordinary advance. Old bare cursor requests remain best-effort and carry explicit metadata. Array routes are not converted to pages. Partial/uninitialized snapshots must not invent canonical anchors.
- [ ] **Step 4: Prove page identity or explicit invalidation.** Walk at least three pages per route family in both supported directions; test exact limit, last page, empty sparse page if applicable, changed filter/token pairing, restart, forked gidx reuse and reorg replacing anchor. Reorder holder/rich-list keys by applying a block; continuation must return 409, not a plausible mixed ranking. Append above immutable anchors must not duplicate/include new items.
- [ ] **Step 5: Upgrade the frontend pager.** Pass token/cursor pairs, reset accumulated rows on 409 and offer user-initiated restart with a concise chain-change message. Prevent automatic retry loops. Preserve filter resets and bounded DOM behavior. Test that data from before/after 409 is never concatenated. All first-party ordinary lists opt in; retain separate historical handling.
- [ ] **Step 6: Run compatibility tests and G.** Old clients using unchanged query/JSON fields must still work. Verify strict route additions preserve decimal strings and ids. Document the deliberate best-effort legacy exception prominently in the API README.

**Acceptance/verification:** Every listed family has a strict-policy test; multi-page identity or 409 is observable; legacy fields/cursors unchanged; frontend never silently joins snapshots. Test via temporary Stores/in-process routers and deterministic UI mocks, then real-API continuation in M6. No reporting schema or production-file access.

---

### M6: Establish final release evidence and close delivery

**Delivers:** A candidate whose code, wire behavior, independent data checks, resource budgets and recovery path have all been demonstrated. **Size:** 3–5 days plus seven-day soak.

**Depends on:** M1–M5 accepted. **Why last:** Earlier SHA results cannot certify the final changed API or deployment configuration.

**Does not do:** Expand scope to reporting, conceal an inconclusive result, or reopen already closed defects as feature work.

**Files:** Reuse parity wrapper/test and real-API browser suites; add only missing release harness checks and artifact manifest; update `WORK-REPORT.md`, README/change log and scope references as needed.

- [ ] **Step 1: Assemble the candidate manifest.** SHA, dirty-tree status, core schema, binary/frontend hashes, lock/tool versions, effective limits, source identity, database anchor and ledger. Run G through required CI on that SHA. Operational configuration and assets must match the manifest.
- [ ] **Step 2: Execute existing independent parity.** Invoke `scripts/parity.sh` with explicitly supplied explorer/reference URLs; preserve JSON, sampled ids, matched height/hash, provenance and outcome. Existing floors are 20 address balance/tx-count/unspent-id comparisons each and 5 token emission/name/internal-supply/unspent-id comparisons each. Require `pass` and zero mismatches. Reference/page caps remain explicit exclusions. Missing evidence or changing anchors is inconclusive, never overridden with a forced pass.
- [ ] **Step 3: Validate the parity evidence boundary.** Retain the existing negative verdict tests; exercise missing-field, all-skipped and mismatched-id failure cases through a controlled fixture reference. Do not rewrite the already implemented coverage mechanism. Independent reference comparisons and internal accounting checks remain separately labelled. If stable live reference access is unavailable, use independently captured reference data with pinned provenance and equivalent coverage; the absence of either blocks release.
- [ ] **Step 4: Retain consensus evidence and run real-API frontend smoke separately.** Archive the pinned authoritative source references and captured inputs/parameters supporting existing rent behavior; distinguish existing regression tests from independent evidence. If that provenance cannot be supplied, record an evidence blocker rather than rebuilding a classifier or declaring the rules verified.  Existing `tests/live` is outside the default `tests/e2e` directory; explicitly configure/run it against the built frontend and actual explorer API, using `--workers=4`. Never substitute the default mock runner and call it live. Cover a shared token/box id, summary global/block navigation, detail, historical balance/pages, strict continuation and changed-snapshot restart, current rent presentation, errors, mobile navigation and status. Use disposable services for destructive/error injection; production gets read-only probes.
- [ ] **Step 5: Close operating gates.** Retain the nominal/mixed-load artifacts at final code/config, seven daily growth intervals, peak RSS, status/summary p95, rejection counts and reader drain. During a seven-day deployment-class soak require no unexplained integrity failures, no budget violation, telemetry continuity across a deliberate restart, backup age within 24 hours and headroom above the formula. Attach M4's full-size restore/catch-up timings and checksums; repeat the drill only if subsequent changes affect the recovery mechanism or invalidate its manifest assumptions.
- [ ] **Step 6: Make rollout reviewable and close the ledger.** Prepare binary/frontend rollback steps, source-capability check, capacity preflight, backup verification and post-rollout read-only smoke commands. Use the environment's actual deployment authorization when implementation reaches rollout. Record `released` only after deployment and smoke evidence; until then say `verified candidate`. Update current product documentation so the six cut routes are not advertised as delivery debt. Leave older design documents as historical proposals with clear links to this superseding scope.

**Acceptance/verification:** G, independent parity, real-API smoke, seven-day operating budgets and full-size recovery evidence all pass; no unresolved integrity mismatch and no unverifiable “done” checkboxes. A blocker returns to its owning milestone, followed by affected checks and a new candidate manifest. This round itself runs none of those implementation gates.

## Risks, unknowns and stop conditions

The design §6 is authoritative. Implementation must keep the following visible in the ledger:

- Unsupported production source after cutting legacy selection → choose a supported primary before rollout; do not restore guessing.
- Existing full-store corruption exposed by M2 → block release and separately scope repair/recovery; no implicit resync authorization.
- No required-check access, external telemetry retention, independent backup storage or stable reference evidence → operational milestone remains incomplete. Continue independent code work, but do not mark DONE.
- Measured storage API cost or cap bookkeeping requiring incompatible schema → redesign; resync is vetoed.
- Register cap stopping routine indexing → capacity decision, not silent omission. The index is complete at its last committed tip and must be described that way.
- Mutable-list restart frequency or legitimate details exceeding limits → measured usability/limit review; do not weaken correctness or hide response fields.
- Recovery/latency targets miss on actual hardware → report measured numbers and explicitly revise resources or scope before release; no retroactive claim that proposed targets were observations.
- Rent evidence disagreement → independent pinned source/input/parameter review. Do not build the cut classifier or use explorer results to certify themselves.

The least confident scope decisions are the complete reporting cut, exact-tip invalidation for mutable lists, and the proposed capacity/recovery commitments. Review those decisions directly; the rest of the plan does not silently depend on reversing them.
