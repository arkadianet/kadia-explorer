# Mempool observations

`/mempool` loads one bounded observation when opened. Further checks use the explicit
**Refresh mempool** button. It does not poll. The source is the configured primary
Ergo node; a local pending observation is neither confirmation nor a network-wide
count. Transaction links open the existing lifecycle view.

The page uses `GET /v1/mempool` without query parameters. A successful response has
scope `configured_node_mempool`, source `configured_primary_node`, observation and
cache-expiry times, a `cached` flag, `limit: 100`, `limit_reached`, `observed_count`
and up to 100 summary items. Each summary contains the ID, input/output/data-input
counts, nullable size and nullable exact decimal-string miner-fee output total.
The page does not infer input values, address deltas, token movements, arrival
times, priority or fee payer from these summaries.

`observed_count` is the returned item count. `limit_reached` is true when 100 items
are returned; it is not evidence of a 101st transaction. At capacity the UI says
additional transactions **may** exist. No unstable second page is fetched to make
a global count. Filtering searches only the already loaded IDs and retains the
original observation count and scope.

An empty list is shown only after a successful validated response. The 503 codes
`mempool_not_configured`, `mempool_unsupported`, `mempool_unavailable`,
`mempool_invalid_response` and `mempool_busy` have unavailable states. Refresh clears
old rows before fetching; a later error cannot preserve an old successful count.
Navigation aborts requests and discards late responses. Checked time remains a
loaded observation, never a live badge.

The client accepts at most 100 unique transaction IDs, the fixed response scope,
bounded counts, u32 sizes and u128 fee strings. It limits HTTP bodies to 256 KiB
before JSON parsing, validates UTF-8, and aborts after ten seconds. Date fields must
fit JavaScript's supported date range. Monetary display uses BigInt throughout.
Unknown fees or sizes are labelled as unavailable rather than shown as zero.

Original uses a compact list below its observation summary. Prism places the source
observation beside raised transaction panels. Atelier puts observation metadata in
the outer margin of an open ledger. Aurora uses paired rounded transaction panels
beneath its centered introduction. All become one column at narrow widths.

Unit tests cover exact quantities, bounds, unavailable responses, cancellation and
absence of polling. Browser fixtures are explicitly synthetic: they verify one
entry request, local filtering, empty/error/capacity states, failed-refresh clearing
and keyboard access in all four appearances in light and dark modes at 320px.
