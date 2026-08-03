//! Per-IP inbound connection limiting — a native abuse control at the
//! connection layer, below where Rspamd (DATA-time) or the auth backoff
//! (post-AUTH) can act.
//!
//! The global `max_connections` semaphore already caps total concurrent
//! sessions, but one host opening hundreds of slow connections could
//! still starve every other sender. This bounds *per source IP*: over
//! the cap, the connection is refused with `421` (the same transient
//! "come back later" the global cap uses), so a legitimate host briefly
//! over the limit simply retries while an abuser is throttled.
//!
//! Counting, not rate: concurrent connections per IP is the quantity an
//! abuser actually consumes (sockets, tasks, memory); it needs no clock
//! and no per-IP timer to reap. IPv6 is bucketed by /64 — the smallest
//! block a single subscriber is typically assigned — so an abuser
//! cannot sidestep the cap by walking a /64's 2^64 addresses.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// Tracks concurrent inbound connections per source IP and admits or
/// refuses new ones against a fixed cap. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct PerIpLimiter {
    max_per_ip: usize,
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl PerIpLimiter {
    /// A limiter admitting at most `max_per_ip` concurrent connections
    /// from any one IP (or /64 for IPv6). `0` disables limiting — every
    /// connection is admitted (the guard is inert).
    pub fn new(max_per_ip: usize) -> Self {
        Self {
            max_per_ip,
            counts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Tries to admit a connection from `ip`. `Some(guard)` reserves a
    /// slot that is released when the guard drops; `None` means the IP
    /// is at its cap and the caller must refuse (421). A disabled
    /// limiter always admits.
    pub fn admit(&self, ip: IpAddr) -> Option<ConnGuard> {
        if self.max_per_ip == 0 {
            return Some(ConnGuard { limiter: None });
        }
        let key = bucket(ip);
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        let entry = counts.entry(key).or_insert(0);
        if *entry >= self.max_per_ip {
            return None;
        }
        *entry += 1;
        Some(ConnGuard {
            limiter: Some((self.clone(), key)),
        })
    }

    /// Current concurrent count for `ip`'s bucket (for tests/metrics).
    pub fn count(&self, ip: IpAddr) -> usize {
        let counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts.get(&bucket(ip)).copied().unwrap_or(0)
    }

    fn release(&self, key: IpAddr) {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = counts.get_mut(&key) {
            *entry -= 1;
            if *entry == 0 {
                // Reap the bucket so the map cannot grow unboundedly
                // under a spray of one-shot connections from many IPs.
                counts.remove(&key);
            }
        }
    }
}

/// Holds a per-IP connection slot; releasing it on drop keeps the count
/// accurate however the session ends (clean quit, I/O error, or panic).
pub struct ConnGuard {
    limiter: Option<(PerIpLimiter, IpAddr)>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        if let Some((limiter, key)) = self.limiter.take() {
            limiter.release(key);
        }
    }
}

/// The rate-limiting key for an IP: IPv4 addresses stand alone; IPv6 is
/// bucketed to its /64 so a single subscriber's block counts as one.
fn bucket(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let mut octets = v6.octets();
            octets[8..].fill(0); // keep the /64 prefix, zero the interface id
            IpAddr::V6(octets.into())
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn admits_up_to_the_cap_then_refuses() {
        let limiter = PerIpLimiter::new(2);
        let a = ip("192.0.2.1");
        let g1 = limiter.admit(a);
        let g2 = limiter.admit(a);
        assert!(g1.is_some() && g2.is_some());
        assert_eq!(limiter.count(a), 2);
        assert!(limiter.admit(a).is_none(), "third connection is refused");
        // A different IP is unaffected.
        assert!(limiter.admit(ip("192.0.2.2")).is_some());
    }

    #[test]
    fn releasing_a_guard_frees_a_slot() {
        let limiter = PerIpLimiter::new(1);
        let a = ip("198.51.100.7");
        let g = limiter.admit(a);
        assert!(g.is_some());
        assert!(limiter.admit(a).is_none());
        drop(g);
        assert_eq!(limiter.count(a), 0, "bucket reaped at zero");
        assert!(limiter.admit(a).is_some(), "slot freed after drop");
    }

    #[test]
    fn ipv6_is_bucketed_by_64() {
        let limiter = PerIpLimiter::new(1);
        // Two addresses in the same /64 share the cap.
        let _g = limiter.admit(ip("2001:db8:abcd:1::1"));
        assert!(
            limiter.admit(ip("2001:db8:abcd:1::ffff")).is_none(),
            "same /64 counts together"
        );
        // A different /64 is separate.
        assert!(limiter.admit(ip("2001:db8:abcd:2::1")).is_some());
    }

    #[test]
    fn zero_disables_the_limit() {
        let limiter = PerIpLimiter::new(0);
        let a = ip("203.0.113.9");
        let guards: Vec<_> = (0..1000).map(|_| limiter.admit(a)).collect();
        assert!(guards.iter().all(Option::is_some));
        assert_eq!(limiter.count(a), 0, "disabled limiter tracks nothing");
    }
}
