# M5 ordinary route policy matrix

Step 1 evidence, before codec implementation. Counts: **4 immutable membership, 10 current-state** (14 route/order entries, 13 distinct paged paths). Both supported directions share a policy; both unspent filter values share a policy. Policy tests live in `crates/xp-api/src/paging.rs`.

Immutable membership means canonical membership below the original height/transaction bound survives appends; continuation requires the anchor to survive. Current-state means ANY observed tip change invalidates continuation, even a normal append. Steps 1–2 established this contract; steps 3–4 now integrate and exercise it in the backend handlers.

| Policy key | Route / order | Policy | Field or membership evidence; binding / bound |
| --- | --- | --- | --- |
| `blocks` | /v1/blocks, desc | immutable membership | `handlers/blocks.rs::list`, `dto.rs::block_dto`: stored HeaderRow facts only; no confirmations or tip enrichment. Cap heights at original anchor. |
| `global_summaries` | /v1/tx-summaries, asc/desc | immutable membership | `handlers/txs.rs::summaries`, `checked_tx_summary_dto`: id, height, index, timestamp, size, fee, input/data-input/output counts from immutable TxRow. Cap transaction gidx at anchor block end, derived from store. |
| `block_summaries` | /v1/blocks/{height_or_id}/tx-summaries, asc/desc | immutable membership | `handlers/blocks.rs::summaries`: same immutable summary fields; contiguous selected-block transaction membership. Bind resolved canonical block id (including height-path requests); derive its range in Reader. |
| `address_summaries` | /v1/addresses/{addr}/txs, asc/desc | immutable membership | `handlers/addresses.rs::txs`, `tx_summary_dto`, `read.rs::tree_txs`: historical tree/transaction membership plus immutable TxRow summary, no balance. Bind resolved tree hash and cap gidx at anchor block end. |
| `transactions` | /v1/txs, asc/desc | current-state | `handlers/txs.rs::list`, `budget.rs::Budget::tx`: expanded input/output BoxDto carries `spent_by`, `spent_height`, `rent.claimable_at_tip`. Transaction membership alone is insufficient. |
| `address_boxes` | /v1/addresses/{addr}/boxes, asc/desc, unspent=false/true | current-state | `handlers/addresses.rs::boxes`, `dto.rs::box_dto_from_reader`: `spent_by`, `spent_height`, `rent.claimable_at_tip`; live membership also changes when spent. Bind tree hash and unspent. |
| `token_boxes` | /v1/tokens/{id}/boxes, asc/desc, unspent=false/true | current-state | `handlers/tokens.rs::boxes`: same mutable BoxDto fields, including on all-box walks. Bind token id and unspent. |
| `template_boxes` | /v1/templates/{hash}/boxes, asc/desc, unspent=false/true | current-state | `handlers/templates.rs::boxes`: same mutable BoxDto fields. Bind template hash and unspent. |
| `tokens_newest` | /v1/tokens, sort=newest, desc | current-state | `handlers/tokens.rs::list`, `dto.rs::token_info_dto`: mint order is stable but `burned`, `supply`, `holder_count`, `box_count` are mutable TokenRow enrichment. |
| `tokens_holders` | /v1/tokens, sort=holders, desc | current-state | Same TokenInfoDto enrichment; `read_tokens.rs::tokens_by_holders` also ranks by mutable holder count. Sort is a separate binding. |
| `holders` | /v1/tokens/{id}/holders, desc | current-state | `handlers/tokens.rs::holders`: `amount` changes ranking; `share_pct` depends on current emission minus burned. Bind token id. |
| `richlist` | /v1/richlist, desc | current-state | `handlers/richlist.rs::list`: `nano` changes both value and ranking. Legacy dir is ignored; strict binding must use effective desc order. |
| `register_boxes` | /v1/registers/{reg}/{value}/boxes, asc/desc | current-state | `handlers/registers.rs::boxes`: register membership below gidx is stable but returned BoxDto spent/rent fields change. Bind register number and hash of decoded value bytes. |
| `rent_eligible` | /v1/rent/eligible, asc | current-state | `handlers/rent.rs::eligible` and `items_of`: live RENT_MATURES membership is removed by spend; eligibility boundary moves with tip; expanded box rent state changes. Legacy dir is ignored; effective order is asc. |

Nonpaged exclusions: `/v1/rent/upcoming` is a capped window prefix (`complete`, always-null `next_cursor`), not a pageable list; `/v1/addresses/{addr}/rent` is a capped scan sorted afterward (`truncated`), not an exhaustive earliest-maturity answer. They remain explicitly samples. `/v1/blocks/{height_or_id}/txs` stays an all-or-error array. Detail/search/supply/template examples are not paged walks. `/v1/addresses/{addr}/balance/at` and `/boxes/at` keep their existing historical anchors and cursor contract unchanged; existing history tests remain regression targets.

The subtle classifications are newest token listings (immutable ordering with mutable projection), all-box/register lists (immutable membership with mutable spent/rent enrichment), and address transactions (summaries despite the `/txs` name, so no mutable expansion). No monetary field is used to classify a summary as mutable merely because it is an amount: the stored transaction fee is immutable.

## Steps 3–4 implementation evidence

All 14 entries above now use `paging::Request` with the page's own Reader. `consistency=strict` requires a bound `snapshot`/legacy `cursor` pair after page one. Responses add `consistency`, `observed_anchor`, original `anchor`, and `next_snapshot`. Bare cursors remain `best_effort`, with no claimed original anchor or continuation token. A null anchor reports unavailable canonical identity; missing required in-range headers still fail as corruption.

Immutable global/address summary reads cap the store range at the original header's exclusive transaction allocation end before resolving rows. Blocks cap heights; block summaries retain their selected block's range and bind its canonical id. Current-state projection reads run only after exact height/id tip validation. Address/block selector resolution first checks token route/order/cursor and anchor availability, so a reorg removing an address or rebinding a height produces 409; complete normalized filter binding is still required before projection reads. This is the awkward case beyond the matrix's mutable-enrichment classifications.

`crates/xp-api/tests/routes.rs` contains six `m5_` in-process tests over temporary Stores:

- Every route/order (including both unspent filters) walks at least three pages, compares every item with a single-page result, verifies uniqueness/termination, and repeats continuation after closing/reopening the Store.
- Applying a block actually reverses holder and rich-list leaders; every current-state continuation must return 409 `snapshot_changed` with no `items`. Immutable walks preserve exact original items, anchor and cursor traversal while reporting the advanced observed anchor.
- Rollback below the anchor, same-height anchor replacement, and proven transaction gidx reuse on a fork all reject every affected continuation. Reorgs removing selected entities or replacing a selected block also return 409, including block id and height paths.
- HTTP cursor/token cross-pairs, directions, unspent filters, entities, route families, token sorts and register filters reject with 400. Canonical block id/height aliases share a binding. Sparse address traversal ends empty below its original allocation bound even after an append adds another matching member.
- Empty/genesis-only/permitted headerless partial snapshots invent no anchor; in-range missing headers retain the M2 integrity error.

The pre-M5 `summary_block_empty_missing_range_and_exact_count_boundary` expected only two JSON fields. Its whole-response assertion now includes the additive metadata; the old items/cursor, array, corruption and count-boundary assertions are retained. No existing assertion was relaxed. Frontend integration and the separate compatibility deliverable (steps 5–6) remain outside this change.
