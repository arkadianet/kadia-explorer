//! Per-client rate limiting (spec §3): token buckets keyed by client IP, a CIDR allowlist,
//! the proxy-aware client key, and the tower layer that puts them in front of the router.

use crate::{ApiConfig, ApiError, Counters};
use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tower::{Layer, Service};

#[derive(Debug, Clone, Copy)]
pub struct TokenBucket {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(burst: u32) -> TokenBucket {
        TokenBucket::new_at(burst, Instant::now())
    }

    /// [`new`](TokenBucket::new) with the clock injected, so callers that already hold a
    /// request timestamp (and tests) never read the clock twice.
    pub fn new_at(burst: u32, now: Instant) -> TokenBucket {
        TokenBucket {
            tokens: burst as f64,
            last: now,
        }
    }

    /// Refill up to `burst` at `per_second`, then take one token. `Err(wait)` is the time
    /// until one token is available. A non-positive or non-finite `per_second` means rate
    /// limiting is disabled (spec §3): always succeeds without touching the bucket.
    pub fn try_take(&mut self, now: Instant, per_second: f64, burst: u32) -> Result<(), Duration> {
        if per_second <= 0.0 || !per_second.is_finite() {
            return Ok(());
        }
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

    /// True when the bucket has been full for `idle` — safe to drop from the map. A
    /// non-positive or non-finite `per_second` means rate limiting is disabled, so there is
    /// nothing to keep track of and the bucket is always idle.
    pub fn is_idle(&self, now: Instant, per_second: f64, burst: u32, idle: Duration) -> bool {
        if per_second <= 0.0 || !per_second.is_finite() {
            return true;
        }
        let full_since =
            self.last + Duration::from_secs_f64((burst as f64 - self.tokens).max(0.0) / per_second);
        now.saturating_duration_since(full_since) >= idle
    }
}

/// A single IPv4 or IPv6 network in CIDR notation. Matching normalises IPv4-mapped IPv6
/// addresses to IPv4 first (see [`contains`](Cidr::contains)), so `0.0.0.0/0` matches every
/// IPv4 (and IPv4-mapped) address but no native IPv6 address, and `::/0` matches every native
/// IPv6 address but no IPv4 (or IPv4-mapped) address — the two families never cross-match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

fn unmap(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
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
        let raw = ip.parse::<IpAddr>().map_err(|e| format!("{s}: {e}"))?;
        let addr = unmap(raw);
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.parse::<u8>().map_err(|e| format!("{s}: {e}"))?,
            None => max,
        };
        if prefix > max {
            if raw.is_ipv6() && addr.is_ipv4() {
                return Err(format!(
                    "{s}: address is an IPv4-mapped IPv6 address, normalised to IPv4; \
                     prefix {prefix} exceeds {max}"
                ));
            }
            return Err(format!("{s}: prefix {prefix} exceeds {max}"));
        }
        Ok(Cidr { addr, prefix })
    }
}

impl Cidr {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, unmap(ip)) {
            (IpAddr::V4(a), IpAddr::V4(b)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                (u32::from(a) & mask) == (u32::from(b) & mask)
            }
            (IpAddr::V6(a), IpAddr::V6(b)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
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
        items
            .iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .map(Allowlist)
    }
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|c| c.contains(ip))
    }
}

/// Spec §3 "Client key". Walks `X-Forwarded-For` right to left past trusted proxies.
pub fn client_key(peer: IpAddr, forwarded_for: Option<&str>, trusted: &Allowlist) -> IpAddr {
    let peer = unmap(peer);
    if !trusted.contains(peer) {
        return peer;
    }
    let Some(header) = forwarded_for else {
        return peer;
    };
    for hop in header.rsplit(',') {
        let hop = hop.trim();
        if hop.is_empty() {
            continue;
        }
        match hop.parse::<IpAddr>() {
            Ok(ip) if trusted.contains(unmap(ip)) => continue,
            Ok(ip) => return unmap(ip),
            Err(_) => return peer,
        }
    }
    peer
}

// ---------------------------------------------------------------------------------------
// The tower layer
// ---------------------------------------------------------------------------------------

/// How often idle buckets are swept out of the map, and how long a full bucket must have
/// been idle to be swept. Both are amortised onto request handling: no background task.
const SWEEP_EVERY: Duration = Duration::from_secs(60);
const IDLE_FOR: Duration = Duration::from_secs(60);

/// Hard ceiling on tracked buckets, so a source-address rotation flood cannot grow the map
/// without bound. At roughly 56 bytes per entry a full map is a few megabytes. Once a sweep
/// cannot get back under the cap the whole map is cleared: that refills every client's bucket,
/// which is the safe direction (a brief under-limit, never an over-limit or an OOM).
const MAX_BUCKETS: usize = 100_000;

