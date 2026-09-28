# Transaction receipts and live tracking

The confirmed transaction page now leads with an address-specific receipt, all-address
balance changes, and inspectable evidence. Raw input/output boxes remain on the page.
It uses exact integer amounts and distinguishes transaction fees from net changes.
Missing input boxes suppress net receipts rather than making output totals look like
amounts received. Token quantities use token IDs; mint/burn deltas are separate from
conservation checks. Unknown decimals display raw units.

## Evidence and limits

`GET /v1/txs/{id}` adds `block_id`, `indexed_height` and `confirmations` from the
same store snapshot. Lists retain their existing wire shape. The UI labels counts
as the last checked snapshot and can refresh the transaction. It does not infer
pending, dropped or reorged status from a missing mempool observation.

`GET /v1/txs/{id}/evidence` reads proofs from the configured ingest source via
`Extension<Arc<dyn BlockSource>>`. The binary wires the source into the router;
embedders using `xp_api::router` without that extension keep working, but evidence
is unavailable. This route uses two separate evidence permits, a five-second
network timeout, the existing transaction-expansion budget, and a 16 MiB
decompressed block-body limit. The source's canonical header is checked before
and after fetching, the local header is checked again, and ordered transaction
inputs/outputs are matched to the store. It reports `trusted_node_response`;
proof commitments and consensus are not independently replayed. Source failure
does not break the underlying transaction page. No on-disk migration is needed.
The body limit applies to the optional evidence lookup through
`full_block_json_bounded`; ordinary block ingestion retains its existing behavior.

Automatic rent recognition supports confirmed, mature standard P2PK inputs with
an explicitly empty proof and a canonical nonnegative Short selector pointing to
an existing output. A P2PK script cannot pass ordinary signature validation with
an empty proof, which distinguishes this from arbitrary scripts that can. This
supports both fully consumed boxes and recreated outputs without assuming today's
fee factor applied throughout history. Other script types remain unclassified
until historical validation or a supported decoder is available. Signed old boxes
are not called rent, and unavailable proof evidence is not treated as empty.

Emission and fee collection use server-provided contract kinds. Unsupported
contract activity and incomplete input coverage remain explicit. The homepage
uses the same conservative labels without performing per-row node lookups.
Bot operation, miner identity, payment intent and token authenticity are not inferred.

## Search and local development

Address candidates are validated server-side using the existing Ergo address
parser (network, checksum and script). Long P2S addresses, short scripts, and
64-character nonhex candidates are not rejected by a prefix/length heuristic.
Search errors distinguish malformed addresses from valid but unindexed addresses.

The npm lockfile now includes the two Windows x64 native optional packages used
by the pinned bundler/CSS toolchain. Prettier accepts checkout line endings so
Windows CRLF checkout does not fail every unchanged file's formatting check.

Fixtures under `tests/fixtures/receipts` record the mainnet storage-rent transaction
that motivated the receipt. Unit, route and browser regressions cover its exact
effects, signed old boxes, missing proofs, forks, partial input coverage, large
integer values, mint/burn, duplicate names, and mobile layout.

Live transaction tracking is described below. Arbitrary-contract rent classification,
user ownership, application decoding, and a network-wide mempool archive remain
follow-up work.

## Pending-to-confirmed tracking

`GET /v1/txs/{id}/status` returns a small, uncached status response. Confirmation
height, block ID and count come from one store Reader; the expanded transaction is
still served separately. The browser polls every five seconds while visible,
pauses while hidden, resumes immediately, and discards responses from disposed
routes. Automatic polling does not repeatedly expand inputs/outputs or fetch rent
proofs. Explicit refresh reloads mutable box details; their snapshot is distinct
from live confirmation counts.

For an unindexed transaction, the API makes a read-only lookup against the primary
node's `/transactions/unconfirmed/byTransactionId/{id}` endpoint. Fallback block
sources are never used as fallback mempools. A transaction-specific 404 response
means absent; proxy/HTML/ambiguous 404, unsupported endpoints and outages mean
unavailable. Returned transaction IDs and input references are validated. A fresh
store read after node I/O gives any newly indexed inclusion precedence.

