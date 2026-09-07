# Ops Hardening (Plan 3a) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the public explorer API safe under load: a cheap address-transaction list, per-IP rate limiting with a trusted allowlist, and bounded blocking store reads.

**Architecture:** All changes live in `xp-api` (a new `limit.rs` tower layer, a `TxSummaryDto`, a semaphore-gated `blocking()`), a new `ApiConfig` struct that `bin/explorer` fills from an optional `[api]` TOML section, and a small frontend type change. No store change, no new dependencies.

**Tech Stack:** Rust 2021, axum 0.8, tower/tower-http, tokio (sync), serde; SvelteKit 2 / Svelte 5 / TypeScript, Vitest, Playwright.

**Spec:** `docs/superpowers/specs/2026-09-07-ops-hardening-design.md`

## Global Constraints

- No new crate or npm dependencies (CIDR matching and the token bucket are hand-written).
- Rate-limit defaults: `per_second = 10`, `burst = 30`, empty allowlist, `trusted_proxies = ["127.0.0.1", "::1"]`; `per_second = 0` disables limiting. Limited requests answer `429` with RFC 7807 problem JSON and `Retry-After` in whole seconds (minimum 1).
- Bounded reads: `max_inflight_reads` default `32`; when exhausted answer `503` problem JSON with `Retry-After: 1` without touching the blocking pool.
- Client key: peer address unless the peer is a trusted proxy, in which case the right-most `X-Forwarded-For` entry that is not a trusted proxy; missing/unparsable header ⇒ peer address.
- `StatusDto` gains `inflight_reads: u32` and `rate_limited_total: u64` (process-lifetime counters).
- `GET /v1/addresses/{addr}/txs` returns `PageDto<TxSummaryDto>`; every other transaction endpoint keeps `TxDto`.
- Amounts on the wire are decimal strings; ids hex; `limit ≤ 500`.
- Every commit: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` green; frontend commits also `npm test && npm run check && npm run lint && npm run build` green, e2e `npx playwright test --workers=4` green.
- Commit with pathspec (`git commit -m … -- <paths>`), `git add` new files first. Trailer: `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_01Jegbu3j98VqxmXFrcFCFdS`.

---

## File structure

| File | Responsibility |
|---|---|
| `crates/xp-api/src/dto.rs` | `TxSummaryDto` + `tx_summary_dto()`; `StatusDto` counters |
| `crates/xp-api/src/handlers/addresses.rs` | `txs` handler switches to summaries |
| `crates/xp-api/src/limit.rs` (new) | `TokenBucket`, `Allowlist`/`Cidr`, `client_key()`, `RateLimit` layer + service, `Counters` |
| `crates/xp-api/src/error.rs` | `TooManyRequests { retry_after }`, `Overloaded` |
| `crates/xp-api/src/lib.rs` | `ApiConfig`, `AppState { read_permits, counters }`, semaphore in `blocking()`, `router(state, cfg)` |
| `crates/xp-api/src/handlers/status.rs` | counters on `/v1/status` |
| `crates/xp-api/tests/routes.rs` | route tests |
| `bin/explorer/src/config.rs`, `main.rs` | `[api]` section, validation, `into_make_service_with_connect_info` |
| `frontend/src/lib/api/types.ts`, `endpoints.ts` | `TxSummaryDto`, `addressTxs` |
| `frontend/src/routes/status/+page.svelte` | two new facts |
| `frontend/tests/e2e/mock/handlers.ts`, `fixtures.ts` | summary shape for the address route |
| `bin/explorer/README.md`, `CHANGELOG.md` | config docs, changelog |

---

### Task 1: Address transaction summaries

**Files:**
- Modify: `crates/xp-api/src/dto.rs` (near `tx_dto`, ~line 495)
- Modify: `crates/xp-api/src/handlers/addresses.rs:75-100`
- Test: `crates/xp-api/tests/routes.rs`

**Interfaces:**
- Consumes: `xp_store::rows::TxRow { height, index, gidx, first_out_gidx, timestamp, size, fee, inputs, data_inputs, output_count }`, `Reader::tree_txs(&tree, cursor, limit, dir) -> Page<(Hash32, TxRow)>`.
- Produces: `pub struct TxSummaryDto`, `pub fn tx_summary_dto(id: &Hash32, row: &TxRow) -> TxSummaryDto`.

- [ ] **Step 1: Write the failing route test**

Append to `crates/xp-api/tests/routes.rs` (reuse `app()`, `get()`, and the existing `coinbase_address()` helper which returns an address present in the fixtures):

```rust
#[tokio::test]
async fn address_txs_are_summaries_without_resolved_boxes() {
    let (_d, app) = app();
    let addr = coinbase_address();
    let (st, v) = get(&app, &format!("/v1/addresses/{addr}/txs?limit=5")).await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let first = &items[0];
    for key in ["id", "height", "index", "timestamp", "size", "fee",
                "input_count", "data_input_count", "output_count"] {
        assert!(first.get(key).is_some(), "missing {key}");
    }
    assert!(first.get("inputs").is_none(), "summaries must not resolve inputs");
    assert!(first.get("outputs").is_none(), "summaries must not resolve outputs");
    assert!(first["fee"].is_string());
    // Cross-check one row against the full tx endpoint.
    let id = first["id"].as_str().unwrap();
    let (_, full) = get(&app, &format!("/v1/txs/{id}")).await;
    assert_eq!(full["height"], first["height"]);
    assert_eq!(full["fee"], first["fee"]);
    assert_eq!(full["inputs"].as_array().unwrap().len() as u64, first["input_count"].as_u64().unwrap());
    assert_eq!(full["outputs"].as_array().unwrap().len() as u64, first["output_count"].as_u64().unwrap());
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p xp-api --test routes address_txs_are_summaries -- --nocapture`
Expected: FAIL on `summaries must not resolve inputs`.

- [ ] **Step 3: Add the DTO**

In `crates/xp-api/src/dto.rs`, next to `TxDto`:

```rust
/// One row of an address's transaction list. Built from `TxRow` alone: no box or tree
/// reads, so hot addresses page in milliseconds (spec §2).
#[derive(Serialize)]
pub struct TxSummaryDto {
    pub id: String,
    pub height: u32,
    pub index: u16,
    pub timestamp: u64,
    pub size: u32,
    pub fee: String,
    pub input_count: u16,
    pub data_input_count: u16,
    pub output_count: u16,
}

pub fn tx_summary_dto(id: &Hash32, row: &TxRow) -> TxSummaryDto {
    TxSummaryDto {
        id: hex32(id),
        height: row.height,
        index: row.index,
        timestamp: row.timestamp,
        size: row.size,
        fee: row.fee.to_string(),
        input_count: row.inputs.len() as u16,
        data_input_count: row.data_inputs.len() as u16,
        output_count: row.output_count,
    }
}
```

(`Hash32`, `TxRow`, `hex32` are already imported in `dto.rs`; use `u16::try_from(..).unwrap_or(u16::MAX)` instead of `as u16` if clippy objects.)

- [ ] **Step 4: Switch the handler**

In `crates/xp-api/src/handlers/addresses.rs` replace the body of `txs` so it returns `Json<PageDto<TxSummaryDto>>`:

```rust
pub async fn txs(
    State(state): State<AppState>,
    Path(addr): Path<String>,
    Query(p): Query<ListParams>,
) -> Result<Json<PageDto<TxSummaryDto>>, ApiError> {
    let limit = parse_limit(p.limit.as_deref())?;
    let cursor = parse_u64_cursor(p.cursor.as_deref())?;
    let dir = parse_dir(p.dir.as_deref())?;
    let page = blocking(&state, move |rd| {
        let tree = tree_of(rd, &addr)?;
        let page = rd.tree_txs(&tree, cursor, limit, dir)?;
        Ok(PageDto {
            items: page.items.iter().map(|(id, row)| tx_summary_dto(id, row)).collect(),
            next_cursor: page.next_cursor.map(|c| c.to_string()),
        })
    })
    .await?;
    Ok(Json(page))
}
```

Update the `use crate::dto::{…}` list (add `tx_summary_dto, TxSummaryDto`; drop `tx_dto`, `TxDto`, `enrich_txs` if now unused in this file).

- [ ] **Step 5: Run tests, clippy, fmt**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green (an existing test that asserted `inputs` on the address list, if any, must be updated to the summary shape).

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(api): address transaction list returns summaries, no box resolution" -- crates/xp-api
```

---

### Task 2: Rate-limit primitives

**Files:**
- Create: `crates/xp-api/src/limit.rs`
- Modify: `crates/xp-api/src/lib.rs` (add `pub mod limit;`)

**Interfaces:**
- Produces:
  - `pub struct TokenBucket { tokens: f64, last: Instant }` with `pub fn new(burst: u32) -> Self`, `pub fn try_take(&mut self, now: Instant, per_second: f64, burst: u32) -> Result<(), Duration>` (Err = wait until one token).
  - `pub struct Cidr { addr: IpAddr, prefix: u8 }` with `impl FromStr` (accepts `"1.2.3.4"`, `"1.2.3.0/24"`, `"2001:db8::/32"`) and `pub fn contains(&self, ip: IpAddr) -> bool` (IPv4-mapped IPv6 addresses are compared as IPv4).
  - `pub struct Allowlist(Vec<Cidr>)` with `pub fn parse(items: &[String]) -> Result<Allowlist, String>` and `pub fn contains(&self, ip: IpAddr) -> bool`.
  - `pub fn client_key(peer: IpAddr, forwarded_for: Option<&str>, trusted: &Allowlist) -> IpAddr`.

- [ ] **Step 1: Write failing unit tests**

Create `crates/xp-api/src/limit.rs` containing only a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::time::{Duration, Instant};

    fn ip(s: &str) -> IpAddr { s.parse().unwrap() }

    #[test]
    fn bucket_allows_burst_then_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(3);
        for _ in 0..3 { assert!(b.try_take(t0, 1.0, 3).is_ok()); }
        let wait = b.try_take(t0, 1.0, 3).unwrap_err();
        assert!(wait > Duration::from_millis(900) && wait <= Duration::from_secs(1));
        assert!(b.try_take(t0 + Duration::from_secs(1), 1.0, 3).is_ok());
        // Refill never exceeds burst.
        assert!(b.try_take(t0 + Duration::from_secs(100), 1.0, 3).is_ok());
        assert!(b.try_take(t0 + Duration::from_secs(100), 1.0, 3).is_ok());
        assert!(b.try_take(t0 + Duration::from_secs(100), 1.0, 3).is_ok());
        assert!(b.try_take(t0 + Duration::from_secs(100), 1.0, 3).is_err());
    }

    #[test]
    fn cidr_parses_and_matches_v4_v6_and_mapped() {
        let c: Cidr = "10.0.0.0/8".parse().unwrap();
        assert!(c.contains(ip("10.255.1.2")));
        assert!(!c.contains(ip("11.0.0.1")));
        assert!(c.contains(ip("::ffff:10.1.1.1")));
        let h: Cidr = "203.0.113.7".parse().unwrap();
        assert!(h.contains(ip("203.0.113.7")));
        assert!(!h.contains(ip("203.0.113.8")));
        let v6: Cidr = "2001:db8::/32".parse().unwrap();
        assert!(v6.contains(ip("2001:db8:1::5")));
        assert!(!v6.contains(ip("2001:db9::1")));
        assert!("1.2.3.4/33".parse::<Cidr>().is_err());
        assert!("nope".parse::<Cidr>().is_err());
    }

    #[test]
    fn client_key_uses_forwarded_only_behind_trusted_proxy() {
        let trusted = Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).unwrap();
        // Untrusted peer: header ignored.
        assert_eq!(client_key(ip("198.51.100.9"), Some("1.1.1.1"), &trusted), ip("198.51.100.9"));
        // Trusted peer: right-most non-trusted hop.
        assert_eq!(client_key(ip("127.0.0.1"), Some("1.1.1.1, 127.0.0.1"), &trusted), ip("1.1.1.1"));
        assert_eq!(client_key(ip("127.0.0.1"), Some("9.9.9.9, 1.1.1.1"), &trusted), ip("1.1.1.1"));
        // Trusted peer, garbage/missing header: peer address, never bypass.
        assert_eq!(client_key(ip("127.0.0.1"), Some("not an ip"), &trusted), ip("127.0.0.1"));
        assert_eq!(client_key(ip("127.0.0.1"), None, &trusted), ip("127.0.0.1"));
    }
}
```

- [ ] **Step 2: Run and confirm compile failure**

Run: `cargo test -p xp-api --lib limit`
Expected: FAIL, `TokenBucket`/`Cidr`/`Allowlist`/`client_key` not found.

- [ ] **Step 3: Implement the primitives**

Above the tests in `crates/xp-api/src/limit.rs`:

```rust
//! Per-client rate limiting (spec §3): token buckets keyed by client IP, a CIDR allowlist,
//! and the proxy-aware client key. The tower layer that uses them is added in Task 3.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct TokenBucket {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(burst: u32) -> TokenBucket {
        TokenBucket { tokens: burst as f64, last: Instant::now() }
    }

    /// Refill up to `burst` at `per_second`, then take one token. `Err(wait)` is the time
    /// until one token is available.
    pub fn try_take(&mut self, now: Instant, per_second: f64, burst: u32) -> Result<(), Duration> {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * per_second).min(burst as f64);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - self.tokens;
            Err(Duration::from_secs_f64(missing / per_second))
        }
    }

    /// True when the bucket has been full for `idle` — safe to drop from the map.
    pub fn is_idle(&self, now: Instant, per_second: f64, burst: u32, idle: Duration) -> bool {
        let full_since = self.last + Duration::from_secs_f64((burst as f64 - self.tokens).max(0.0) / per_second.max(1e-9));
        now.saturating_duration_since(full_since) >= idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

fn unmap(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

impl FromStr for Cidr {
    type Err = String;
    fn from_str(s: &str) -> Result<Cidr, String> {
        let (ip, prefix) = match s.split_once('/') {
            Some((ip, p)) => (ip, Some(p)),
            None => (s, None),
        };
        let addr = unmap(ip.parse::<IpAddr>().map_err(|e| format!("{s}: {e}"))?);
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.parse::<u8>().map_err(|e| format!("{s}: {e}"))?,
            None => max,
        };
        if prefix > max {
            return Err(format!("{s}: prefix {prefix} exceeds {max}"));
        }
        Ok(Cidr { addr, prefix })
    }
}

impl Cidr {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, unmap(ip)) {
            (IpAddr::V4(a), IpAddr::V4(b)) => {
                let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - self.prefix) };
                (u32::from(a) & mask) == (u32::from(b) & mask)
            }
            (IpAddr::V6(a), IpAddr::V6(b)) => {
                let mask = if self.prefix == 0 { 0 } else { u128::MAX << (128 - self.prefix) };
                (u128::from(a) & mask) == (u128::from(b) & mask)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Allowlist(Vec<Cidr>);

impl Allowlist {
    pub fn parse(items: &[String]) -> Result<Allowlist, String> {
        items.iter().map(|s| s.parse()).collect::<Result<Vec<_>, _>>().map(Allowlist)
    }
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|c| c.contains(ip))
    }
}

