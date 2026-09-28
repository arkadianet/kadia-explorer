# Address activity

`GET /v1/addresses/{addr}/activity` computes exact address balance changes from retained
boxes in one database Reader. It does not infer the owner, a payment path, or who paid
the transaction fee. No new index, schema migration, or chain resync is needed.

Query parameters:

- `limit`: 1–100, default 20. `dir`: `asc` or `desc`, default `desc`.
- `asset`: `all` (default), `erg`, or an exact 64-character token ID.
- `direction`: `all` (default), `received`, `sent`, `mixed`, `neutral`, or `unknown`.
- `from_ms` and `to_ms`: UTC Unix milliseconds, inclusive start and exclusive end.
  Both are optional; when both exist, start must precede end. The maximum is
  8,640,000,000,000,000 (the browser Date bound). Block timestamps are not assumed
  monotonic, so dates never cause an unsafe early end to a scan.
- `consistency`: only `strict` is supported and is also the default. After page one,
  send the returned `next_cursor` and `next_snapshot` together as `cursor` and
  `snapshot`. Canonical tip changes return 409 `snapshot_changed`; restart the walk.
  Changing address, asset, direction, dates, sort order or cursor invalidates the pair.

The response contains `items`, `next_cursor`, `next_snapshot`, `consistency`, `anchor`,
`observed_anchor`, `scanned`, `scan_limit_reached`, and `partial_from`. At most 200
indexed address transactions are scanned per request. A filtered page can contain
zero items and still have a continuation. Only a null `next_cursor` means completion;
the final continuation may return an empty page. `partial_from` discloses incomplete
chain history: transactions involving only missing pre-index inputs may not be members
of the address index and cannot be discovered by this endpoint.

Each item has `id`, `height`, `block_id`, `timestamp`, `index`, `fee`, `input_count`,
`output_count`, `coverage` (`complete`, `resolved_inputs`, `total_inputs`), `erg_delta`,
`tokens` (`id`, `name`, `decimals`, `delta`), `direction`, and `asset_match`.
Amounts and fees are decimal strings in raw units. Positive amounts have no leading
plus sign. Token names are unverified on-chain metadata, not identity.

The ERG delta is selected-address outputs minus selected-address inputs in nanoERG;
token deltas use the same subtraction in raw token units. Touched tokens remain in
the list when their delta is zero. The separate fee is a fact about the transaction,
not a second deduction or an assertion that this address paid it. Direction reflects
the selected asset: with `all`, a token gain and ERG loss is `mixed`. Zero changes are
`neutral`, not a claimed self-transfer.

If any input is unresolved on a declared partial database, every net delta is null
and direction is `unknown`: an unresolved input might belong to the selected address.
Known tokens remain listed with null deltas. Under a token filter such a candidate is
retained with `asset_match: "uncertain"` unless known selected-address boxes prove
token involvement (`definite`). Direction filters exclude unknown rows except for
`all` and `unknown`. Missing inputs on a full database, missing outputs, and mismatched
spending/ownership references are integrity errors, not incomplete coverage.

All transaction, box and token rows pass encoded-byte admission before decoding.
The request shares existing transaction budgets: 10,000 work units, 2 MiB decoded
row bytes, 2 MiB serialized JSON, and a four-second cooperative deadline. Over-budget
requests fail atomically with 422 and `activity_work_limit`, `activity_decode_limit`,
`activity_response_limit`, or `activity_deadline`; no truncated monetary result is
returned. Reduce the page limit or time window. A single oversized transaction can
still exceed these bounds; consult its summary/evidence instead.
