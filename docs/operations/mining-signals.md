# Mining and header signals

`/mining` inspects an explicit range of retained canonical headers. Its public-key rankings describe header keys, not distinct people, pool operators, ownership or measured hashrate. One entity may use several keys. Header versions and raw three-byte vote tuples describe what was recorded, not activated parameters or governance outcomes.

The page makes no request until **Load mining signals** is selected. It does not poll. **Use latest 720 blocks** only fills the range; 720 blocks is not a guaranteed elapsed day. A successful result pins its end-block ID. Refreshing that range preserves the pin; a changed or rolled-back end block returns a conflict. Clearing the pin or editing the selection removes the previous result before another explicit load. Pinned links also require an explicit load.

## API

```sh
curl 'http://127.0.0.1:3000/v1/mining?from_height=1866000&to_height=1866002&top=20'
```

The listener address above is illustrative; use the operator's configured API address. `from_height` and `to_height` are required positive inclusive decimal heights. `top` defaults to 20 and accepts 1–50. Optional `end_block_id` is the 64-hex canonical ID at `to_height`. Unknown and duplicate query fields are rejected.

Responses contain:

- `scope: canonical_block_headers`, `consistency: single_reader`, `complete: true`, the requested heights and block count, the indexed height, `full_history`, `partial_from`, and the end `anchor`.
- `totals.fees` and `totals.transaction_count` as exact decimal strings. Fees are the existing indexed transaction fee-output totals. They do not prove collection by the corresponding key. No legacy `HeaderRow.reward`, subsidy, re-emission or storage-rent income is used.
- `miner_keys.items`, ranked by descending block count and then ascending raw key. Each item includes the full 33-byte key, exact block count, first/last occurrence heights and block IDs, and exact fee-output total for its blocks. `distinct_count`, `other_key_count`, `other_block_count` and `other_fees` account for every omitted key.
- `versions`, all observed version bytes in ascending order with block counts.
- `votes.items`, raw three-byte hexadecimal tuples ranked by descending count and then ascending tuple. `known_blocks` and `unknown_blocks` partition the complete selected range. Missing, malformed or duplicate vote fields in stored header JSON count as unknown. They are never treated as `000000`. `zero_vote_blocks` counts only that literal tuple. `distinct_tuples`, `other_tuple_count` and `other_block_count` describe omitted tuples exactly.

Every visible share uses **all requested blocks** as its denominator, including blocks with unknown vote fields. The page shows exact fractions, not rounded ownership percentages. Decorative bar widths are rounded down to tenths of a percent. `top` only limits the displayed key/tuple lists; it never limits aggregation coverage. Versions are not truncated.

The header range is read from one `Reader` snapshot, with canonical parent linkage checked. The endpoint reuses historical-read admission and requires every requested header. A partial index can serve a complete retained range and reports its partial coverage explicitly. No partial ranking is returned for a missing header or a range beginning before retained history. Malformed stored binary rows remain errors; an unusable raw vote field alone only reduces vote coverage.

## Bounds and failure behavior

Each request allows at most 20,160 headers, 64 MiB of stored header bytes admitted before row decoding, four seconds of bounded work, and a 128 KiB serialized response. The raw JSON parser skips unrelated fields without building an arbitrary JSON object tree. Maps are bounded by the requested header count. Public keys and votes are read only from indexed data; no external pool directory or third-party API is contacted.

`400 invalid_mining_query` rejects invalid ranges/options; `404 mining_unavailable` means the requested end height is unavailable; `409 mining_conflict` rejects a changed or rolled-back pin; `422 mining_incomplete` rejects unretained prefixes. Budget exhaustion returns a `422 mining_*_limit` or `mining_deadline` problem. A missing/disconnected retained header is a server integrity error. None of these failures becomes an empty successful ranking.

The frontend admits at most 128 KiB, has a ten-second fetch deadline, validates count/sum/coverage/order/anchor invariants, aborts replaced requests, and discards late results. Errors clear earlier results. Original, Aurora, Atelier and Prism have separate desktop compositions and preserve controls and readable keys at a 320-pixel viewport.

Focused tests apply real retained block fixtures, then exercise stable append pins, rollback and replacement, missing/disconnected headers, partial prefixes, exact large amounts, stable ties and remainders, unknown vote fields, and admission before oversized row decoding. The small frontend JSON fixture is synthetic and exists to exercise precision and coverage; it is not claimed as a mainnet capture.