/// Spec §3 "Client key". Walks `X-Forwarded-For` right to left past trusted proxies.
pub fn client_key(peer: IpAddr, forwarded_for: Option<&str>, trusted: &Allowlist) -> IpAddr {
    if !trusted.contains(peer) {
        return peer;
    }
    let Some(header) = forwarded_for else { return peer };
    for hop in header.rsplit(',') {
        match hop.trim().parse::<IpAddr>() {
            Ok(ip) if trusted.contains(ip) => continue,
            Ok(ip) => return unmap(ip),
            Err(_) => return peer,
        }
    }
    peer
}

#[allow(dead_code)]
fn _assert_types(_: Ipv4Addr, _: Ipv6Addr) {}
```

Remove the `_assert_types` helper if the `Ipv4Addr`/`Ipv6Addr` imports end up unused (delete the imports instead). Add `pub mod limit;` to `crates/xp-api/src/lib.rs`.

- [ ] **Step 4: Run tests, clippy, fmt**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p xp-api --lib limit`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/xp-api/src/limit.rs
git commit -m "feat(api): token bucket, CIDR allowlist and proxy-aware client key" -- crates/xp-api
```

---

### Task 3: Rate-limit layer, 429, config struct, counters

**Files:**
- Modify: `crates/xp-api/src/limit.rs` (append layer + service)
- Modify: `crates/xp-api/src/error.rs`
- Modify: `crates/xp-api/src/lib.rs` (`ApiConfig`, `Counters`, `router(state, cfg)`)
- Modify: `crates/xp-api/src/handlers/status.rs`, `crates/xp-api/src/dto.rs` (`StatusDto`)
- Test: `crates/xp-api/tests/routes.rs`

**Interfaces:**
- Consumes: Task 2 primitives.
- Produces:
  - `pub struct ApiConfig { pub per_second: u32, pub burst: u32, pub allowlist: Allowlist, pub trusted_proxies: Allowlist, pub max_inflight_reads: u32 }` with `impl Default` (10, 30, empty, `127.0.0.1`+`::1`, 32).
  - `pub struct Counters { pub rate_limited_total: AtomicU64, pub inflight_reads: AtomicU32 }` (`Arc<Counters>` in `AppState`).
  - `pub fn router(state: AppState, cfg: &ApiConfig) -> Router`.
  - `AppState { store, status, counters: Arc<Counters>, read_permits: Arc<Semaphore> }` — `read_permits` is wired in Task 4 but the field is added here so the test harness changes once.
  - `ApiError::TooManyRequests { retry_after: u32 }` (429, title "Too Many Requests", detail "rate limit exceeded; retry after {n} s"), `ApiError::Overloaded` (503, title "Service Unavailable", detail "too many concurrent reads; retry shortly"). Both set `Retry-After`.

- [ ] **Step 1: Write the failing route tests**

In `crates/xp-api/tests/routes.rs`, change `app_with_stall` to build the state with counters and permits and to take an `ApiConfig`; add helpers:

```rust
use std::net::{IpAddr, SocketAddr};
use axum::extract::connect_info::ConnectInfo;
use xp_api::{ApiConfig, Counters};
use tokio::sync::Semaphore;

