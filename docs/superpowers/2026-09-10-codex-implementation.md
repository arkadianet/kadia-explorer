# Implementation report — 10 September 2026

Items 1–4's requested fixes are implemented in the working tree. **No commits could be created.** After item 1 passed every required gate, `git add` failed:

```
fatal: Unable to create '.../ergo-explorer/.git/index.lock': Read-only file system
```

The environment mounts `.git` read-only and does not permit approval escalation. I did not bypass that restriction. Consequently the one-commit-per-item delivery requirement remains blocked, despite continuing implementation and verification. The original review was already untracked and was left untouched.

## Changes and regression evidence

| Item | Changes | Tests pinning the bugs | Commit |
|---|---|---|---|
| 1. Search ambiguity | Add `matches: [{kind,id},…]` for every matching entity, preserving legacy `kind`/`id` and lookup precedence. Ambiguous searches render Token and Mint input box navigation choices; singleton/legacy responses still redirect. | `search_shared_mint_id_returns_box_and_token` in `crates/xp-api/tests/routes.rs` uses **app_two_tokens()**, verifies both detail routes, and checks legacy fields plus both matches. Observed failing before implementation and passing afterward. Browser test `a shared mint input ID offers both token and box destinations` checks links and box navigation. | Blocked: read-only `.git` |
| 2. Freshness | Ingest retains `source_observed_at_ms` (Unix milliseconds) and `source_error` independently of height. A failed best-height retry preserves the observation time and publishes the error; success refreshes time and clears error. Browser tracks last successful poll, coalesces overlapping polls, and retains visibility pausing. Badge, sidebar, homepage and status page use one health policy: failures immediately unavailable; successful observations expire after 15 seconds. Last-known values remain visible. Homepage hero explicitly labels its indexed height as a snapshot. | Both tests in `frontend/tests/unit/freshness.test.ts` failed with stale Live before implementation and pass afterward. `source_failure_preserves_observation_and_recovery_refreshes_it` exercises a caught-up source stub failing and recovering. Browser test `a failed browser poll removes Live, retains height and recovers` uses Playwright's clock. | Blocked: read-only `.git` |
| 3. Honest totals | Upcoming rent looks ahead one row and exposes `complete` and `indexed_height`; null cursor is explicitly not a completeness promise for this bounded endpoint. Homepage/rent views display incomplete answers as lower bounds. Remove the 800-block stop; retain a six-page safety bound and visibly label short/capped windows partial. Failed headline dependencies show Unavailable. Monetary sums continue using bigint. | `upcoming_rent_reports_truncation_and_exact_cap` failed before implementation and passes afterward. `upcoming_501_boxes_explicitly_marks_500_as_incomplete` covers the requested 501-box case. `homeLoad.test.ts` observed 1,000 one-minute blocks before the fix and 1,500 afterward, covering a full day. Browser tests `failed statistic dependencies show unavailable instead of zero` and `capped rent totals are visible lower bounds` verify presentation. | Blocked: read-only `.git` |
| 4. Parity gate | Wrapper requires explicit URLs; exit 0/pass, 1/fail, 2/inconclusive. Missing configuration cannot produce a green release run. Explicit per-field floors: 20 address comparisons and 5 token comparisons. JSON reports attempted/passed/failed/skipped counts, floors, sampled IDs, reference URL, commit, schema and tip evidence. Full-history token detail 404 is a mismatch. Missing arrays/IDs/cursors cannot become empty successful comparisons. Sort before seeded shuffle. Compare token box-ID sets, and separately check holder sum = supply = emission − burned using integer arithmetic. Match best-chain height/hash before and after; changed/unverifiable snapshots yield inconclusive. | `shuffle_is_independent_of_candidate_insertion_order` failed before implementation and passes afterward. `release_verdict_requires_coverage_and_rejects_missing_fields` covers no evidence, floors, missing-field inconclusive and mismatch failure. `box_id_sets_detect_substitution_missing_ids_and_duplicates` checks corrupted IDs. `holder_mutation_and_supply_arithmetic_are_detected_without_rounding` checks a one-unit mutation above JS's safe integer range. All are offline tests. | Blocked: read-only `.git` |

