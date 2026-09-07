//! Per-client rate limiting (spec §3): token buckets keyed by client IP, a CIDR allowlist,
//! and the proxy-aware client key. The tower layer that uses them is added in Task 3.

use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct TokenBucket {
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(burst: u32) -> TokenBucket {
        TokenBucket {
            tokens: burst as f64,
            last: Instant::now(),
        }
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
    // Used by Task 3's tower layer to evict stale per-client buckets.
    #[allow(dead_code)]
    pub fn is_idle(&self, now: Instant, per_second: f64, burst: u32, idle: Duration) -> bool {
        let full_since = self.last
            + Duration::from_secs_f64((burst as f64 - self.tokens).max(0.0) / per_second.max(1e-9));
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
    if !trusted.contains(peer) {
        return peer;
    }
    let Some(header) = forwarded_for else {
        return peer;
    };
    for hop in header.rsplit(',') {
        match hop.trim().parse::<IpAddr>() {
            Ok(ip) if trusted.contains(ip) => continue,
            Ok(ip) => return unmap(ip),
            Err(_) => return peer,
        }
    }
    peer
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
}
