# Token pages

Token pages separate three questions: what the mint declared, which locking scripts hold the indexed supply, and which indexed transactions involve the token. Names, media and a mint output address do not establish a verified issuer or beneficial owner.

## Holder concentration

`GET /v1/tokens/{id}/holders` adds `holder_context` alongside the existing paginated response. Its supply and holder count come from the same database reader as the returned balances. Supply is indexed emission minus indexed burns; it is not an estimate of liquid or circulating supply.

The page computes shares from integer strings with `BigInt`, never by summing rounded API percentages. It shows the largest script, the top ten loaded scripts (or fewer while loading), and the number of loaded scripts. With one loaded script, the second metric shows the supply outside that script. The bar uses that top group's exact ratio. Positive shares below 0.01% remain visibly nonzero. Shares are truncated to two decimal places.

Scripts can represent contracts or many people, and one person may use many scripts. These figures are not a complete ownership distribution or a decentralization score. Missing or inconsistent holder context withholds concentration while retaining the holder rows. A changed strict snapshot clears the list and requires restart.

## Mint provenance and media

The page links the mint transaction, mint output, input-derived token ID and mint height. One internal box request reads the mint output address and metadata; no requests are made per holder. The box must match the token's mint box, transaction and token ID before its media can be used.

For EIP-4 picture, audio and video types, the viewer supports a single UTF-8 `Coll[Byte]` URI in R9 and an optional 32-byte SHA-256 in R8. Unsupported register encodings, including the alternative audio/cover tuple, are explicit states. Public HTTPS links and IPFS links through `ipfs.io` are accepted; executable/data URIs, credentials, custom ports and local/private IP literals are rejected. No claim is made that a hostname cannot resolve privately.

External bytes are fetched only after **Load external media**. The preview request omits cookies and referrer, rejects redirects, and stops at 32 MiB or a 30-second network timeout. The external host or IPFS gateway can still observe the visitor's IP address. CORS restrictions, redirects, network errors, unsupported playback formats and oversized files have a visible retry/fallback state. The declared source can be opened separately; that navigation is outside the private preview request.

A valid declared SHA-256 is compared with the downloaded bytes before display. A mismatch withholds the preview. A matching hash does not verify the creator, ownership rights or content safety. Without a usable declared hash, the preview states that it was not verified against a mint hash. Audio/video use native controls and never autoplay. Unloading or leaving the token page aborts pending work and revokes the local blob URL; another token requires fresh consent.

## Token transaction history

The **Transactions** tab loads `GET /v1/tokens/{id}/txs` lazily, with strict consistency, descending order and 20 summary rows per request. Continuation carries both cursor and snapshot. It does not fetch transaction details for every row.

`history_context.scope = indexed_token_touches` means a token occurs in a resolved input or an output. Each transaction appears once, including minting, burning, change and contract movements. Rows show transaction-wide counts and ERG fees, not a token transfer amount or payment classification. The snapshot anchor is displayed.

For a partial index, the start height is displayed and the page explains that earlier transactions and unresolved-input-only token involvement may be absent. An index rebuild or stale derived index returns `503 token_history_preparing`, which remains a preparing/retry state rather than an empty history. An older server's 404 remains unavailable. A strict-snapshot conflict clears the old rows and requires explicit restart.

The history index is auxiliary and rebuildable; it must be ready and tied to the current canonical state before this route serves results. Store and route tests cover the index's apply, rollback, restart and cursor behavior. Frontend tests cover exact share arithmetic, bounded media handling, consent and integrity, history continuation, readiness, compatibility and conflict recovery.
