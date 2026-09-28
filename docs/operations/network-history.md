# Bounded network history

`GET /v1/network/history?from_height=1866000&to_height=1866719&buckets=60`
reads an inclusive range of canonical block headers in one `Reader` snapshot.
The range is explicit: positive decimal heights, at most 20,160 blocks, and
1–120 requested buckets (default 60). Unknown/duplicate query fields and invalid
ranges return 400. Buckets contain `ceil(block_count / requested_buckets)` blocks;
the last bucket may be shorter. These are height buckets, never calendar days.

`end_block_id=<64 hex>` optionally pins the requested end height. A changed block,
or a rollback below that height, returns 409 `network_history_conflict`. Appending
new blocks does not change a pinned historical range. The client pins every loaded
result and shared link, clears the result before refreshing, and requires an explicit
clear-pin action to accept a replacement chain. Loaded results are observations;
there is no background refresh or continuing canonicality guarantee.

The response contains `scope: canonical_block_headers`, `consistency: single_reader`,
the requested range's `anchor`, `indexed_height`, `full_history`, `partial_from`,
`requested_buckets`, `bucket_width`, `totals` and ascending `buckets`.
`complete: true` means every requested header is present, including when the database
has partial older history. A range beginning before retained history returns 422
`network_history_incomplete`; an unavailable unpinned end returns 404
`network_history_unavailable`. A missing interior header or broken parent linkage
is an integrity failure, never a shortened chart.

Each aggregate includes inclusive heights, block count, exact decimal strings for
transaction count, fee total in nanoERG, serialized block bytes, and minimum,
maximum and end-block difficulty. Difficulty is the indexed header value, not an
estimated hashrate. No average difficulty is inferred. First/last timestamps follow
height order; earliest/latest timestamps report the observed extrema. Clock
regressions do not reorder blocks or drop them. Counts and fees use sufficiently
wide integer accumulators; difficulty remains an exact u128 decimal string without
a potentially overflowing sum. Fees follow the store's existing block-fee accounting.
No market prices, circulating supply or miner identities are introduced.

Work shares the API's two historical-read permits and ordinary read admission.
Each request is limited to 20,160 header visits, 64 MiB of encoded header bytes
admitted **before decode/copy**, a four-second deadline, 120 aggregates and a
256 KiB serialized response. Headers are consumed one at a time. Resource failure
returns a named `network_history_*_limit` (or `network_history_deadline`) 422 with
no partial result. Route metrics use a fixed `/v1/network/history` label.

The `/network` page loads only after an explicit action. Its latest-720 shortcut
copies the current indexed height; it does not assert a 24-hour duration. A partial
index may require a smaller starting range. Charts scale bounded display ratios
using exact integer arithmetic, while the keyboard-scrollable table retains every
raw amount and difficulty value. Four appearance presets and both color modes
are supported. Only the same-origin API is requested; no external analytics service
or new database index is required.

Regression coverage includes real apply/append/rollback/replacement reads,
precision above 2^53 and u128 difficulty, clock regressions, incomplete and missing
headers, query limits, a malformed oversized stored row rejected before decode,
shared deadline/work/byte budgets, client aggregate validation, pinned refresh,
late response cancellation, and narrow layouts. The browser suite uses deterministic
route overrides; backend tests exercise the actual router and store.
