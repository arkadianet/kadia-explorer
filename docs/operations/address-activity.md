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

## Loaded activity summary

The address view summarizes only the loaded rows matching its current filters and
snapshot. Counts, direction categories and UTC bounds describe that loaded subset,
not an entire date range or lifetime history. ERG changes are summed with `BigInt`;
any unresolved input coverage or null delta withholds the entire net aggregate.
The exact raw nanoERG value remains available alongside the formatted amount.

## Local saved addresses, groups and backups

The browser's `xp-saved-addresses` entry stores up to 100 addresses with local labels,
creation/update timestamps, and an optional group. A group is an organizational label,
not evidence of shared ownership. Labels are limited to 80 characters and groups to 40;
control characters are rejected. Group filtering and edits make no chain requests.

Version 1 entries remain readable without an eager rewrite. A deliberate save, remove
or merge writes version 2, preserving existing addresses, labels and timestamps;
label-only callers preserve an existing group. Clearing browser data removes this list.

**Export JSON backup** reads the current local data and downloads a version 2 envelope
with `format: "kadia.saved-addresses"`, `exported_at`, and `items`. Export does not mutate
storage. The backup is plain text, contains local labels/groups, and is not encrypted.
Import also accepts structurally valid version 1/2 storage envelopes. Files are capped
at 512,000 UTF-8 bytes and 100 entries; the merged list cannot exceed 100 unique addresses.
Invalid entries, duplicate addresses and unsupported versions reject the entire import.
Address checks are structural only; importing does not establish on-chain validity.

Import first presents additions, unchanged entries and field conflicts without writing.
The default merge preserves existing labels and groups. Replacing them—including blank
values—requires choosing the explicit backup-fields policy. Existing `added_at` values
are preserved; new entries retain their backup timestamps. Before committing, merge
re-reads storage and compares the exact bytes captured for the preview. An intervening
edit requires a fresh preview. Corrupt or inaccessible browser state blocks writes and
export; it is never replaced by an empty list or by an imported backup. Storage failures
leave both persisted bytes and the last usable in-memory list intact.
