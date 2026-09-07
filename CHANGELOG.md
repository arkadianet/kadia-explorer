# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Plan 3a — ops hardening

- `feat(api)`: address transaction list returns lightweight summaries (`TxSummaryDto`), no
  box resolution — `GET /v1/addresses/{addr}/txs` no longer times out on high-activity
  addresses. `GET /v1/txs/{id}` and the block/global transaction lists are unchanged.
- `feat(api)`: token bucket, CIDR allowlist and proxy-aware client key; per-IP rate limiting
  with trusted proxies and an allowlist (`crates/xp-api/src/limit.rs`) — `429 Too Many
  Requests` with `Retry-After` once a key's bucket is empty, allowlisted keys never limited.
- `fix(api)`: guard the token bucket against non-positive/NaN rates; normalise the
  `client_key` peer address; connect-info wiring, all `X-Forwarded-For` lines, a capped
  bucket map.
- `feat(api)`: bound concurrent blocking store reads via a semaphore
  (`api.max_inflight_reads`) — `503 Service Unavailable` with `Retry-After: 1` when the pool
  is saturated, instead of queuing behind the existing 5 s timeout.
- `fix(api)`: release the `inflight_reads` permit via `Drop`, not a sequential decrement.
- `feat(explorer)`: optional `[api]` config section (`max_inflight_reads`,
  `trusted_proxies`, `[api.rate_limit]` with `per_second`/`burst`/`allowlist`) parsed into
  `xp_api::ApiConfig`; invalid CIDR/IP strings fail at startup like other config errors.
- `feat(frontend)`: address page consumes the summary DTO; status page shows
  `inflight_reads` and `rate_limited_total`.
- Commits: `49c7609`, `5582535`, `dd6c38d`, `cf413fc`, `a9c3400`, `fa969b9`, `b45e192`,
  `29f4122`, `4be4731`.

### Plan 1 — explorer core

- Indexed an Ergo node's blocks (via the Rust/Scala node's JSON REST API) into a standalone,
  local `redb`-backed store (`xp-store`), with its own box/header serialiser (`xp-wire`) so
  the explorer has no dependency on `sigma-rust`'s node types.
- Rollback on reorg: the store keeps enough history to undo and re-apply blocks across a
  fork, bounded by a configurable window; a fork deeper than that halts ingest with a
  distinct exit code (`3`) rather than silently reindexing wrong data.
- Fee accounting (`fees` per block, `fee` per tx) and a box `kind` label (`fee`, `emission`,
  `box`).
- Stall detection and a fallback body source: a node can announce a header and 404 its body,
  so ingest can optionally fetch just that block's body from a second node while everything
  else — headers, `/info`, genesis boxes — comes from the primary alone. Stalls are surfaced
  on `/v1/status` rather than hidden.
- A read-only REST API (`xp-api`, `/v1/...`) over the store: status, blocks, txs, boxes,
  addresses (balance/boxes/txs/rent), richlist, rent (upcoming/eligible), and search.
- A SvelteKit static frontend (`frontend/`) covering every API route, with unit tests over
  its pure modules and a Playwright end-to-end suite against a fixture-backed mock API.
- A parity gate (`scripts/parity.sh`) comparing the explorer's answers against a live node.
- Deploy scripts and a systemd unit for the Nuremberg host, plus a Caddy site config for the
  frontend with immutable-asset caching and an SPA `try_files` fallback.

### Plan 2 — tokens, templates, holders, register search

- `feat(wire)`: register `Coll[Byte]` decoding and EIP-4 token metadata parsing.
- `feat(store)`: schema v2 tables, keys and row codecs for tokens/templates/registers.
- `feat(store)`: index script templates, register values and per-address tx counts.
- `fix(store)`: discriminating `tx_count` test, deterministic undo order, const register
  patterns.
- `feat(store)`: token mint/transfer/burn/holder indexing and full v2 rollback.
- `fix(store)`: cover the token rollback re-key branch; token review follow-ups.
- `feat(store)`: `Reader` queries for tokens, templates and register search.
- `fix(store)`: open pager tables once per query, not once per row.
- `feat(api)`: token, template and register-search endpoints; search kinds; `tx_count`;
  token names on boxes.
- `test(api)`: discriminate supply from emission, cover `token_kind`, drop the unused
  holders `dir`.
- `test(parity)`: `tx_count` from the API and token emission/holders checks.
- `feat(frontend)`: token/template API types, endpoints, amount formatting, search kinds.
- `fix(frontend)`: widen e2e mock fixtures for `TokenDto`/`AddressDto` fields.
- `fix(frontend)`: extract `formatFixedPoint` helper, clarify decimals-0 handling.
- `feat(frontend)`: tokens list and token detail pages.
- `feat(frontend)`: template page, register search, token enrichment, top-tokens card.
- `fix(frontend)`: rent-row identity, shared form control, token id fallback.
- `test(frontend)`: mock tokens/templates/registers and e2e specs.
- `test(frontend)`: bound the scroll helper by wall clock, fix review notes.
- Schema bump to `SCHEMA_VERSION = 2`; no in-place migration — a v1 store must be deleted
  and resynced (see `bin/explorer/README.md`).
