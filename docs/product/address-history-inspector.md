# Historical address snapshots

The address page’s Historical snapshot tab inspects balances and boxes after a selected block. Opening the tab or a shared `?at_height=H&at_block=ID#history` link does not start a scan: the user selects **Inspect snapshot**. Height 0 is the recognized mainnet genesis state before block 1 and has no block ID.

The frontend uses the existing `/balance/at` and `/boxes/at` contracts. A successful scalar balance anchors subsequent box pages to the same height and block ID. If the scalar exceeds its scan or response budget, bounded box pages can still be inspected. Their totals remain explicitly incomplete until `next_cursor` is null. Empty pages with a continuation remain actionable. A valid unseen address may have a complete zero balance; unavailable or partial stores never become zero balances in the UI.

Amounts remain decimal strings and are summed with `BigInt`. Historical token quantities are raw units; today’s names, decimals, supply and fiat prices are not projected into the past. The historical box projection uses creator inclusion height and omits current spent/rent status. Detail links explicitly open the current indexed records.

Normal blocks appended above H preserve the anchor. A 409 or inconsistent returned anchor clears both totals and boxes, requiring an explicit restart at that height. Transient box-page failures retain the already checked snapshot and retry the same cursor. Changing the address or height discards late responses. No scans, retries or polling run automatically in the background.

The view caps retained box pages at 500 boxes and 10,000 token entries. It never splits a box or silently accepts part of an over-limit page. Complete scalar totals remain distinct from a capped box listing. Raw token balances can be filtered by ID; at most 200 matching balance rows are displayed at once. Anchored API links provide access beyond UI limits.

The four appearances retain distinct compositions: Prism uses a balance inspector beside box cards, Atelier a ledger, Aurora spacious grouped cards and Original compact rows. Tests cover opt-in loading, exact large integers, genesis/zero/unavailable states, anchored continuation, empty pages, rollback, transient errors, cancellation and narrow layouts.

## Comparing two blocks

**Compare two blocks** reads complete balances before subtracting the earlier state from the later one. `GET /v1/addresses/{addr}/balance/compare` accepts required `from_height` and `to_height` (`from_height <= to_height`) plus optional `from_block_id` and `to_block_id`. Both anchors and balances come from one immutable Reader. Equal heights are allowed; height 0 is genesis and rejects a block ID. The response includes `address`, `indexed_height`, `complete: true`, and `from`/`to` objects, each containing `at` and `balance` in the existing historical balance format.

One retained-box scan supplies both endpoints. A shared budget allows at most 100,000 candidate boxes, 8 MiB of encoded box/creator data (admitted before decoding), 100,000 token additions across both balances, four seconds and a 2 MiB combined response. Genesis recognition is included in byte admission. Limits return 422 with no endpoint balances; partial/unavailable history returns 503. A stale pinned anchor returns 409, including when a rollback moves the tip below it. There is no paged or loaded-subtotal comparison fallback.

The frontend uses `BigInt` for ERG, raw token and unspent box-count differences. A token present only at one endpoint has zero at the other **only after both complete states are validated**. The result is a balance difference, not transaction volume, minted/burned quantities, or counts of boxes created and spent between the heights. Assets may leave and return between endpoints. Historical names, decimals and prices remain absent.

Shared `?from_height=A&to_height=B&from_block=ID&to_block=ID#history` links pin both returned anchors and never load automatically. Genesis pins omit its block ID. A new read clears the previous comparison; 409, incomplete responses and budget failures leave no stale or partial difference visible. Address/selector changes and unmount discard late responses. The view validates up to 10,000 combined token entries, renders at most 200 matches and supports token-ID filtering. Tests cover exact integer subtraction, entering/leaving tokens, both pins, genesis, reorg restart, incomplete evidence, shared work limits and all four appearances at 320 px.
