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

## Saved group balance dashboard

The Saved page's balance selection is exactly the visible saved-address list: the
selected local group intersected with the local label/address/group text filter.
**Load group balances** explicitly sends those address strings together in
`POST /v1/addresses/balances` with `{ "addresses": [...] }`. Labels and group names
remain local. Page load, filtering and editing do not trigger balance requests.
The UI discloses the shared request before loading; neither grouping nor the response
establishes ownership. Limits are 1–100 addresses, 4,096 UTF-8 bytes per address,
128,000 address bytes total, and a 512,000-byte JSON request.

The response uses one store Reader snapshot. Canonical script aliases are explicitly
linked to their first member and counted once. ERG values are rendered exactly from
integer nanoERG strings; token quantities remain raw integer units with token IDs,
without token-name, decimals or price assumptions. Individual balances and combined
totals are checked for consistent member order, alias mappings and exact sums before
display. Token lists render bounded chunks with explicit local expansion.

An invalid or unseen member withholds the combined ERG and token totals; unknown
balances never become zero. A partial index may return **observed balances only**,
with incomplete-history wording because earlier outputs and unresolved pre-index
spends can affect the view. A seeded genesis snapshot can be complete without a
retained block anchor; the absence of an anchor is disclosed separately. This is a
loaded snapshot, not a live account monitor.

Changing the visible address membership clears all results and aborts pending work.
Late responses after a scope change or navigation cannot restore old totals. A failed
explicit refresh retains the previous snapshot with a visible stale/error notice;
only a successful refresh replaces it. Older servers, busy servers and bounded-read
failures show unavailable/error states rather than empty or truncated balances.

## Page-open group balance watch

**Start watching group** opts in for the current visible address selection only.
It repeats the same batch POST, never per-address fan-out. Requests start at least
60 seconds apart, never overlap, and pause while the document is hidden. Returning
to the page checks immediately only when the next check is already due. Changing
membership or leaving stops watching; no preference is stored and there is no
background push, service worker, notification permission or wallet connection.
Manual balance refresh is disabled while watching to preserve request bounds.

The in-page timeline keeps at most 20 baseline/change/reset/error observations.
Exact signed nanoERG and raw token differences use `BigInt` and compare only
complete, anchored responses for the same canonical script set. Missing members,
partial history, absent anchors and read failures clear the comparison baseline;
the next complete read starts a new baseline without claiming a change across the
gap. A decreasing tip, a replaced block at the same height, a changed script set,
or contradictory values at an unchanged anchor reset the baseline without a
monetary change claim. A higher tip cannot establish common-branch continuity, so
the UI explicitly describes these as differences between observed snapshots,
not transfers or ownership changes. Existing group results remain visibly stale
if a refresh fails. Stop preserves the local timeline for review; membership
changes clear it. Token-change previews are bounded and disclose omitted rows.
