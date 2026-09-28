# Token transaction history

`GET /v1/tokens/{id}/txs` returns ordinary `TxSummaryDto` items from a derived token
membership index. A token occurs in a transaction when a retained resolved input or
output contains that token ID. This includes mints, burns, change and contract movements;
it makes no claim about payments, transfer paths, owners or transferred amounts.

`limit` is 1–100 (default 20), `dir` is `asc` or `desc` (default `desc`). Consistency is always
strict; pass each `next_cursor`/`next_snapshot` together as `cursor`/`snapshot`.
The standard anchor metadata is included. Appends preserve the original immutable
summary membership and anchor allocation bound. Reorganizations invalidate continuation
with 409 `snapshot_changed`; changed token/order/cursor/index-version bindings return 400.

`history_context` contains `scope: "indexed_token_touches"` and `partial_from` (height or
null). Partial indexing can miss transactions whose only token involvement is an input
from before the indexed range. Per-row input coverage and amounts are not asserted by
this summary feed. A null final cursor means the indexed membership walk is exhausted.

The auxiliary index is version 1 and is excluded from canonical table fingerprints.
There is no core schema or undo encoding change. It has forward `(token_id,tx_gidx)`
and reverse `(tx_gidx,token_id)` keys, plus build metadata. Apply writes resolved input
and output membership atomically; rollback removes memberships by canonical transaction
before removing that transaction. Key uniqueness deduplicates repeated boxes.

Existing installations rebuild automatically in the background from retained boxes.
Each box contributes its origin transaction, and its spending transaction when present.
Genesis boxes have no origin transaction and contribute only a later spend. Missing
required canonical references fail the build; no invented transaction IDs are emitted.
Headerless partial starts remain unavailable until ingest establishes an anchor.

Every read validates version and a full canonical header ID/height plus allocation
counters and retained-box count. Every core mutation checks this same pre-mutation
anchor before updating it. Consequently an older binary's missed writes trigger a
bounded clear/rebuild on reopen, and one new apply cannot certify an incomplete index.

While stale or rebuilding the API returns 503 `token_history_preparing`, never a false
empty page. The worker admits one serialized write batch at a time, pauses 25 ms between
batches and joins admitted writes at shutdown. Batches process at most 500 boxes or
index removals, 8 MiB decoded box bytes, 2 MiB per box and 10,000 membership work units;
a 250 ms cooperative boundary yields between boxes. One oversized row fails explicitly
and leaves history unavailable while ordinary ingest and APIs continue. Restart after
resolving a build failure resumes the persisted build state.

HTTP reads traverse only indexed token/transaction keys, with admission before owned
transaction decode. They use the existing 2 MiB decoded/response, 10,000 work and 4 s
deadline budgets; an oversized page returns 422 `token_history_budget` atomically.
