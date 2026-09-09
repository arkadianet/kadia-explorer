**Kadia Ergo explorer investigation — 10 September 2026**

Fix misleading answers before expanding features. The strongest findings are token-ID search resolving to a box, health remaining “Live” after failures, and incomplete homepage data being presented as totals. Keep the standalone redb architecture; there is not enough measured evidence here to justify an engine change or a broad frontend redesign.

The checkout is `daab98490f556d95ec05bca528a316fef0455998`, matching the brief. I read the core/frontend designs, the plans’ scope and constraints, relevant implementation/tests, the supplied changelog, deferred-minor ledger and final reviews. Implementation-plan deployment instructions were treated as historical intent, not authorization to deploy. The repository was not modified. No production requests, SSH, reference-node requests or VPS access occurred.

**Critical limitation:** live measurements could not run. The first request was `GET http://127.0.0.1:18091/v1/status`; the sandbox rejected socket creation with `Operation not permitted`. A second attempt through the attached local-only Python harness failed identically and stopped. This establishes an execution restriction, **not** that the server is down. There are no measured server latencies, throughput figures, live cursor results or verified current chain height in this report. The performance ranking below is provisional. Offline results are explicitly distinguished from live observations.

| Rank | Recommendation | Evidence/confidence | Estimated build effort |
|---|---|---|---|
| 1 | Make token/box ambiguity explicit in search | Direct code proof; full-store consequence not checked over HTTP | 1–2 engineer-days |
| 2 | Make health and displayed data freshness truthful | Offline reproduction plus ingest code evidence | 2–3 days |
| 3 | Stop presenting capped, failed or partial data as totals | Offline reproduction and API/store code proof | 1 day containment; 3–5 days durable solution |
| 4 | Turn parity and full-store UI checks into meaningful release evidence | Known unrun gate plus concrete fail-open paths | 2–4 days, excluding reference-data provisioning |
| 5 | Extend transaction summaries and measure resource consumption | Definite unnecessary work; latency impact unmeasured | 2–4 days plus 1 day instrumentation |

These are planning estimates for an engineer familiar with this codebase, including focused tests and review; they are not measured implementation times. Ranks 1–3 address incorrect user-visible meaning. Rank 4 prevents recurrence. Rank 5 should be reprioritized if the local measurements show serious saturation.

**1. Search cannot reliably take a token ID to its token page.**

`crates/xp-store/src/tokens.rs:223` and `:232` implement minting from the first input box ID. That same ID therefore names both the token and a historical box. Spent boxes remain in `BOXES` (`crates/xp-store/src/apply.rs:226` updates `spent`; it does not remove the row). In `crates/xp-api/src/handlers/search.rs`, `search()` returns `kind: "box"` before checking `rd.token()`. On the brief’s full-history store, the box lookup wins for an ordinary indexed token ID. `frontend/src/routes/search/+page.ts` immediately redirects according to this single kind.

This is not a rare random hash collision. It follows from the mint rule. The regression test `search_resolves_a_token_id_and_a_template_hash` (`crates/xp-api/tests/routes.rs:1197`) misses it because `app_sigusd()` seeds at height 453050 and applies only 453051: the pre-existing mint input is absent. The same file already has `app_two_tokens()`, which mints from a box indexed in the previous fixture block and is a suitable discriminating fixture.

Add a compatible search result containing all matched entity kinds, and show “Token” and “Mint input box” choices when both exist. Preserve the legacy `kind/id` fields for clients, or introduce a separate search-results endpoint. Add direct token/box links between the corresponding detail pages. Simply reversing lookup priority trades away box search; explicit ambiguity handling preserves both tasks.

Success means a token ID with both rows present offers a token detail link and a historical box link; a box-only ID still resolves directly; existing block/transaction/template/address searches still work. Exercise the shared-ID fixture through both the API and Svelte navigation.

Local reproduction, once socket access is available (SigUSD ID comes from `routes.rs:757`):

```sh
curl --noproxy '*' -sS 'http://127.0.0.1:18091/v1/tokens/03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
curl --noproxy '*' -sS 'http://127.0.0.1:18091/v1/boxes/03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
curl --noproxy '*' -sS 'http://127.0.0.1:18091/v1/search?q=03faf2cb329f2e90d6d23b58d91bbb6c046aa143261cc21f52fbe2824bfcbf04'
```

Expected from the code: both entities exist, search returns `box`. This is a prediction, not a recorded HTTP result.

**2. “Live” currently means a healthy answer existed at some time.**

There are two independent stale-health paths:

- Browser → API failure: `frontend/src/lib/status/status.svelte.ts` keeps `current` on poll failure and only sets `error`. `health()` accepts no observation age or error. `StatusBadge.svelte` only changes its tooltip for an error when there is *no* previous `current`. The offline probe fed one successful healthy response followed by three failed polls: the error was present, the old height was retained, and the health label was still `Live`.
- Explorer → source failure: `crates/xp-ingest/src/lib.rs:319` handles a failed `best_height()` by calling `retry!`. That macro (`:217`) republishes the previous `best` without recording source failure, unless a separate existing body stall is already present. If the explorer was caught up before the failure, `/v1/status` can keep returning zero lag while source observation is no longer current. This path was inspected, not executed.

The homepage also mixes times. `frontend/src/routes/+page.svelte` derives the hero’s “Indexed height”, recent blocks and charts from the original load, while the sidebar status and transaction list refresh. Leaving the tab open can show two different indexed heights under identical labels. Its code explicitly says only transactions refetch.

Track last successful source observation and its error independently of chain height advancement; a quiet chain is not itself an outage. Expose that freshness in status. Track the browser’s last successful poll too, and show “Status unavailable — last checked …” when stale while retaining the last-known values. Use one shared freshness policy for badge, sidebar and status page. Refresh the small recent-block view when the indexed tip changes, or label snapshots with their height and observation time. Coalesce in-flight polls and retain the existing pause-when-hidden behavior.

A proposed browser freshness threshold is 15 seconds (three nominal 5-second polling intervals), to be validated rather than treated as an existing requirement. Acceptance: fake-clock tests for successful → failed → recovered polling; a source stub whose `best_height()` starts failing after catch-up; a tab spanning a new block shows consistent heights or explicit snapshot labels. No WebSocket is required for these fixes.

This is broader than the ledger’s initial-status-load warning: a successful response currently suppresses meaningful failure presentation indefinitely.

**3. Homepage totals need completeness and error semantics.**

Three concrete cases undermine the frontend design’s “trustworthy numbers” goal:

- **Rent truncation is concealed by the API itself.** `Reader::rent_matures_range` (`crates/xp-store/src/read.rs:430`) stops at `limit`. `handlers/rent.rs::upcoming` always returns `next_cursor: None`, even when more qualifying boxes exist. The homepage requests 500, discards the envelope and displays `rentItems.length` and their summed `due_nano` as “maturing in 720 blocks” (`frontend/src/routes/+page.ts::upcomingRentItems`, `+page.svelte:248`). The dedicated rent loader requests only 100 (`frontend/src/routes/rent/+page.ts`). With more qualifying boxes, the result is a capped prefix without an explicit completeness signal. Whether the current local window exceeds those caps is unmeasured.
- **The day window stops before a day.** `blockWindow()` exits at `WANT_BLOCKS = 800`, in 500-row pages. The offline harness executed the actual load function against synthetic one-minute blocks: two requests loaded 1,000 blocks spanning 16.65 hours; 1,440 blocks belong to the requested day, so 440 were omitted. `dayPartial` notices the short span, but its tooltip incorrectly blames the indexed chain length, and the visible label still says “last 24 h of chain”. This synthetic input proves the boundary defect, not its frequency on mainnet.
- **Errors become zeros in the statistics cards.** `safe()` returns null data plus an error; the Svelte derived values use `data.blocks.data ?? []` and `data.rent.data ?? []`. Errors are shown in some lower panels, but headline transaction/block/reward/rent values can still read zero. The offline failed-dependency probe produced zero day blocks, zero reward and zero rent items from errors.

First contain the misleading presentation: use unavailable states for failed statistics, explicitly label partial windows, and expose capped rent answers as incomplete/lower bounds. Do not make every browser walk an arbitrarily large rent set just to render a card.

Then add a small snapshot-stamped home summary endpoint: exact requested-window counts/sums plus `indexed_height`, window boundaries and `complete`. Reuse header scans for bounded block statistics; for rent, add cursor pagination with a stable window anchor and `(maturity,gidx)` continuation, and compute/cache an exact summary without resolving full box DTOs. If rent aggregation proves expensive, maintain per-maturity counts/sums in the apply/rollback transaction. That option needs schema/rollback design and is at the upper end of the estimate; it is not necessary for the first containment fix. DuckDB is unnecessary for this narrow query.

