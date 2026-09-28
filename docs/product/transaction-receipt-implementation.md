# Transaction receipt: first implementation

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
The body limit also applies to ingestion through `RustNode`; it is an operational
guard, not a consensus maximum, and a larger node body would be rejected.

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

Mempool observation, arbitrary-contract rent classification, user ownership,
application decoding, and automated confirmation polling are follow-up work.

## Local validation (2026-09-28)

- Frontend: 138 unit tests and 84 Playwright tests passed; Svelte check reported no
  errors or warnings; lint passed. Desktop, mobile, and dark-mode browser review
  found no layout problems or console errors.
- A clean Windows `npm ci` followed by the production build passed. Home-route
  JavaScript was 49.54 KiB gzipped against the existing 120 KiB budget.
- Rust: workspace check, formatting, and Clippy with warnings denied passed.
  The complete workspace test run passed every target except two five-second
  ingestion timeouts in `fork.rs`. Both passed unchanged in an isolated serial
  rerun using a local temporary directory (2 tests, 0.70 seconds). The initial
  workspace run therefore exited nonzero; it is not recorded as a clean full run.
- Existing Python rent-classifier (14 tests) and capacity-collector (3 tests)
  checks passed.
