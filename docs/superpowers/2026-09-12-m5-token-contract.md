# M5 step 2 cursor token contract

Foundation only: `xp-api::paging`. No handlers, legacy cursor encodings, array responses, frontend, schema versions or stored rows change. The accepted step 1 route matrix remains unchanged. Steps 3–6 are deferred.

## Wire layout

The snapshot parameter is hex-encoded UTF-8 JSON, with exactly these fields:

```json
{"version":1,"route":"global_summaries","filter":"<64 hex characters>","order":"asc","anchor":{"height":1866000,"block_id":"<64 hex characters>"},"cursor_hash":"<64 hex characters>"}
```

`version` is u8 and must equal 1. Route and order are closed snake_case enums. Anchor height is u32. Every hash is exactly 32 bytes represented by 64 hex characters. Issuance uses lowercase hex; decoding accepts either case. Unknown fields (including inside anchor), missing/duplicate fields, unknown enum values, malformed hashes/JSON/hex and unsupported versions are rejected.

Maximum decoded JSON: **1,024 bytes**; maximum input: **2,048 hex characters**, checked before hex allocation and parsing. Tests accept a valid token padded with JSON whitespace to the exact bound and reject both one extra hex character and one extra decoded byte. Fixed-width hash strings replace the surviving implementation's default JSON byte arrays: arrays could make valid tokens exceed the bound. No external dependency is added.

`filter` is BLAKE2b-256 of serde_json serialization of the typed, normalized `Filter` enum. None binds unfiltered routes; Entity binds canonical block id, tree hash or token id; Boxes binds entity and unspent; Register binds number 4–9 and the hash of decoded register value bytes. Binding construction enforces route-specific filter shape and effective order. Token-list sort has separate route keys. Path resolution must occur within the page Reader.

`cursor_hash` is BLAKE2b-256 of the exact existing exclusive cursor string, bounded to 1–85 bytes (u64 decimal, colon, 32-byte hex id). It is a binding, not a cursor parser: step 3 must invoke the unchanged route-specific legacy parser and all existing request/page budgets. Snapshot and cursor must be supplied together to this strict foundation; legacy opt-in dispatch remains step 3. Page size is not a binding. No rank, count, scan budget or membership bound is trusted from a token.

## Validation and Reader lifetime

`with_page` accepts one borrowed Reader. It parses and validates version, route, order, normalized filter and cursor binding before store lookups. It observes that Reader's tip and checks the anchor against that same Reader's canonical header. Current-state routes require exact tip height/id equality; immutable membership routes permit append only if the original anchor remains canonical. Future, absent and replaced anchors fail with `snapshot_changed` (409); malformed tokens and binding mismatches fail with BadRequest (400). Actual store corruption remains a store error.

Only successful validation invokes the page callback. `PageReader::reader()` returns the exact borrowed Reader, and issuance retains that context's anchor and binding. No detached public validator or validated-context constructor exists. A first page without an observed anchor cannot issue a continuation; no next cursor means no next token. Handler integration must read through this context and derive immutable membership bounds from its store header. The callback cannot prevent a programmer from separately opening another Reader: doing so would violate this API contract. No handler integration is claimed here.

Tests hold a Reader while the store tip advances, replace the canonical anchor at the same height, and move the tip below the anchor. Validation, observed tip, page header reads and issuance remain on the pinned snapshot; fresh Readers reject stale current-state anchors and replaced immutable anchors. Fresh immutable continuation survives append with its original anchor.

## Hostile clients

Tokens are **unsigned, not authentication or proof of server issuance**. A client can recompute every hash, choose a different valid exclusive cursor, or construct a token for a valid canonical anchor. Such a request can be accepted as an ordinary bounded page. Therefore it would be false to promise that every forged token is rejected or that the server can prove the client's prior page sequence.

A client gains no authority over store facts or scan budgets. Inconsistent bindings or noncanonical/stale anchors are rejected before page reads. Consistent forgery selects a valid request under the requested route's policy. The adverse outcome is rejection, never fabricated data or token-driven read amplification: input allocation is bounded, validation makes at most two header lookups plus bounded metadata lookups, and no token field controls loops, page limits or scans. The ordinary page budget and immutable membership cap must still be enforced by step 3; end-to-end page identity and handler proofs remain out of scope.

## Tests

`cargo test -p xp-api --lib paging::` includes the accepted matrix test and four token tests. Bidirectional rejection pairs cover global summaries/expanded transactions, asc/desc, newest/holders token sorts, entity ids, box entity and both unspent values, register number and register value hash. Each direction also validates its own minted token and rejects a changed cursor. Parser, exact size boundary, invalid binding construction, anchor forgery, empty-store issuance and pinned-reader tests cover the remaining foundation contract.