fn app_with(cfg: ApiConfig, stall: Option<StalledInfo>) -> (tempfile::TempDir, Router) {
    // … same store/fixture/watch setup as today …
    let state = xp_api::AppState {
        store: Arc::new(store),
        status: rx,
        counters: Arc::new(Counters::default()),
        read_permits: Arc::new(Semaphore::new(cfg.max_inflight_reads as usize)),
    };
    (dir, xp_api::router(state, &cfg))
}
fn app_with_stall(stall: Option<StalledInfo>) -> (tempfile::TempDir, Router) { app_with(ApiConfig::default(), stall) }

/// GET as a given peer, optionally with X-Forwarded-For.
async fn get_from(app: &Router, path: &str, peer: &str, xff: Option<&str>) -> (StatusCode, HeaderMap, Value) {
    let peer: SocketAddr = format!("{peer}:4000").parse().unwrap();
    let mut req = Request::builder().uri(path);
    if let Some(x) = xff { req = req.header("x-forwarded-for", x); }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, headers, v)
}
```

Tests:

```rust
fn limited(per_second: u32, burst: u32, allow: &[&str]) -> ApiConfig {
    ApiConfig {
        per_second, burst,
        allowlist: xp_api::limit::Allowlist::parse(&allow.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap(),
        ..ApiConfig::default()
    }
}

#[tokio::test]
async fn rate_limit_returns_429_after_burst_with_retry_after() {
    let (_d, app) = app_with(limited(1, 2, &[]), None);
    for _ in 0..2 {
        let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.1", None).await;
        assert_eq!(st, StatusCode::OK);
    }
    let (st, h, v) = get_from(&app, "/v1/status", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h.get("retry-after").unwrap(), "1");
    assert_eq!(v["status"], 429);
    assert_eq!(v["title"], "Too Many Requests");
    // A different client is unaffected.
    let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.2", None).await;
    assert_eq!(st, StatusCode::OK);
    // The counter moved.
    let (_, _, s) = get_from(&app, "/v1/status", "198.51.100.3", None).await;
    assert_eq!(s["rate_limited_total"], 1);
}

#[tokio::test]
async fn allowlisted_clients_are_never_limited() {
    let (_d, app) = app_with(limited(1, 1, &["198.51.100.0/24"]), None);
    for _ in 0..5 {
        let (st, _, _) = get_from(&app, "/v1/status", "198.51.100.7", None).await;
        assert_eq!(st, StatusCode::OK);
    }
}

#[tokio::test]
async fn forwarded_for_is_honoured_only_from_a_trusted_proxy() {
    let (_d, app) = app_with(limited(1, 1, &[]), None);
    // Behind the trusted proxy (127.0.0.1), two distinct forwarded clients each get one.
    assert_eq!(get_from(&app, "/v1/status", "127.0.0.1", Some("1.1.1.1")).await.0, StatusCode::OK);
    assert_eq!(get_from(&app, "/v1/status", "127.0.0.1", Some("2.2.2.2")).await.0, StatusCode::OK);
    assert_eq!(get_from(&app, "/v1/status", "127.0.0.1", Some("1.1.1.1")).await.0, StatusCode::TOO_MANY_REQUESTS);
    // From an untrusted peer the header is ignored: the peer itself is the key.
    assert_eq!(get_from(&app, "/v1/status", "203.0.113.5", Some("3.3.3.3")).await.0, StatusCode::OK);
    assert_eq!(get_from(&app, "/v1/status", "203.0.113.5", Some("4.4.4.4")).await.0, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn zero_rate_disables_limiting() {
    let (_d, app) = app_with(limited(0, 1, &[]), None);
    for _ in 0..10 {
        assert_eq!(get_from(&app, "/v1/status", "198.51.100.1", None).await.0, StatusCode::OK);
    }
}

#[tokio::test]
async fn requests_without_connect_info_are_keyed_as_unspecified_and_still_limited() {
    // `oneshot` without ConnectInfo (the existing `get` helper) must not panic.
    let (_d, app) = app_with(limited(1, 1, &[]), None);
    let (st, _) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = get(&app, "/v1/status").await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test -p xp-api --test routes rate_limit`
Expected: compile failure (`ApiConfig`, `Counters`, `router` arity).

- [ ] **Step 3: Errors**

In `crates/xp-api/src/error.rs` add variants and mappings:

```rust
    #[error("rate limited")]
    TooManyRequests { retry_after: u32 },
    #[error("overloaded")]
    Overloaded,
```

`status()`: `TOO_MANY_REQUESTS` / `SERVICE_UNAVAILABLE`. `title()`: `"Too Many Requests"` / `"Service Unavailable"`. `detail()`: `format!("rate limit exceeded; retry after {retry_after} s")` / `"too many concurrent reads; retry shortly".to_owned()`. In `into_response`, after building the response, set `Retry-After`:

```rust
        let retry_after = match &self {
            ApiError::TooManyRequests { retry_after } => Some(*retry_after),
            ApiError::Overloaded => Some(1),
            _ => None,
        };
        let mut resp = (status, Json(body)).into_response();
        if let Some(s) = retry_after {
            resp.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(s));
        }
        resp
```

(`use axum::http::{header, HeaderValue};`.) Do not log these two variants.

- [ ] **Step 4: Config, counters, state**

In `crates/xp-api/src/lib.rs`:

```rust
use std::sync::atomic::{AtomicU32, AtomicU64};
use tokio::sync::Semaphore;
pub use limit::Allowlist;

/// Runtime knobs for the public API (spec §3–§5). `bin/explorer` builds it from TOML.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub per_second: u32,
    pub burst: u32,
    pub allowlist: Allowlist,
    pub trusted_proxies: Allowlist,
    pub max_inflight_reads: u32,
}

impl Default for ApiConfig {
    fn default() -> ApiConfig {
        ApiConfig {
            per_second: 10,
            burst: 30,
            allowlist: Allowlist::default(),
            trusted_proxies: Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).expect("static"),
            max_inflight_reads: 32,
        }
    }
}

#[derive(Debug, Default)]
pub struct Counters {
    pub rate_limited_total: AtomicU64,
    pub inflight_reads: AtomicU32,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub status: watch::Receiver<IngestStatus>,
    pub counters: Arc<Counters>,
    pub read_permits: Arc<Semaphore>,
}
```

`router(state: AppState, cfg: &ApiConfig) -> Router`: build the routes as today, then `.layer(limit::RateLimit::new(cfg, state.counters.clone()))` **outside** (after) the `TimeoutLayer` so a limited request never enters the timeout budget, and keep `CorsLayer` outermost.

- [ ] **Step 5: The layer**

Append to `crates/xp-api/src/limit.rs`:

```rust
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service};
use crate::{ApiConfig, ApiError, Counters};

