# Node-local mempool observations

`GET /v1/mempool` returns one bounded observation from the configured **primary**
node. It accepts no query parameters. The node adapter calls
`GET /transactions/unconfirmed?offset=0&limit=100`, within the published node API's
maximum page size. A 100-item reply sets `limit_reached: true`: more entries may
exist, but are not asserted. There is no lookahead or second mutable page. There is no global
mempool scan, stored pending index, automatic continuation, or request to the
block-body fallback. A node-local response is not a network-wide pending count.

The production binary reuses `[source].url` in its `explorer.toml`; see
[`explorer.example.toml`](../../explorer.example.toml). No separate mempool setting,
database rebuild or proxy route is needed. `source.fallback_url` does not supply
mempool data. After deploying the updated API binary, check the explorer endpoint:

```sh
curl -i https://explorer.kadia.io/v1/mempool
```

A 200 response with `items: []` is a valid empty observation. For a 503, use its
problem `code` below: check reachability and support for
`GET /transactions/unconfirmed?offset=0&limit=100` at the configured primary node.
Changing only the frontend cannot add this backend route; a 404 requires checking
the deployed API version and `/v1/*` proxy target. The shipped production Caddy
configuration already forwards that path to the explorer API.

The JSON contract is `scope: configured_node_mempool`,
`source: configured_primary_node`, `checked_at_ms`, `expires_at_ms`, `cached`,
`limit: 100`, `limit_reached`, `observed_count`, and `items`. `observed_count` always
equals the returned item count, not a guessed upstream total. Each item has a
lowercase transaction ID, input/data-input/output counts, nullable serialized size
in bytes, and nullable exact decimal-string fee in nanoERG. The current decoder
computes fees by summing outputs with the canonical fee script, using exact wide
integers. It does not resolve inputs, value transfers, enrich assets, infer arrival
times, estimate confirmation times, or rank transactions by node ordering.

`[]` is an empty observation only after a successful validated node response.
HTTP 503 problem codes distinguish `mempool_not_configured`,
`mempool_unsupported`, `mempool_unavailable`, `mempool_invalid_response`, and
`mempool_busy`. An outage, timeout, unsupported endpoint, invalid or oversized
response never becomes an empty success. Source URLs and node error bodies are
not exposed. All responses use `Cache-Control: no-store`; errors include
`Retry-After: 1`.

The process shares a single mempool observation permit, a two-second node deadline,
a two-MiB decompressed body limit checked during streaming, at most 100 parsed
transactions, and 10,000 transaction/input/output work entries. Each transaction
admits at most 512 inputs, 512 data inputs, 4,096 outputs and 16-KiB tree hex text.
Duplicate transaction/input IDs and malformed summary fields fail the whole read.
The source JSON parser runs off the async executor and retains admission if request
cancellation outlives parsing. The outer request keeps the same permit until cache
publication, preventing overlapping source requests or late cache replacement.

Successful observations are reused for five seconds; failures back off for two.
Expired successes are never served after a failed refresh. `cached` identifies a
reused observation; timestamps identify its age. These loaded observations are
not a continuing guarantee that a transaction remains pending: it may have been
confirmed, evicted or replaced after observation. The existing transaction-status
route independently checks inclusion when an ID is opened.

The additive `connections` object derives relationships only from the exact same
parsed response. Parsing first builds a unique output-ID map, then matches spending
and read-only references, so node ordering does not affect the result. Optional
output IDs are permitted, with `identified_output_count` disclosing coverage.
Malformed or duplicate output IDs, a supplied creator transaction ID that differs
from its containing transaction, an inconsistent output index, or a transaction
referencing its own reported output fail the observation with
`mempool_invalid_response`. This is structural validation of trusted node facts,
not independent cryptographic verification of box IDs.

No new node route, database query, index or configuration is involved. The same
10,000-work bound covers all transactions, inputs and outputs used for matching.
Matching uses keyed collections rather than expanding shared inputs into every
pair of competing transactions. The wire response returns at most 256 edges,
16 shared-input groups and 256 shared-input transaction memberships. Exact counts
describe the matched source scope; per-list and per-group truncation flags describe
omitted results. All connection evidence expires and fails with its containing
snapshot. An older API's omitted `connections` field is accepted by the client but
displayed as unavailable, not as zero relationships.

Primary implementation checked at Ergo commit
[`23aabead88774d27f2c9190ace3c9abbc8f1d5cb`](https://github.com/ergoplatform/ergo/tree/23aabead88774d27f2c9190ace3c9abbc8f1d5cb):
[`TransactionsApiRoute`](https://github.com/ergoplatform/ergo/blob/23aabead88774d27f2c9190ace3c9abbc8f1d5cb/src/main/scala/org/ergoplatform/http/api/TransactionsApiRoute.scala)
returns transaction outputs and may enrich input boxes when available.
[`ErgoMemPool`](https://github.com/ergoplatform/ergo/blob/23aabead88774d27f2c9190ace3c9abbc8f1d5cb/src/main/scala/org/ergoplatform/nodeView/mempool/ErgoMemPool.scala)
replaces or rejects same-input contenders during admission. Consequently,
same-snapshot shared-input groups are positive observations only; their absence
does not establish a conflict-free network state or identify replacement history.

The `/mempool` page requests one snapshot on entry and then refreshes only on user
action. It filters only the already loaded IDs and labels page capacity, age and
source scope. No background polling or third-party request is introduced.

Primary route reference: the [Ergo node OpenAPI specification](https://github.com/ergoplatform/ergo/blob/master/src/main/resources/api/openapi.yaml)
and [official Swagger overview](https://docs.ergoplatform.com/node/swagger/).
The adapter's list shape and fixed offset/limit request are covered by local HTTP
tests. API tests cover exact fees, empty versus capacity-limited data, cached observations,
expiry/outage/recovery, unsupported and absent node configuration, cancellation,
concurrency, timeout, malformed/oversized replies and primary-only fallback behavior.
