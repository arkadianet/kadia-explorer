# Operator telemetry — M3 step 1 only

`GET /v1/metrics` serves Prometheus text 0.0.4 on the existing API, without a metrics dependency. No schema, row encoding, or existing response contract changes.

## Exposure

Default `[api] metrics_allowlist = []` denies every peer with 404. To expose it, explicitly set operator source CIDRs, for example:

```toml
[api]
metrics_allowlist = ["192.0.2.10/32"] # replace with the actual private operator source
```

The check uses socket `ConnectInfo` only; missing peer information and nonmatching peers fail closed. `X-Forwarded-For`, public rate-limit allowlists, and trusted-proxy settings do not grant access. Do not allowlist a shared public reverse proxy: every client of that proxy would share its admitted socket peer. If exposing through a proxy, first deny this path on its public listener and restrict the operator listener at the network boundary. No automatic loopback exemption. Existing rate limiting and timeouts also apply. Responses use `Cache-Control: no-store`.

## Exact series and fixed bound

All names below have the `explorer_` prefix. All series, including zero counters, are emitted on every successful scrape.

| Name | Labels / series |
| --- | --- |
| `requests_total` | `route`, `status_class` (1xx, 2xx, 3xx, 4xx, 5xx): 155 |
| `request_duration_seconds_bucket` | `route`, `le`: 279 |
| `request_duration_seconds_sum`, `request_duration_seconds_count` | `route`: 31 each |
| `response_bytes_total` | `route`: 31 |
| `request_events_total` | `route`, `event` (rejection, timeout, overload, integrity, error): 155 |
| `blocking_permit_duration_seconds_bucket` | `le`: 9 |
| `blocking_permit_duration_seconds_sum`, `blocking_permit_duration_seconds_count` | none: 1 each |
| `process_start_time_seconds` | none: 1 |
| `active_readers`, `available_read_permits`, `available_history_permits` | none: 1 each |
| `ingest_indexed_height`, `ingest_best_height`, `ingest_lag_blocks` | none: 1 each |
| `ingest_halted`, `ingest_stalled`, `ingest_source_error` | none: 1 each |
| `register_entries`, `register_ceiling` | none: 1 each |

Both cumulative histograms use second boundaries `0.001, 0.01, 0.05, 0.1, 0.5, 1, 5, 30, +Inf`, plus sum and count. The maximum is **31 × (5 + 11 + 1 + 5) + 11 + 12 = 705 series**. Arrays and static whitelists allocate every slot at initialization. Methods, query strings, error text, addresses, ids, and peer IPs never become labels. Additional unknown router templates collapse to `unmatched`; cardinality can only change through source changes.

The 31 route values are:

```text
/v1/metrics
/v1/register-capacity
/v1/status
/v1/blocks
/v1/blocks/{height_or_id}
/v1/blocks/{height_or_id}/txs
/v1/tx-summaries
/v1/blocks/{height_or_id}/tx-summaries
/v1/txs
/v1/txs/{id}
/v1/boxes/{id}
/v1/boxes/{id}/rent
/v1/addresses/{addr}
/v1/addresses/{addr}/boxes
/v1/addresses/{addr}/balance/at
/v1/addresses/{addr}/boxes/at
/v1/addresses/{addr}/txs
/v1/addresses/{addr}/rent
/v1/tokens
/v1/tokens/{id}
/v1/tokens/{id}/holders
/v1/tokens/{id}/boxes
/v1/templates/{hash}
/v1/templates/{hash}/boxes
/v1/registers/{reg}/{value}/boxes
/v1/richlist
/v1/supply
/v1/rent/upcoming
/v1/rent/eligible
/v1/search
unmatched
```

## Semantics and limits

Request accounting wraps CORS, rate limiting, and the HTTP timeout. Route labels come exclusively from Axum `MatchedPath`, checked against the static whitelist. Duration ends when the response is constructed. Bytes are the exact size of the API's materialized JSON/text/empty response bodies, excluding headers and transport framing; they are produced bytes, not confirmed delivery. HEAD responses contribute zero bytes. If streaming responses are introduced, byte accounting must be extended to count frames.

Events are mutually exclusive: rate admission is `rejection`; exhausted read/history permits are `overload`; HTTP 408 and cooperative expansion deadline responses are `timeout`; explicit integrity failures are `integrity`; remaining 4xx/5xx responses are `error`. Thus an ordinary history-unavailable 503 is not mislabeled as reader overload. All still contribute their status-class request counter.

Blocking duration starts at read-permit acquisition, includes blocking-pool queue time, and is recorded by the worker's drop guard on completion/error/panic, even after the HTTP handler times out or is cancelled. Active readers remain elevated while those workers drain. Cumulative buckets and event counters retain evidence of slow reads and admission pressure after active readers return to zero; `/v1/status` alone could not show that. These aggregates cannot timestamp or isolate a past episode without external samples. Worker panics after HTTP timeout contribute their lifetime but cannot produce a second HTTP error response.

The process marker is the Unix timestamp of process telemetry initialization (`Counters` construction at API startup), retained across router clones. Counters reset on process restart. Gauges read atomics, semaphore state, configuration, and the ingest watch snapshot. Unindexed height is 0; flags are 0/1; an unset register ceiling is `+Inf`. Register occupancy is initialized from root metadata during store open and published after successful apply/genesis/rollback commits. Writer serialization protects commit/publication order; scrapes never take that lock or open a store transaction. A scrape can briefly see the preceding committed occupancy during publication. No database scan, background sampler, or per-scrape database read occurs. The separate `/v1/register-capacity` JSON contract remains unchanged.

Scrapes include the metrics route itself; the scrape being rendered enters counters after its response is constructed. Atomic values are individually sampled, not a transactional snapshot. Durations/bytes are process-lifetime u64 counters, subject to eventual numeric overflow; exported integer gauges above 2^53 have normal floating-point scrape precision limits.

**Step 2 durable collection and step 6 load measurement remain BLOCKED.** No collector deployment, scrape pipeline, retention, restart-retrieval evidence, or deployment-class measurements are supplied here. Endpoint-only instrumentation does not satisfy the full M3 operational acceptance.
