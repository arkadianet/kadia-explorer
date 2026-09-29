# Address rent exposure

The address page’s **Rent** tab offers an explicit **Load rent exposure** check. Opening a tab or a shared link never scans the address automatically. Category and approaching-horizon controls filter the loaded snapshot locally; the URL preserves those choices, not a claim that the snapshot is still current. Refresh is manual. A failed refresh retains any previous evidence with a stale warning; changing address clears it.

## Compact endpoint

`GET /v1/addresses/{address}/rent?view=exposure` reads one store Reader. The existing route without `view` keeps its original full-box response. Unknown views are rejected. Unseen or invalid addresses return 404; they do not imply zero rent exposure. An older server that ignores `view` cannot supply the required context and the UI reports an unavailable view rather than accepting its legacy rows as anchored evidence.

Each compact item contains `id`, exact raw `value`, declared `creation_height`, serialized `size`, `token_count`, and the existing `rent` projection. It does not hydrate scripts, parse registers, or enrich token metadata. `context` identifies the requested address, canonical `tree_hash`, `indexed_height`, canonical `anchor` (height and block ID), `full_history`, `partial_from`, `scanned_count`, `scan_limit`, and `scan_complete`. Anchor, maturity calculation and unspent rows come from the same Reader. Refresh may produce a different canonical block; this read is not a permanent inclusion guarantee.

Genesis-only stores may have a complete indexed unspent set with no indexed height or canonical header. The UI withholds maturity aggregates when height is unavailable and independently discloses a missing anchor.

## Scope and arithmetic

The scan covers at most 5,000 indexed unspent boxes, in store insertion order, before sorting the scanned subset by maturity then box ID. The presence of another key sets `truncated=true` and `scan_complete=false` **before decoding that extra row**. Exactly 5,000 boxes without a next key is complete. Unscanned boxes may mature sooner than the listed boxes. A partial-history store can omit earlier outputs and cannot resolve all pre-index spends; even a complete scan is only the indexed address view.

The inspector uses positive signed `consensus_fee_nano`, capped by the box’s value, for potential collectible charges. It deliberately does not sum nominal `due_nano`: consensus fee multiplication is signed 32-bit arithmetic and a large serialized box can have a non-positive fee. `claimable_at_tip` alone indicates age and unspent status, so it is not sufficient to call a box collectible. All maturity distances use `context.indexed_height`, never a separately polled global tip. The approaching horizon is expressed in blocks, without a predicted calendar deadline.

All amount arithmetic uses exact integers. Aggregate ERG exposure is separate from the value held in boxes; token quantities and their possible disposition are not included. These are potential claims against boxes, not automatic debts, a prediction that a collector will act, or evidence of address ownership. Spending or recreating a box changes its exposure.

## Resource limits and failure

The compact view admits at most 2 MiB of encoded box rows before decode, charges token entries and register-byte work against 10,000 work units, enforces a four-second read budget, and caps serialized output at 2 MiB. Register work is counted without parsing register JSON. A resource refusal returns HTTP 422 with `rent_exposure_decode_limit`, `rent_exposure_work_limit`, `rent_exposure_response_limit`, or `rent_exposure_deadline`. No partial success or total is returned for budget failures. Paged unspent-box evidence remains a separate route for inspecting large addresses.

The browser caps response reads at 2 MiB and renders at most 100 filtered boxes per local page. It cancels abandoned address requests and ignores late responses. No automatic recurring reads, wallet connection, external pricing, or inferred ownership is involved.

## Validation

API tests cover the legacy response, exact scan cap versus overflow, predecode rejection, register non-hydration, token-work admission, signed non-collectible fees, partial history, rollback replacement, genesis without an anchor, spent exclusion, and unseen versus a known empty address. Frontend units cover exact sums, fee caps and maturity boundaries, response validation, explicit requests, cancellation, timeout and stale retention. Browser cases exercise explicit opt-in, linkable local filters, resource states, bounded rendering and all four appearances at 320 px in light and dark modes.
