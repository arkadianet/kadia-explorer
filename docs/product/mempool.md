# Mempool observations

`/mempool` loads one bounded observation when opened. Further checks use the explicit
**Refresh mempool** button. It does not poll. The source is the configured primary
Ergo node; a local pending observation is neither confirmation nor a network-wide
count. Transaction links open live status and the available pending inputs and
outputs immediately, before local inclusion.

The homepage also loads one compact observation independently of its block and
transaction panels. It shows the returned count, at most three transaction links,
exact fees when supplied, and UTC observation time. A node failure does not block
the homepage or become a zero count. Refresh is explicit; the preview never fetches
transaction details for each returned item.

The **Pending connections** view matches spending inputs and read-only data inputs
to outputs identified in this same returned snapshot. Select a transaction to see
its immediate producers and consumers; the selection is shareable as
`/mempool?focus=<64-character lowercase transaction ID>`. Changing the selection,
following a neighboring transaction within the view, and expanding lists use only
loaded data. They do not fetch another node page or transaction graph. The ordinary
transaction links still open the independent live-status page.

Solid connections represent spending inputs; dashed connections represent read-only
data inputs. Both have accessible lists with complete box IDs available under
**Box reference**. An output lacking an ID cannot be matched, and coverage shows
how many output IDs were supplied. A parent may appear after its child in node
order. Unmatched inputs can be confirmed boxes or outside the returned page; they
are not evidence that a transaction is blocked. A focus absent from the page is
described as absent from this loaded snapshot, not absent from the node or rejected.

If multiple returned transactions reference the same spending input, their IDs are
shown as a positive shared-input observation. No winner, replacement, rejection or
eventual inclusion is inferred. The existing transaction-status route separately
reports conflicts supported by the confirmed index.

Connections are optional for compatibility with earlier API versions. Missing
connection data has an unavailable message, while a supported empty list means no
matching references were returned within the disclosed scope. A refresh clears
both the transaction list and connections before requesting the next observation.

The page uses `GET /v1/mempool` without query parameters. A successful response has
scope `configured_node_mempool`, source `configured_primary_node`, observation and
cache-expiry times, a `cached` flag, `limit: 100`, `limit_reached`, `observed_count`
and up to 100 summary items. Each summary contains the ID, input/output/data-input
counts, nullable size and nullable exact decimal-string miner-fee output total.
The page does not infer input values, address deltas, token movements, arrival
times, priority or fee payer from these summaries.

The additive `connections` object has scope `returned_snapshot_only`, output and
identified-output counts, an exact `edge_count` within that scope, up to 256 `edges`,
and `edges_truncated`. Each edge supplies `producer_id`, `consumer_id`, `box_id` and
kind `spend` or `read`. Shared spending inputs have an exact `shared_input_count`,
up to 16 `shared_inputs` groups, and `shared_inputs_truncated`. Each group supplies
its box ID, exact transaction count, returned transaction IDs and its own
`truncated` flag. At most 256 transaction memberships are returned across these
groups. Truncation is displayed and is independent of the node's 100-item page
capacity. Initially four references per side are shown; explicit buttons expand
the already returned references.

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

The desktop list aligns transaction IDs, exact fees and input/output counts in a
compact ledger. Size, data-input count, full IDs and raw fees remain available in
each row's **Details** disclosure. Standard and Compact density work independently
of appearance. Original uses ruled rows; Prism uses raised rows beside a crystal
inspector; Atelier places its inspector to the left of an open paper ledger;
Aurora uses an inset ledger with leading connection controls and rounded sections.

Connections occupy a bounded side inspector on desktop. On mobile, the native
**Pending connections** disclosure starts closed unless a focus link was opened.
Selecting a row opens and focuses it; selecting a neighbor changes the local
inspection and shareable URL without another request. Lists, declarations and
coverage warnings remain accessible through native buttons, links and disclosures.

The transaction page consumes optional `pending.details` from its existing status
request, with no additional node or box requests. It shows spending-input IDs,
read-only input IDs and declared output values, IDs when supplied, token IDs and
exact raw token quantities. Input values and assets remain unknown. Safely derived
output addresses link to indexed address pages without identifying an owner or
inferring payment intent; a missing address leaves the exact script available to
inspect and copy. Pending outputs and tokens may not yet have indexed records.

This projection retains at most 32 input IDs, 32 data-input IDs, 32 outputs and 32
tokens per output within 32 KiB. Scripts over 2,048 bytes are omitted explicitly;
address derivation only attempts retained scripts of at most 512 bytes. Per-list
truncation, missing asset entries and script omissions remain visible. Initially
four loaded inputs and outputs are displayed, with local expansion controls.
Neither the projection nor its `complete` flag establishes network acceptance,
transaction validation, payment volume or an input/output net balance.

Older servers keep their summary-only state. Malformed detail projections are
rejected for display without replacing the available summary. A pending-to-confirmed
transition removes the pending projection and loads the canonical receipt. Retained
details after an outage or mempool disappearance are explicitly historical evidence.

Unit tests cover exact quantities, bounds, unavailable responses, cancellation and
absence of polling. Browser fixtures are explicitly synthetic: they verify one
entry request, local filtering, empty/error/capacity states, failed-refresh clearing
and keyboard access in all four appearances in light and dark modes at 320px.