const SWEEP_EVERY: Duration = Duration::from_secs(60);
const IDLE_FOR: Duration = Duration::from_secs(60);

struct Shared {
    per_second: f64,
    burst: u32,
    allowlist: Allowlist,
    trusted: Allowlist,
    counters: Arc<Counters>,
    buckets: Mutex<(HashMap<IpAddr, TokenBucket>, Instant)>, // (map, last sweep)
}

#[derive(Clone)]
pub struct RateLimit(Arc<Shared>);

impl RateLimit {
    pub fn new(cfg: &ApiConfig, counters: Arc<Counters>) -> RateLimit {
        RateLimit(Arc::new(Shared {
            per_second: cfg.per_second as f64,
            burst: cfg.burst.max(1),
            allowlist: cfg.allowlist.clone(),
            trusted: cfg.trusted_proxies.clone(),
            counters,
            buckets: Mutex::new((HashMap::new(), Instant::now())),
        }))
    }

    /// `Ok(())` to pass, `Err(seconds)` to reject.
    fn check(&self, key: IpAddr, now: Instant) -> Result<(), u32> {
        let s = &self.0;
        if s.per_second <= 0.0 || s.allowlist.contains(key) {
            return Ok(());
        }
        let mut guard = s.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let (map, last_sweep) = &mut *guard;
        if now.saturating_duration_since(*last_sweep) >= SWEEP_EVERY {
            map.retain(|_, b| !b.is_idle(now, s.per_second, s.burst, IDLE_FOR));
            *last_sweep = now;
        }
        let bucket = map.entry(key).or_insert_with(|| TokenBucket::new(s.burst));
        match bucket.try_take(now, s.per_second, s.burst) {
            Ok(()) => Ok(()),
            Err(wait) => {
                s.counters.rate_limited_total.fetch_add(1, Ordering::Relaxed);
                Err((wait.as_secs_f64().ceil() as u32).max(1))
            }
        }
    }
}