struct Shared {
    per_second: f64,
    burst: u32,
    /// [`MAX_BUCKETS`] in production; tests set it small.
    max_buckets: usize,
    allowlist: Allowlist,
    trusted: Allowlist,
    counters: Arc<Counters>,
    /// (buckets by client key, time of the last sweep).
    buckets: Mutex<(HashMap<IpAddr, TokenBucket>, Instant)>,
}

/// Per-client token-bucket rate limiting as a `tower` layer.
#[derive(Clone)]
pub struct RateLimit(Arc<Shared>);

impl RateLimit {
    pub fn new(cfg: &ApiConfig, counters: Arc<Counters>) -> RateLimit {
        RateLimit::with_max_buckets(cfg, counters, MAX_BUCKETS)
    }

    fn with_max_buckets(cfg: &ApiConfig, counters: Arc<Counters>, max_buckets: usize) -> RateLimit {
        RateLimit(Arc::new(Shared {
            per_second: f64::from(cfg.per_second),
            burst: cfg.burst.max(1),
            max_buckets,
            allowlist: cfg.allowlist.clone(),
            trusted: cfg.trusted_proxies.clone(),
            counters,
            buckets: Mutex::new((HashMap::new(), Instant::now())),
        }))
    }

    /// `Ok(())` to pass, `Err(seconds)` to reject with that `Retry-After`.
    fn check(&self, key: IpAddr, now: Instant) -> Result<(), u32> {
        let s = &self.0;
        if s.per_second <= 0.0 || s.allowlist.contains(key) {
            return Ok(());
        }
        // A poisoned lock only means some other request panicked mid-update; the map is a
        // plain cache of buckets, so recovering the guard is strictly better than taking the
        // whole API down with it.
        let mut guard = s.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let (map, last_sweep) = &mut *guard;
        let due = now.saturating_duration_since(*last_sweep) >= SWEEP_EVERY;
        if due || map.len() >= s.max_buckets {
            map.retain(|_, b| !b.is_idle(now, s.per_second, s.burst, IDLE_FOR));
            *last_sweep = now;
            // Still over the cap: every tracked bucket is live, so there is nothing to evict
            // selectively that would not be arbitrary. Drop the lot (see [`MAX_BUCKETS`]).
            if map.len() >= s.max_buckets {
                map.clear();
            }
        }
        let bucket = map
            .entry(key)
            .or_insert_with(|| TokenBucket::new_at(s.burst, now));
        match bucket.try_take(now, s.per_second, s.burst) {
            Ok(()) => Ok(()),
            Err(wait) => {
                s.counters
                    .rate_limited_total
                    .fetch_add(1, Ordering::Relaxed);
                Err((wait.as_secs_f64().ceil() as u32).max(1))
            }
        }
    }
}

