# Ops hardening (Plan 3a) — design

Sub-plan 3a of the explorer's phase 3. Scope: make the public API safe to expose
under load without changing the store. Three changes: a lightweight
address-transaction list, per-IP rate limiting with a trusted allowlist, and
bounded blocking reads. No schema change; deployment is a binary swap plus an
optional config section.

Parent spec: `2026-09-05-ergo-explorer-design.md` §8 ("Limits: `limit ≤ 500`,
request timeout 5 s, per-IP token bucket") and §10 (`rate_limits` config).

## 1. Problem

Measured on the local full store with the Plan 1 binary (cold cache):

| Address (richlist top 5) | `/txs?limit=50` | `/txs?limit=500` | boxes resolved |
|---|---|---|---|
| exchange hot wallet | 0.31 s | 408 timeout | > 10 000 |
| exchange hot wallet | 0.47 s | 408 timeout | > 19 000 |
| miner payout | 0.06 s | 1.09 s | 3 726 |
| contract | 0.13 s | 0.92 s | 2 000 |

Warm, the 500-item page takes 20 ms. The cost is one or two point reads per
resolved input/output box (box row, tree row) on an IOPS-bound disk, and
`GET /v1/addresses/{addr}/txs` resolves every box of every transaction even
though the address page renders only id, height, timestamp and fee.

The API also has no per-client limit and no bound on concurrent blocking
store reads: a burst of slow requests occupies the blocking pool and every
request waits behind it until the 5 s ceiling.

## 2. Address transaction summaries

`GET /v1/addresses/{addr}/txs?cursor&limit&dir` returns
`PageDto<TxSummaryDto>` instead of `PageDto<TxDto>`:

```
TxSummaryDto {
  id: hex32,
  height: u32,
  index: u16,            // position in block
  timestamp: u64,
  size: u32,
  fee: string,           // nanoERG, decimal string
  input_count: u16,
  data_input_count: u16,
  output_count: u16,
}
```

Every field comes from `TxRow`; the handler performs no box or tree reads
beyond the `TREE_TXS` range and one `TXS` get per item. `GET /v1/txs/{id}`
and the block/global transaction lists keep the full `TxDto`.

Frontend: `TxSummaryDto` type and `api.addressTxs` return type change; the
address page table is unchanged in appearance. The e2e mock emits the summary
shape for the address route. The parity gate reads `tx_count` from the address
DTO and no longer walks this route, so it needs no change.

Rejected: capping `limit` for this route at 100 (still 2–4 s cold on exchange
addresses); storing per-address deltas at apply time (schema v3, not needed
by any page).

## 3. Rate limiting

A tower `Layer` in `xp-api` (`crates/xp-api/src/limit.rs`), no new
dependencies.

**Client key.** If the connection's peer address is in `trusted_proxies`, the
key is the last address in `X-Forwarded-For` that is *not* itself a trusted
proxy (right-to-left walk); otherwise the key is the peer address. Caddy on
`127.0.0.1` is the only trusted proxy in production. Missing or unparsable
header behind a trusted proxy ⇒ key is the peer address (never bypass).

**Bucket.** Token bucket per key: capacity `burst`, refill `per_second`
tokens/s, one token per request. State in a `Mutex<HashMap<IpAddr, Bucket>>`
(requests are microseconds; contention is not a concern at the target
rates). A sweep every 60 s removes buckets that have been full for over 60 s.

**Allowlist.** `allowlist` entries are IPs or CIDRs (v4 and v6); a matching
key bypasses the bucket entirely and is not counted.

**Response on limit.** `429 Too Many Requests`, RFC 7807 problem JSON like
every other error, `Retry-After` header with the whole seconds until one
token is available (minimum 1). `rate_limited_total` counter increments.

**Defaults** (used when the section is absent): `per_second = 10`,
`burst = 30`, empty allowlist, `trusted_proxies = ["127.0.0.1", "::1"]`.
`per_second = 0` disables limiting.

## 4. Bounded blocking reads

`blocking()` in `xp-api` acquires a permit from a `tokio::sync::Semaphore`
(`max_inflight_reads`, default 32) with `try_acquire`. No permit ⇒
`503 Service Unavailable` problem JSON with `Retry-After: 1`, without
touching the blocking pool. The existing 5 s `TimeoutLayer` stays and still
covers the read itself. `inflight_reads` is exposed on `/v1/status`.

Rationale: a saturated blocking pool turns every request into a 5 s wait;
failing fast keeps cheap requests cheap while the disk is busy.

## 5. Configuration

New optional `[api]` section in `explorer.toml`:

```toml
[api]
max_inflight_reads = 32
trusted_proxies = ["127.0.0.1", "::1"]

[api.rate_limit]
per_second = 10
burst = 30
allowlist = ["203.0.113.7", "2001:db8::/32"]
```

Parsed into `ApiConfig` in `bin/explorer` and passed to `xp-api::router` as
a plain struct (`xp-api` does not read TOML). Invalid CIDR or IP strings fail
at startup with a clear message (exit code 1, like other config errors).

## 6. Status

`StatusDto` gains `inflight_reads: u32` and `rate_limited_total: u64`. Both
are process-lifetime counters (no persistence). The frontend status page shows
them in the existing facts list.

## 7. Errors

All new failures are problem JSON via the existing `ApiError` mapping:
`ApiError::TooManyRequests { retry_after: u32 }` → 429,
`ApiError::Overloaded` → 503. Neither is logged per request (a flood must not
also flood the log); the counters are the signal.

## 8. Testing

- Unit: token bucket (refill maths, burst, zero rate disables), CIDR/IP
  allowlist matching (v4, v6, mapped v4), client-key derivation for all four
  header/peer combinations.
- Route tests (`crates/xp-api/tests/routes.rs`): 429 after `burst` requests
  from one key with `Retry-After`; allowlisted key never limited; forwarded
  key honoured only from a trusted peer; 503 when the semaphore is exhausted
  (permit count 1, one request parked on a blocked reader); summary DTO
  fields for a fixture address equal the corresponding `TxRow` fields;
  `/v1/status` counters.
- Frontend: `npm run check` covers the type change; the e2e address spec
  passes against the summary-shaped mock.
- Manual: after deployment, `ab`-style burst from one IP returns 429 and the
  fleet arms (allowlisted) do not.

## 9. Deployment

Build, copy the binary to Nuremberg, add the `[api]` section with the fleet
arms and the operator's IP in `allowlist`, restart the service (the store is
untouched, the resync continues from its cursor), redeploy the frontend.
Caddy already forwards `X-Forwarded-For`.

## 10. Out of scope

WebSocket, analytics, mempool, testnet (sub-plans 3b–3e). Response caching.
Per-route limits. API keys.