Acceptance: fixtures with 501 qualifying rent boxes return an exact count of 501 or an explicit incomplete result; pagination recovers all IDs without duplicates; exact-cap answers are distinguished from truncated answers via lookahead. Test a chain with more than 1,000 blocks per day, a genuinely short chain, a failed dependency and values beyond JS safe integer range. Compare the summary with a complete independent walk at the same indexed snapshot. A proposed performance goal is one compact home-summary response instead of transferring full header and rent-box pages; measure response bytes and request count before setting a latency target.

**4. Strengthen the evidence gate before trusting another feature wave.**

The schema-v2 parity gate has never run, as the brief states. More importantly, merely running the current test can produce a green result without useful comparisons:

- `crates/xp-api/tests/parity.rs:724–765` returns successfully when either URL is absent or heights differ (unless forced).
- The final assertion (`:845`) checks only zero mismatches. There is no floor on successfully compared fields/entities. Many malformed responses, HTTP errors and token 404s are accumulated as skips (`compare_token`, beginning at `:517`). The explanation “minted before the indexed range” is not appropriate for a full-history token sampled from that store.
- Token checks cover emission, name and unspent-box **count**, not holder totals, token box-ID sets, template membership or register-index answers. `explorer_token_unspent_count` even treats a 404 as a completed zero count (`:217`).
- A fixed shuffle seed does not make the sample reproducible when its input comes from unordered `HashSet` iteration (`:807`); sort candidates before shuffling.

Make the wrapper distinguish pass/fail/inconclusive, with a nonzero exit for an inconclusive release run. Emit machine-readable attempted/passed/skipped counts per field and enforce explicit coverage floors. Keep exclusions for known reference limits visible. Treat missing explorer token rows as failures when the run declares a full-history store. Compare token ID sets, verify `sum(holder.amount) == supply`, `supply == emission - burned`, and cross-check template/register queries against sampled box content. Internal agreement is useful but is not independent parity.

Add a small browser smoke suite against the real local API, alongside existing deterministic mock tests: shared token/box search, one real cursor continuation, stale status, and statistics failure/completeness states. The shared-ID search bug is direct evidence that partial-store API tests plus hand-written UI mocks can agree with each other while missing full-store behavior.

Success means deliberately removing a required field, returning all skips, changing one holder amount or substituting one box ID makes the appropriate gate fail. A successful artifact must record schema/commit, sampled IDs, coverage, reference provenance and matched tip/hash. Guard cross-request comparisons against tip changes/reorgs; do not label moving-snapshot races as corruption.

**This investigation did not run `scripts/parity.sh`.** It requires a separate reference-node endpoint, which is outside the one allowed address. Do not invoke its defaults here. To retain this task’s network boundary, use previously captured, independently produced reference data offline; otherwise a future parity run needs a separately authorized environment. Reference parity remains outstanding even after internal local checks pass.

**5. Finish the summary API work, then tune from measurements.**

Plan 3a intentionally changed only address transaction lists. The same avoidable expansion remains in `/v1/txs` and `/v1/blocks/{height_or_id}/txs` (`handlers/txs.rs::list`, `handlers/blocks.rs::block_txs`). Both call `tx_dto`, which resolves every input box and tree, every output through its gidx index and tree, and token metadata (`crates/xp-api/src/dto.rs:529`). The block route returns the entire block’s expanded transaction array without pagination. Yet the transaction listing renders only ID, block, age, input/output counts and fee (`frontend/src/routes/txs/+page.svelte`). The block loader also waits for the complete expanded array before rendering its page.

An offline count of committed fixture `1702686.json` found 8 transactions, 129 inputs and 36 outputs; one transaction alone references 103 input/output boxes. On a full store, those boxes imply **366 additional logical key lookups** before token enrichment: `2 × 129` for input box/tree and `3 × 36` for output gidx/box/tree. These are code-derived lookups, not physical disk reads or measured latency. Summaries need none of that box expansion. The fixture calculation is reproduced by `offline-checks.cjs`.

Offer opt-in summary mode or additive summary routes, preserving the existing full API contract. Reuse `tx_summary_dto`; paginate block transaction summaries using their contiguous tx range. Keep expanded detail on demand. The homepage’s transaction-kind badges inspect full transactions (`txKind`), so decide whether to add a cheap explicit classification field or retain a small detail request there; changing types blindly would lose current behavior.

Measure before pursuing lower-level table-open refactors or larger read concurrency. The semaphore and cancellation guard already keep permits until background reads actually finish (`crates/xp-api/src/lib.rs:90`). The 5-second HTTP timeout does not stop an executing blocking read; its regression test acknowledges that. The read budget should remain protective. Add route-template latency histograms, response-size/error counts, overload/timeout totals and blocking-read duration. Existing status contains only current in-flight reads and a rate-limit total; it cannot explain a past saturation episode. Avoid raw addresses/IDs as metric labels.