impl<S> Layer<S> for RateLimit {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> RateLimitService<S> {
        RateLimitService {
            inner,
            limit: self.clone(),
        }
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
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // `bin/explorer` serves the router through
        // `into_make_service_with_connect_info::<SocketAddr>()`, so in production the peer
        // address is always present. The fallback is for direct `oneshot` calls (tests), which
        // carry no `ConnectInfo`: they all share the unspecified key rather than panicking.
        let peer = req
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        // All `X-Forwarded-For` lines, in order, joined into one list. Reading only the first
        // line would let a client that sends its own header outrank the hop the trusted proxy
        // appends, and so choose its own rate-limit key.
        let xff = req
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join(",");
        let xff = if xff.is_empty() { None } else { Some(xff) };
        let key = client_key(peer, xff.as_deref(), &self.limit.0.trusted);
        match self.limit.check(key, Instant::now()) {
            Ok(()) => {
                // Only the clone this service holds has been made ready by `poll_ready`, so
                // call *it* and leave a fresh clone behind for the next request.
                let clone = self.inner.clone();
                let mut inner = std::mem::replace(&mut self.inner, clone);
                Box::pin(async move { inner.call(req).await })
            }
            Err(retry_after) => {
                Box::pin(
                    async move { Ok(ApiError::TooManyRequests { retry_after }.into_response()) },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::time::{Duration, Instant};

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn bucket_allows_burst_then_refills() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(3);
        for _ in 0..3 {
            assert!(b.try_take(t0, 1.0, 3).is_ok());
        }
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
    fn bucket_disabled_rate_never_limits() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(3);
        assert!(b.try_take(t0, 0.0, 3).is_ok());
        assert!(b.try_take(t0, 0.0, 3).is_ok());
        assert!(b.try_take(t0, f64::NAN, 3).is_ok());
    }

    #[test]
    fn bucket_is_idle_basic() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(3);
        assert!(b.try_take(t0, 1.0, 3).is_ok());
        // Just used: not idle yet.
        assert!(!b.is_idle(t0, 1.0, 3, Duration::from_secs(1)));
        // Bucket refills the missing token after 1s, then stays idle from there.
        assert!(b.is_idle(
            t0 + Duration::from_secs(1) + Duration::from_secs(1),
            1.0,
            3,
            Duration::from_secs(1)
        ));
    }

    #[test]
    fn bucket_is_idle_disabled_rate_is_always_idle() {
        let t0 = Instant::now();
        let b = TokenBucket::new(3);
        assert!(b.is_idle(t0, 0.0, 3, Duration::from_secs(1)));
    }

    #[test]
    fn bucket_map_is_capped_and_cleared_under_a_rotation_flood() {
        let cfg = ApiConfig {
            per_second: 1,
            burst: 1,
            ..ApiConfig::default()
        };
        let limit = RateLimit::with_max_buckets(&cfg, Arc::new(Counters::default()), 4);
        let t0 = Instant::now();
        // Every key is fresh, so no sweep can evict anything: the map must still stay capped.
        for i in 0..50u32 {
            let key = IpAddr::V4(std::net::Ipv4Addr::from(0x0b00_0000 + i));
            assert!(limit.check(key, t0).is_ok());
            let len = limit.0.buckets.lock().unwrap().0.len();
            assert!(len <= 4, "map grew to {len} past the cap");
        }
        // Limiting still works below the cap: a fresh limiter, one key, two takes.
        let limit = RateLimit::with_max_buckets(&cfg, Arc::new(Counters::default()), 4);
        let key = IpAddr::V4(std::net::Ipv4Addr::new(11, 1, 1, 1));
        assert!(limit.check(key, t0).is_ok());
        assert_eq!(limit.check(key, t0), Err(1));
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
    fn cidr_prefix_zero_and_max() {
        let all_v4: Cidr = "0.0.0.0/0".parse().unwrap();
        assert!(all_v4.contains(ip("9.9.9.9")));
        let all_v6: Cidr = "::/0".parse().unwrap();
        assert!(all_v6.contains(ip("2001:db8::1")));
        assert!(!all_v6.contains(ip("9.9.9.9")));
        let exact_v4: Cidr = "203.0.113.7/32".parse().unwrap();
        assert!(exact_v4.contains(ip("203.0.113.7")));
        assert!(!exact_v4.contains(ip("203.0.113.8")));
        let exact_v6: Cidr = "2001:db8::1/128".parse().unwrap();
        assert!(exact_v6.contains(ip("2001:db8::1")));
        assert!(!exact_v6.contains(ip("2001:db8::2")));
    }

    #[test]
    fn client_key_uses_forwarded_only_behind_trusted_proxy() {
        let trusted = Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).unwrap();
        // Untrusted peer: header ignored.
        assert_eq!(
            client_key(ip("198.51.100.9"), Some("1.1.1.1"), &trusted),
            ip("198.51.100.9")
        );
        // Trusted peer: right-most non-trusted hop.
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("1.1.1.1, 127.0.0.1"), &trusted),
            ip("1.1.1.1")
        );
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("9.9.9.9, 1.1.1.1"), &trusted),
            ip("1.1.1.1")
        );
        // Trusted peer, garbage/missing header: peer address, never bypass.
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("not an ip"), &trusted),
            ip("127.0.0.1")
        );
        assert_eq!(client_key(ip("127.0.0.1"), None, &trusted), ip("127.0.0.1"));
    }

    #[test]
    fn client_key_normalises_v4_mapped_peer() {
        let trusted = Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).unwrap();
        assert_eq!(
            client_key(ip("::ffff:8.8.8.8"), None, &trusted),
            ip("8.8.8.8")
        );
        // Mapped trusted peer is still recognised as trusted.
        assert_eq!(
            client_key(ip("::ffff:127.0.0.1"), Some("1.1.1.1"), &trusted),
            ip("1.1.1.1")
        );
    }

    #[test]
    fn client_key_security_edge_cases() {
        let trusted = Allowlist::parse(&["127.0.0.1".into(), "::1".into()]).unwrap();
        // Only trusted proxies in the chain: fall back to peer.
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("127.0.0.1, ::1"), &trusted),
            ip("127.0.0.1")
        );
        // Blank hops (double comma) are skipped.
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("1.1.1.1, , 2.2.2.2"), &trusted),
            ip("2.2.2.2")
        );
        // A hop with a port is not a bare IP: treat as unparsable, fall back to peer.
        assert_eq!(
            client_key(ip("127.0.0.1"), Some("1.2.3.4:5678"), &trusted),
            ip("127.0.0.1")
        );
    }
}