impl<S> Layer<S> for RateLimit {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> RateLimitService<S> {
        RateLimitService { inner, limit: self.clone() }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    limit: RateLimit,
}

impl<S> Service<Request<Body>> for RateLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Requests without ConnectInfo (tests via `oneshot`) share one key; production always
        // has it because main serves with `into_make_service_with_connect_info`.
        let peer = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        let xff = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let key = client_key(peer, xff.as_deref(), &self.limit.0.trusted);
        match self.limit.check(key, Instant::now()) {
            Ok(()) => {
                let fut = self.inner.call(req);
                Box::pin(fut)
            }
            Err(retry_after) => Box::pin(async move {
                Ok(ApiError::TooManyRequests { retry_after }.into_response())
            }),
        }
    }
}
```

Note for the implementer: with `Clone + Send` inner services axum's `Router` satisfies these bounds; if `poll_ready`/clone semantics complain, use the standard `let clone = self.inner.clone(); let mut inner = std::mem::replace(&mut self.inner, clone);` pattern inside `call`.

- [ ] **Step 6: Status counters**

`StatusDto` gains `pub inflight_reads: u32, pub rate_limited_total: u64`; `handlers::status::status` fills them from `state.counters` (`Ordering::Relaxed` loads). Fix the two existing status tests' expectations if they compare whole objects.

- [ ] **Step 7: Run everything**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: green. (`bin/explorer` will not compile until its call site passes `&ApiConfig::default()` and fills the two new state fields — do that minimal change in `main.rs` now: `counters: Arc::new(xp_api::Counters::default())`, `read_permits: Arc::new(tokio::sync::Semaphore::new(32))`, `xp_api::router(app_state, &xp_api::ApiConfig::default())`. Task 5 replaces it with real config.)

- [ ] **Step 8: Commit**

```bash
git commit -m "feat(api): per-IP rate limiting with trusted proxies, allowlist and 429 problem responses" -- crates/xp-api bin/explorer/src/main.rs
```

---

### Task 4: Bounded blocking reads

**Files:**
- Modify: `crates/xp-api/src/lib.rs` (`blocking()`)
- Modify: `crates/xp-api/src/handlers/status.rs` (already reads counters)
- Test: `crates/xp-api/tests/routes.rs`

**Interfaces:**
- Consumes: `AppState.read_permits: Arc<Semaphore>`, `Counters.inflight_reads`, `ApiError::Overloaded`.
- Produces: `blocking()` semantics: `try_acquire_owned` on `read_permits`; failure ⇒ `Err(ApiError::Overloaded)`; `inflight_reads` incremented while a permit is held (decrement on every exit path, including panic-free error returns).

- [ ] **Step 1: Failing test**

```rust
#[tokio::test]
async fn reads_beyond_the_permit_budget_fail_fast_with_503() {
    let cfg = ApiConfig { max_inflight_reads: 1, per_second: 0, ..ApiConfig::default() };
    let (_d, app) = app_with(cfg.clone(), None);
    // Occupy the single permit from outside the router, exactly as a parked reader would.
    // The harness exposes the state's semaphore via a second helper:
    let (_d2, app2, state) = app_with_state(cfg, None);
    let _held = state.read_permits.clone().try_acquire_owned().unwrap();
    let (st, h, v) = get_from(&app2, "/v1/blocks?limit=1", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(h.get("retry-after").unwrap(), "1");
    assert_eq!(v["title"], "Service Unavailable");
    drop(_held);
    let (st, _, _) = get_from(&app2, "/v1/blocks?limit=1", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::OK);
    // /v1/status does not take a permit (it reads the watch channel, not the store).
    let (st, _, s) = get_from(&app2, "/v1/status", "198.51.100.1", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["inflight_reads"], 0);
    let _ = app;
}
```

Add `fn app_with_state(cfg, stall) -> (TempDir, Router, xp_api::AppState)` to the harness (refactor `app_with` to call it and drop the state).

- [ ] **Step 2: Run, confirm failure** — `cargo test -p xp-api --test routes reads_beyond` → FAIL (200 instead of 503).

- [ ] **Step 3: Implement**

```rust
pub(crate) async fn blocking<T, F>(state: &AppState, f: F) -> Result<T, ApiError>
where
    F: FnOnce(&Reader) -> Result<T, ApiError> + Send + 'static,
    T: Send + 'static,
{
    let permit = state
        .read_permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::Overloaded)?;
    let counters = state.counters.clone();
    counters.inflight_reads.fetch_add(1, Ordering::Relaxed);
    let store = state.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit; // released when the read finishes
        let rd = Reader::new(&store)?;
        f(&rd)
    })
    .await;
    counters.inflight_reads.fetch_sub(1, Ordering::Relaxed);
    result.map_err(|e| ApiError::Internal(format!("blocking task failed: {e}")))?
}
```

- [ ] **Step 4: Gates** — `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` green.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): bound concurrent blocking store reads, 503 when saturated" -- crates/xp-api
```