Acceptance: summary fields match full transaction fields on the same fixtures; list handlers perform no input/output box resolution; compare full versus summary response bytes and success-only latency distributions for ordinary and many-input transactions. Under mixed load, separately record status responsiveness, 200/408/503 counts and time for in-flight reads to return to baseline. A proposed warm sequential list target is p95 below 100 ms, subject to actual local hardware; no claim is made that current code passes or fails it.

**Scope decisions and cost uncertainty.**

Keep WebSocket, mempool, testnet and broad DuckDB analytics deferred. The observed freshness problem can be fixed with existing polling, and exact home totals do not require a general analytics engine. Token-name search may improve discovery, but first fix ID search and avoid conflating unverified token names with identity. On phones, Tokens/Rich list have no primary tabbar entry and the sidebar nav is hidden under 900 px (`+layout.svelte`); an accessible “More” menu is a specific 0.5–1 day follow-up, with keyboard and narrow-viewport route-reachability checks. This repeats a known deferred minor and should not displace the higher-ranked correctness work. No browser rendering or aesthetic assessment was performed.

The brief reports an 89 GB store against the original design’s 30–40 GB index budget: arithmetically 2.23–2.97 times that original target. These are supplied/design figures, not a disk measurement; schema-v2 scope also expanded. No per-table occupancy, free-page ratio, current disk headroom or resync duration was measured. Do not infer that redb or register indexing caused the overrun. A 0.5–1 day sizing follow-up to rank 5 should add read-only in-process table statistics and record store growth plus an isolated restore/resync drill. Success is an attributable byte budget and measured recovery time, not an assumed compression ratio. Do not open/copy the running database through an uncoordinated second process. No such store access occurred here.

**Reproduction artifacts and measurement procedure.**

Files next to this report:

- `offline-checks.cjs` and `offline-results.jsonl`: source-based probes and their actual output. TypeScript is transpiled from the checkout. The status probe substitutes an identity function for `$state` and stubs timers/document; it tests control flow, not Svelte rendering. Home load/series functions execute with synthetic API responses. Fixture counts come from committed JSON.
- `measure-local.py`: fixed-target HTTP harness. It disables proxies and redirects, permits only `/v1/` paths under `127.0.0.1:18091`, makes status the first request, and stops if that request is unsuccessful. It records bytes/timing/status, discovers real addresses/tokens, probes search ambiguity and malformed parameters, walks cursors with duplicate/cycle detection, and optionally generates mixed load. Walks are capped and marked incomplete; mutable holder rankings across blocks must not be mistaken for a stable snapshot.
- `local-results.jsonl`: actual socket-denial result. Its approximately 4 ms duration is client-side failure overhead, **not API latency**.

Run from this directory:

```sh
node offline-checks.cjs > offline-results.jsonl
python3 measure-local.py > local-results.jsonl
python3 measure-local.py --load > local-load-results.jsonl
```

The sequential harness records 12 requests per selected endpoint, including the first separately in the raw sample list. Its p95 is therefore only a small-sample screening statistic. Optional load uses 128 requests at each concurrency of 1, 8, 32 and 64, mixing status, blocks, full transactions and rent. Record successes and rejections separately; quick 503s must not make a latency percentile look like successful service. Cache state is uncontrolled: call these sequential/warm-up observations, not cold-disk benchmarks. Do not drop shared OS caches. The brief says rate limiting is disabled locally, so these runs cannot validate production rate-limit behavior.

Initial attempted request, reproducible verbatim:

```sh
curl --noproxy '*' -sS --max-time 20 \
  -w '\nHTTP %{http_code} time %{time_total}\n' \
  http://127.0.0.1:18091/v1/status
```

Observed: `curl: (7) failed to open socket: Operation not permitted`, `HTTP 000`. No HTTP response existed.

For source verification, paths above are relative to `/home/rkadias/coding/development/arkadianet/ergo-explorer`; inspect with `nl -ba <file>` and verify revision with `git -C /home/rkadias/coding/development/arkadianet/ergo-explorer rev-parse HEAD`. The offline harness reproduces every experimentally quoted non-network number. Defaults, limits and polling/timeout durations are literals in the cited files; all new thresholds and effort figures are proposals.

Previously fixed Important review items—schema-v1 refusal coverage, dust-holder display, API config typo handling, IPv6 limiter behavior and README port—are not reported as open. The full Rust/frontend suites and schema-v2 reference parity were not rerun; no repository tests or builds were presented as passing on the strength of prior reviews.
