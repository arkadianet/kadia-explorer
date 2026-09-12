# M3 steps 3–4: transaction summaries and legacy safety net

These are initial engineering judgments, **not mainnet maxima, measured capacity,
or a claim that legacy expansion is cheap**. The reviewer's six-block sample has
at most about 164 estimated box/tree resolutions; 10,000 resolutions would leave
about 61x headroom. The sample is not a historical maximum. Summary routes remove
box/tree/token enrichment entirely. That is the intended ordinary-list containment;
first-party frontend migration is deferred to M3 step 5, so existing first-party
traffic does not yet receive that benefit.

## Chosen limits and provenance

| Limit per legacy transaction request | Basis and behavior |
|---|---|
| 10,000 **shared work units**, replacing the proposed 10,000 box resolutions | Engineering choice retaining a generous pathological-case allowance, tightened so tokens and large registers cannot bypass it. One unit per transaction, input/data-input/output slot, output-index lookup, box/tree/token row admission, and token DTO; one per started 128 bytes of registers, tree bytes plus address, or enriched token name. Counts are cumulative, including repeated boxes/tokens. This is not a count of physical disk IOs or consensus execution cost. |
| 2 MiB serialized JSON | Engineering choice bounding the successful response body and its serialization buffer. Exact UTF-8 bytes, including escapes and page/array punctuation, are checked before extending the writer. This is not derived from a block size multiplier and may reject a legitimate block. |
| 2 MiB cumulative encoded rows admitted for decoding | Additional engineering allocation guard, aligned with the response allowance for a simple initial policy. Transaction/box/tree/token lengths are checked before owned row decoding; repeat reads count again. This is not an RSS ceiling: decoded objects, allocators and the serialized body add overhead. |
| Four-second cooperative deadline | Engineering choice leaving nominally one second before the existing five-second HTTP timeout. Starts inside the worker before store reads and covers expansion and serialization. No host latency measurement justifies a drain guarantee yet. |

Logical work is charged before the corresponding DTO vectors, register parsing,
hex conversion or token enrichment. Admission can reject a whole pending batch
without performing it. Expansion loops check at each item; register parsing reads
at most 128 bytes between checks, tree hex conversion uses 128-byte chunks, and the
JSON writer checks before each at-most-128-byte extension. Stored vector decoding checks each element and copies byte strings in 128-byte chunks.
A blocked redb call or allocator is not preemptible; the deadline is cooperative, not a hard
wall-clock bound. Reader permits and inflight guards stay inside `spawn_blocking`,
through final serialization, even after cancellation/HTTP timeout.

Only `/v1/txs`, `/v1/txs/{id}`, and `/v1/blocks/{height_or_id}/txs` use this new budget.
Existing historical readers retain their independent, stricter limits (including
250 ms for historical pages), admission semaphore, and response accounting.

## What a worst-case block implies

Ergo's `maxBlockSize` bounds the serialized **transactions section**, not the
explorer's expanded input boxes or enriched JSON. The launch default is 524,288
bytes; the parameter is voteable. The `MaxBlockSizeMax = 1 MiB` constant in reference
source is not in its `maxValues` enforcement map, so treating it as an immutable
ceiling would be wrong. [Reference Parameters.scala](https://raw.githubusercontent.com/ergoplatform/ergo/master/ergo-core/src/main/scala/org/ergoplatform/settings/Parameters.scala).

Ergo's documentation currently publishes 1,271,009 bytes for maximum block size
and 4,096 bytes for maximum box size. These are published values consulted during
this implementation, **not a production-node parameter capture at a named height**.
[Ergo voting documentation](https://docs.ergoplatform.com/mining/gov/voting/).

For a ceiling `B`, input ids alone imply `I <= floor(B / 32)`. At the published
`B = 1,271,009`, that is 39,719 input references and up to 162,689,024 bytes
(155.15 MiB) of referenced 4 KiB boxes, before JSON, outputs, trees, and token names.
This is a deliberately loose **input-only size envelope**, not an attainable valid
block or a historical maximum: proofs, outputs and transaction framing also need
space. Even the launch 512 KiB value gives an input-only envelope of 64 MiB.
Hex-encoded payloads can roughly double their own contribution to JSON; enrichment
and repeated token names add further bytes.

The reference validator also charges input/output/data-input and token costs plus
script execution against `maxBlockCost`. Thus for positive effective input cost
`cI` and block cost `C`, the stronger input envelope is
`4096 * min(floor(B/32), floor(C/cI))` bytes, with actual feasible counts lower
because other work consumes cost. Output and token bounds also depend on those
voted costs. [Reference transaction validation](https://raw.githubusercontent.com/ergoplatform/ergo/master/ergo-core/src/main/scala/org/ergoplatform/modifiers/mempool/ErgoTransaction.scala).

For example, 500 historical 4 KiB input boxes represent 1.95 MiB before JSON, despite
only 16,000 bytes of input ids in the new block. That is a size illustration, not
a claim that 500 inputs plus their remaining validation work fit a particular
mainnet epoch. A genuinely attainable maximum and its elapsed time require the
height-specific parameter set, constructible scripts/boxes, and host measurements;
none was established by six samples or by this implementation. It would be false
to supply an exact worst-case milliseconds/JSON figure here.

Implementation cost scales with inputs + outputs + token occurrences + register,
tree/name bytes + serialized JSON. Its added preflight reads mean up to four
box/tree table reads per resolved input and five index/box/tree reads per output,
plus two token-table reads per token occurrence (and transaction/header/meta reads).
An unbounded expansion of the input-only envelope above would therefore involve
158,876 box/tree table reads before token enrichment. Successful requests here
instead must satisfy **all** the work/decoded-byte/JSON/deadline limits. A valid
pathological block can return 422; it is never represented by a shortened array.
The limits do not promise ordinary latency. Deployment-class sizing remains M3
step 6, outside this task.

## Additive contract and evidence

- `/v1/tx-summaries` reuses global gidx traversal.
- `/v1/blocks/{height_or_id}/tx-summaries` walks only the block's contiguous gidx
  range, in `dir=asc|desc` order (default `desc`). Cursors are exclusive; block pages
  have no continuation once the range is exhausted. Existing limit parsing applies.
- Both return existing `TxSummaryDto` fields in `{items, next_cursor}`. Input and
  data-input counts use checked `u16` conversion; overflow is `500 integrity_error`.
  The preexisting address summary route retains its previous conversion behavior.
- Legacy successful JSON is compared against the original full DTO/enrichment path
  for every fixture transaction and block, including all fields and nulls.
- Store-scoped test-only counters count box/tree/token enrichment access. All new
  summary route walks assert `[0, 0, 0]`; the full DTO path increments all three.
  The feature is enabled by an API dev-dependency, not an operator metrics endpoint.
- Legacy failures use `422 application/problem+json` with `expansion_work_limit`,
  `expansion_decode_limit`, `expansion_response_limit`, or `expansion_deadline`.
  The whole worker result is discarded on failure; no response bytes are streamed.

Tests cover exact/one-over work, decode bytes, serialized bytes (including escapes),
and deadline; real token/register expansion at the work boundary; one oversized
input/output transaction before lookups; cumulative multi-transaction rejection;
oversized register/tree/token rows; summary count boundaries, empty/missing ranges,
both orders and exclusive cursors; unchanged legacy JSON; cancellation permit
retention. See `WORK-REPORT.md` for candidate gate evidence.
