# Historical address snapshots

The address page’s Historical snapshot tab inspects balances and boxes after a selected block. Opening the tab or a shared `?at_height=H&at_block=ID#history` link does not start a scan: the user selects **Inspect snapshot**. Height 0 is the recognized mainnet genesis state before block 1 and has no block ID.

The frontend uses the existing `/balance/at` and `/boxes/at` contracts. A successful scalar balance anchors subsequent box pages to the same height and block ID. If the scalar exceeds its scan or response budget, bounded box pages can still be inspected. Their totals remain explicitly incomplete until `next_cursor` is null. Empty pages with a continuation remain actionable. A valid unseen address may have a complete zero balance; unavailable or partial stores never become zero balances in the UI.

Amounts remain decimal strings and are summed with `BigInt`. Historical token quantities are raw units; today’s names, decimals, supply and fiat prices are not projected into the past. The historical box projection uses creator inclusion height and omits current spent/rent status. Detail links explicitly open the current indexed records.

Normal blocks appended above H preserve the anchor. A 409 or inconsistent returned anchor clears both totals and boxes, requiring an explicit restart at that height. Transient box-page failures retain the already checked snapshot and retry the same cursor. Changing the address or height discards late responses. No scans, retries or polling run automatically in the background.

The view caps retained box pages at 500 boxes and 10,000 token entries. It never splits a box or silently accepts part of an over-limit page. Complete scalar totals remain distinct from a capped box listing. Raw token balances can be filtered by ID; at most 200 matching balance rows are displayed at once. Anchored API links provide access beyond UI limits.

The four appearances retain distinct compositions: Prism uses a balance inspector beside box cards, Atelier a ledger, Aurora spacious grouped cards and Original compact rows. Tests cover opt-in loading, exact large integers, genesis/zero/unavailable states, anchored continuation, empty pages, rollback, transient errors, cancellation and narrow layouts.