Observed pre-change failures above are distinguished from added regression tests that were run only after implementation. No reference parity result is claimed from the offline tests.

## Gates

All required gates passed at each item's implementation checkpoint (later checkpoints include earlier uncommitted changes):

| Checkpoint | Rust fmt / Clippy / workspace tests | Frontend unit / check / lint / build | Playwright |
|---|---|---|---|
| Item 1 | Pass | Pass; 123 unit tests | 55 passed, 4 workers |
| Item 2 | Pass | Pass; 125 unit tests | 56 passed, 4 workers |
| Item 3 | Pass | Pass; 126 unit tests | 58 passed, 4 workers |
| Item 4 | Pass | Pass; 126 unit tests | 58 passed, 4 workers |

Commands:

```
cargo fmt --all -- --check
CARGO_TARGET_DIR=target cargo clippy --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR=target cargo test --workspace
# frontend/
npm test
npm run check
npm run lint
npm run build
npx playwright test --workers=4
```

The configured Cargo target under `~/.cache` was read-only. Builds used normal repository `target/` output instead; nothing under `~/.cache` was modified or deleted. Builds included the existing bundle-budget check. `git diff --check` passed.

The first item-3 browser run found two existing rent assertions whose mock omitted `complete`. The mock now calculates completeness from its dataset/limit; the complete suite passed afterward. A final full Rust run also covers the final stricter parity parsing.

`bash -n scripts/parity.sh` passed. Invoking the wrapper with both URL environment variables removed returned the expected JSON inconclusive result and exit 2 **before starting Cargo or making any requests**. The live parity test remained ignored in workspace tests, as did the existing explicitly opt-in live-node smoke tests.

Local logs for the final checkpoints are under `/tmp/ergo-item{2,3,4}-{rust,build,e2e}.log` (item 1 Rust: `/tmp/ergo-item1-rust.log`; final Rust: `/tmp/ergo-final-rust.log`).

## Scope, skipped work and verification limits

- No SSH, production requests, rent-collector VPS access, store copying/deletion, or process restarts/reconfiguration occurred. No request to the permitted at-tip instance was necessary: the supplied live shared-ID observation and performance measurements were accepted as facts. No performance remeasurement occurred.
- New behavior was verified with isolated fixture stores and deterministic browser mocks, not the locked at-tip store. Reference parity was **not run**. Its HTTP orchestration, reference compatibility and real-store coverage remain unverified.
- Rent work is containment: capped responses are explicitly incomplete, not cursor-paginated. Exact summary aggregation, stable-window rent pagination and a new home-summary endpoint remain deferred as described in the review's containment-first approach.
- The 24-hour walk remains bounded at 3,000 blocks. If that or available history is insufficient, the visible window is partial. Homepage blocks remain a labeled snapshot rather than being reloaded on every tip change.
- The broader review's template/register cross-query parity expansion and real-store browser smoke suite were not added. Current template/register API and browser suites still run; they are not independent reference parity. Holder arithmetic is explicitly internal consistency, not independent reference evidence.
- Rank 5 was skipped. The supplied measurements show no saturation, so the expansion optimization does not justify displacing the correctness work or the blocked commit delivery. No new telemetry, performance sweep or engine work was introduced.

## Intended commit messages once Git is writable

One commit per numbered item is still required. Suggested subjects:

1. `Let token ID searches offer both the token and mint input box`
2. `Stop showing Live after source or browser observations fail`
3. `Label capped totals and show unavailable statistics honestly`
4. `Prevent incomplete parity evidence from passing the release gate`

Each must end with the requested trailers:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01Jegbu3j98VqxmXFrcFCFdS
```