States are `confirmed`, `pending`, `not_observed`, `no_longer_observed`,
`unavailable`, and `conflicted`. Disappearance never establishes rejection.
Conflicts are positive findings from canonical indexed spending of remembered
inputs, not exhaustive network-wide detection. A previous inclusion that is no
longer in the local index remains visible without claiming a network reorg when
the index may simply be behind. Pending summaries show counts and the standard
fee-output total; they do not fabricate receipt balances.

Observation history is process-local and records only requested transaction IDs.
It holds at most 1,024 entries, expires after 60 minutes without a request, may be
evicted earlier under capacity pressure, and resets on process restart. Continuous
requests can retain an entry longer. `retention_seconds` describes idle expiry,
not a guaranteed archival period. First observed is this explorer's observation,
not the transaction's broadcast time. At most 512 complete input references are
remembered per transaction, including transactions first seen after confirmation;
larger confirmed transactions retain confirmation status without remembered input
coverage. Pending responses beyond the parser limits are unavailable, not partially
reported. A missing conflict is never proof that none exists.

Node lookups have two independent admission slots, a two-second timeout and a
2 MiB decompressed-body bound. Successful observations are shared for five seconds;
errors are shared for two. Concurrent requests for the same ID coalesce. The
per-entry lock and admission permit are released on cancellation; parsing keeps
its permit until the blocking worker finishes. Existing API rate/reader limits
still apply. No mempool database migration or background archival process is added.

The existing confirmed detail route keeps its response contract. Valid unknown IDs
can be followed on `/tx/{id}` without an initial 404 dead end. Search resolves a
known pending transaction and offers an explicit tracking link for an unknown
64-hex ID, without assuming every unknown hash identifies a transaction. Older
servers without the status route retain their original confirmed receipt.

Protocol references: [Ergo node OpenAPI](https://github.com/ergoplatform/ergo/blob/master/src/main/resources/api/openapi.yaml),
[transaction routes](https://github.com/ergoplatform/ergo/blob/master/src/main/scala/org/ergoplatform/http/api/TransactionsApiRoute.scala).
Kadia's public node absence response was also checked on 2026-09-28.

## Selectable appearance and box flow (2026-09-28)

The header's Style control offers Original (forest/lime), Aurora (emerald glass),
Atelier (paper/copper), and Prism (blue spatial surfaces). Prism is the default.
The browser saves appearance under `xp-appearance`, separately from the existing
`xp-theme` light/dark preference. Both are validated and applied before first
paint; unavailable storage still allows changes for the current session. The
native appearance dialog supports radio-arrow navigation, contained Tab focus,
Escape, and focus restoration. Display fonts are self-hosted, with their SIL OFL
licenses alongside the files; table data and identifiers retain their legible
shared typography. Reduced-motion preferences continue to apply.

Box flow is an optional transaction tab available in every appearance. It uses the
same resolved boxes and address effects as Receipt, without another request or a
second balance calculation. Selecting a non-fee box changes the shared address
perspective. The diagram shows transaction topology, never attributed paths or
value-weighted lines; data inputs are explicitly excluded because they are read,
not spent. Box amounts remain visible with incomplete inputs, but all exact net
changes are withheld until every input is resolved. Each side initially shows
four boxes, expands four at a time, and can collapse independently. The regular
Receipt remains the default view.

## Earlier local validation (2026-09-28)

- Frontend: 148 unit tests and 94 Playwright tests passed; Svelte check reported no
  errors or warnings; lint passed. Desktop, mobile, and dark-mode browser review
  found no layout problems or console errors.
- A clean Windows `npm ci` followed by the production build passed. Linux CI's
  home-route JavaScript budget check reported 60.33 KiB gzipped against 120 KiB.
- Rust: workspace check, formatting, Clippy with warnings denied, and the complete
  workspace test run passed. Tests used a local temporary directory and four test
  threads. Existing ignored tests that require external services or snapshots were
  not run. The suite includes nine status-route tests and 31 source tests covering
  confirmation races, rollback/re-inclusion, canonical conflicts, same-ID request
  coalescing, unavailable sources, and decompressed response limits.
- Existing Python rent-classifier (14 tests) and capacity-collector (3 tests)
  checks passed.
