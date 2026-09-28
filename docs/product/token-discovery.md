# Token discovery and the shared visual system

Token names are discoverable from the global search and `/tokens`. Ordinary text
opens name results; existing ID, address, block-height and pending-transaction
lookups retain their meanings. The dedicated token form also accepts numeric or
hash-shaped names. Results retain the name as minted and distinguish tokens by
their full ID. Inspect opens a modal with mint provenance and current indexed
supply; Compare presents two identities side by side. Names, age, holder counts,
and metadata never confer an authenticity badge.

The shared shell, home page, page headings, panels, receipts and tracking use the
same forest/lime palette and stronger type hierarchy. Light and dark themes,
keyboard focus, narrow screens and reduced-motion preferences remain supported.
The visual changes use CSS and existing icons; there are no decorative images,
fabricated chain metrics or new runtime dependencies.

## HTTP contract

`GET /v1/tokens/search?q=SigUSD&match=prefix&limit=50&consistency=strict`
returns the existing token DTOs in normalized-name/token-ID ascending order.
`match` is `prefix` (default) or `exact`; page size defaults to 50 and caps at 100.
`next_cursor` is the last token ID, with the next page exclusive of that token.
Strict continuation requires both `cursor` and `snapshot=next_snapshot`.
The token binds the normalized query, match mode, normalization version, cursor,
and canonical tip. Any tip change returns `409 snapshot_changed` because holder
counts and supply are current state. Best-effort paging remains available under
the ordinary paging contract.

The response adds `search` metadata: `query`, `normalized_query`, `match`,
`index_version`, `coverage`, `partial_from`, `indexed_names`, `total_tokens`, and
`unindexed_tokens`. Coverage is partial if chain history is partial or any indexed
token lacks a searchable name. Counts describe the database, not the query's
total match count. An empty ready result means no indexed matches; unavailable
search never masquerades as an empty result.

Normalization version 1 trims and collapses ASCII whitespace and lowercases ASCII
letters. Non-ASCII UTF-8 remains exact: no Unicode case folding, normalization or
confusable-character equivalence is claimed. Queries are at most 512 UTF-8 bytes.
Stored names over 4096 bytes, empty names, and names containing other ASCII
controls are excluded from name discovery and counted as unindexed. Those tokens
remain accessible by ID. Names and descriptions remain untrusted display data.

## Upgrade and operation

The name index and readiness marker live in separate auxiliary redb tables. The
canonical schema remains version 2; TokenRow and UndoRow encodings are unchanged.
Existing databases rebuild names from canonical token rows without a chain resync.
Normal mint, genesis and rollback transactions maintain names atomically.

Startup launches one background worker after the listening socket has been bound.
It clears stale entries and builds in resumable batches of 500 rows with 25 ms
pauses between batches. The store API enforces at most 1000 rows and 8 MiB of
encoded TokenRows per batch before decoding. A single row above that byte limit
fails explicitly. Shutdown waits for any admitted write before completing.

Every read and write checks index version and the canonical anchor (height, full
header ID, allocation counters and token count) in its transaction. A missing tip
header makes this auxiliary anchor unavailable; it does not prevent the database
from opening or invent a zero-ID canonical anchor. A database
changed by an older binary invalidates readiness and rebuilds; applying one new
batch cannot certify a stale index complete. Building can interleave with minting
or rollback, including IDs behind the backfill cursor. Auxiliary state is outside
the canonical table set and does not change rollback fingerprints.

While preparing, name search returns `503 token_search_preparing` with
`Retry-After: 1`; ordinary explorer routes keep working. The worker logs completion
or a failure and stops. A temporarily unavailable canonical anchor is retried with
a one-second cancellation-aware delay so a partial seed can begin indexing. On
other failures, diagnose the reported database/resource issue
and restart to resume; the worker does not spin on errors. A frontend served
against an older API shows search as unavailable while ID pages remain usable.

Search reads at most 100 result rows, one index lookahead, and an optional cursor
row, with a 4 MiB encoded-row budget admitted before decoding. Oversized metadata
returns `422 token_search_bytes` with guidance to reduce the page size or open by
ID. It never returns a silently truncated successful page.

## Validation

Store regressions cover the real SigUSD mint, duplicate normalized names, Unicode
boundaries, bounded reads, legacy database rebuild/restart, interleaved mints and
rollback, and stale anchors after older-binary writes. Router tests cover exact
and prefix search, preserved identity fields, preparing versus empty results,
coverage counts, malformed queries and strict continuation binding. Worker tests
cover readiness, multiple batches, failure isolation and cancellation with an
in-flight write. Frontend/browser validation is recorded in the PR.