---

### Task 5: `[api]` config section and peer addresses in the binary

**Files:**
- Modify: `bin/explorer/src/config.rs`, `bin/explorer/src/main.rs`
- Test: `bin/explorer/src/config.rs` (unit tests in-file, following the existing `Config::parse` tests if present; otherwise add a `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `xp_api::{ApiConfig, Allowlist}`.
- Produces: `pub struct ApiSection { max_inflight_reads: u32, trusted_proxies: Vec<String>, rate_limit: RateLimitSection }`, `pub struct RateLimitSection { per_second: u32, burst: u32, allowlist: Vec<String> }`, `impl TryFrom<&ApiSection> for xp_api::ApiConfig` (Err(String) on bad CIDR), `Config.api: ApiSection` with `#[serde(default)]`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn api_section_defaults_when_absent() {
    let cfg = Config::parse(r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
    "#).unwrap();
    let api = xp_api::ApiConfig::try_from(&cfg.api).unwrap();
    assert_eq!((api.per_second, api.burst, api.max_inflight_reads), (10, 30, 32));
    assert!(api.trusted_proxies.contains("127.0.0.1".parse().unwrap()));
    assert!(!api.allowlist.contains("1.1.1.1".parse().unwrap()));
}

#[test]
fn api_section_parses_and_rejects_bad_cidr() {
    let good = Config::parse(r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
        [api]
        max_inflight_reads = 4
        trusted_proxies = ["10.0.0.1"]
        [api.rate_limit]
        per_second = 2
        burst = 5
        allowlist = ["203.0.113.0/24", "2001:db8::/32"]
    "#).unwrap();
    let api = xp_api::ApiConfig::try_from(&good.api).unwrap();
    assert_eq!((api.per_second, api.burst, api.max_inflight_reads), (2, 5, 4));
    assert!(api.allowlist.contains("203.0.113.9".parse().unwrap()));
    assert!(api.trusted_proxies.contains("10.0.0.1".parse().unwrap()));
    assert!(!api.trusted_proxies.contains("127.0.0.1".parse().unwrap()));

    let bad = Config::parse(r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
        [api.rate_limit]
        allowlist = ["1.2.3.4/40"]
    "#).unwrap();
    let err = xp_api::ApiConfig::try_from(&bad.api).unwrap_err();
    assert!(err.contains("1.2.3.4/40"), "{err}");
}
```

- [ ] **Step 2: Run, confirm failure** — `cargo test -p explorer api_section` → compile error.

- [ ] **Step 3: Implement the section**

```rust
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ApiSection {
    pub max_inflight_reads: u32,
    pub trusted_proxies: Vec<String>,
    pub rate_limit: RateLimitSection,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RateLimitSection {
    pub per_second: u32,
    pub burst: u32,
    pub allowlist: Vec<String>,
}

impl Default for ApiSection {
    fn default() -> ApiSection {
        ApiSection {
            max_inflight_reads: 32,
            trusted_proxies: vec!["127.0.0.1".into(), "::1".into()],
            rate_limit: RateLimitSection::default(),
        }
    }
}

impl Default for RateLimitSection {
    fn default() -> RateLimitSection {
        RateLimitSection { per_second: 10, burst: 30, allowlist: Vec::new() }
    }
}

impl TryFrom<&ApiSection> for xp_api::ApiConfig {
    type Error = String;
    fn try_from(s: &ApiSection) -> Result<xp_api::ApiConfig, String> {
        Ok(xp_api::ApiConfig {
            per_second: s.rate_limit.per_second,
            burst: s.rate_limit.burst,
            allowlist: xp_api::Allowlist::parse(&s.rate_limit.allowlist)
                .map_err(|e| format!("[api.rate_limit] allowlist: {e}"))?,
            trusted_proxies: xp_api::Allowlist::parse(&s.trusted_proxies)
                .map_err(|e| format!("[api] trusted_proxies: {e}"))?,
            max_inflight_reads: s.max_inflight_reads.max(1),
        })
    }
}
```

Add `#[serde(default)] pub api: ApiSection` to `Config`.

- [ ] **Step 4: Wire `main.rs`**

After parsing config: `let api_cfg = xp_api::ApiConfig::try_from(&cfg.api).map_err(|e| anyhow::anyhow!(e))?;` (this happens before binding, so a bad allowlist exits 1 like other config errors). Build state with `counters: Arc::new(xp_api::Counters::default())`, `read_permits: Arc::new(tokio::sync::Semaphore::new(api_cfg.max_inflight_reads as usize))`, call `xp_api::router(app_state, &api_cfg)`, and serve with peer addresses:

```rust
axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
```

Log the effective limits once at startup: `info!(per_second, burst, max_inflight_reads, allowlist = cfg.api.rate_limit.allowlist.len(), "api limits")`.

- [ ] **Step 5: Gates** — full cargo gates green.

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(explorer): [api] config section for rate limits, trusted proxies and read permits" -- bin/explorer
```

---

### Task 6: Frontend types, status facts, e2e mock

**Files:**
- Modify: `frontend/src/lib/api/types.ts`, `frontend/src/lib/api/endpoints.ts:60-61`
- Modify: `frontend/src/routes/address/[addr]/+page.svelte:109` (type only), `frontend/src/routes/status/+page.svelte`
- Modify: `frontend/tests/e2e/mock/handlers.ts:166,294`, `frontend/tests/e2e/mock/fixtures.ts` (if it types the address txs list)
- Test: `frontend/tests/unit/client.test.ts` (endpoint path unchanged; add a type-level check via `expectTypeOf` if the file already uses it, else none), `frontend/tests/e2e/address.spec.ts`, `frontend/tests/e2e/status.spec.ts`

**Interfaces:**
- Consumes: wire shapes from Tasks 1 and 3.
- Produces: `export interface TxSummaryDto { id; height; index; timestamp; size; fee; input_count; data_input_count; output_count }`; `api.addressTxs` returns `PageDto<TxSummaryDto>`; `StatusDto.inflight_reads: number; rate_limited_total: number`.

- [ ] **Step 1: Failing e2e**

In `frontend/tests/e2e/status.spec.ts` add an assertion that the status page shows facts labelled "Reads in flight" and "Rate limited" (values from the mock: `0` and `0`). In `address.spec.ts` no assertion changes (the table shows id/height/age/fee), but the mock must switch shape, so run the suite after the change.

- [ ] **Step 2: Mock**

`handlers.ts`: add `txSummariesOfTree(d, tree): TxSummaryDto[]` mapping `txsOfTree` items to `{ id, height, index, timestamp, size, fee, input_count: t.inputs.length, data_input_count: t.data_inputs.length, output_count: t.outputs.length }`; the `/v1/addresses/{addr}/txs` route uses it. `StatusDto` mock gains `inflight_reads: 0, rate_limited_total: 0`.

- [ ] **Step 3: Types, endpoint, pages**

`types.ts`: add `TxSummaryDto`, extend `StatusDto`. `endpoints.ts`: `addressTxs` returns `apiGet<PageDto<TxSummaryDto>>`. `address/[addr]/+page.svelte`: `createPager<TxSummaryDto>`; the table cells already use only `id`, `height`, `timestamp`, `fee`. `status/+page.svelte`: two `<Fact>` rows in the existing facts list, labels "Reads in flight" and "Rate limited", values `data.status.inflight_reads` and `data.status.rate_limited_total` formatted with the page's existing number formatter.

- [ ] **Step 4: Gates** — `npm test && npm run check && npm run lint && npm run build && npx playwright test --workers=4` green.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): address tx summaries and API limit counters on the status page" -- frontend/src frontend/tests
```

---

### Task 7: Docs

**Files:**
- Modify: `bin/explorer/README.md` (configuration section: the `[api]` block from spec §5 with each key explained; a "Limits" paragraph: 429/503 semantics, `Retry-After`, allowlisting the fleet), `CHANGELOG.md` (Unreleased → "Plan 3a — ops hardening" list: summaries, rate limiting, bounded reads, status counters, config).

- [ ] **Step 1: Write the docs** (prose from spec §3–§7; the TOML block verbatim).
- [ ] **Step 2: `npx prettier --check` on any Markdown under `frontend/` you touched** (none expected).
- [ ] **Step 3: Commit**

```bash
git commit -m "docs: [api] limits configuration and Plan 3a changelog" -- bin/explorer/README.md CHANGELOG.md
```

Deployment (controller, not a task): build release, copy to Nuremberg, append the `[api]` section with the operator IP and fleet arms in `allowlist` to `/opt/explorer/explorer.toml`, `systemctl restart explorer`, redeploy the frontend, then verify a burst from an unlisted IP returns 429 and `/v1/status` counts it.
