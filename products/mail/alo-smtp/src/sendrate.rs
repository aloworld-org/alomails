//! Outbound send-rate limiting — a token bucket per destination domain.
//!
//! A compromised or runaway account can burn the deployment's sending
//! IP reputation faster than any inbound control matters. This caps how
//! fast we hand messages to any one destination domain: when a domain's
//! bucket is empty the message is **deferred** (left in the queue for a
//! later pass), never dropped — the queue's existing retry/backoff does
//! the waiting, so a burst is smoothed into a steady rate rather than
//! lost.
//!
//! Per-destination, not global: the reputation that matters is per
//! receiving domain (Gmail, Outlook, …), and a legitimate blast to one
//! large domain must not stall mail to every other. The bucket refills
//! continuously at `rate_per_min`, capped at `burst`, so a quiet period
//! banks a short burst without allowing a sustained flood.
//!
//! Time is injected (`now_secs`) so the queue's single clock drives it —
//! no `Instant::now()` sprinkled through delivery, and it is
//! deterministically testable.

use std::collections::HashMap;
use std::sync::Mutex;

/// A per-domain token-bucket rate limiter over an externally supplied
/// clock (epoch seconds).
pub struct SendRateLimiter {
    rate_per_min: f64,
    burst: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

#[derive(Clone, Copy)]
struct Bucket {
    /// Tokens available (fractional — refill is continuous).
    tokens: f64,
    /// Epoch second of the last refill.
    updated: i64,
}

impl SendRateLimiter {
    /// A limiter allowing `rate_per_min` messages per minute to each
    /// domain, with a bucket depth (max instantaneous burst) of
    /// `burst`. `rate_per_min == 0` disables limiting (every send is
    /// allowed).
    pub fn new(rate_per_min: u32, burst: u32) -> Self {
        Self {
            rate_per_min: f64::from(rate_per_min),
            // A burst below 1 could never admit a single message; floor
            // it at 1 whenever limiting is on.
            burst: f64::from(burst).max(1.0),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Whether disabled (no rate configured) — the caller can skip the
    /// bucket bookkeeping entirely.
    pub fn is_disabled(&self) -> bool {
        self.rate_per_min <= 0.0
    }

    /// Tries to spend one token for `domain` at `now_secs`. `true`
    /// admits the send; `false` means the domain is over its rate and
    /// the caller must defer. A disabled limiter always admits.
    pub fn try_acquire(&self, domain: &str, now_secs: i64) -> bool {
        if self.is_disabled() {
            return true;
        }
        let key = domain.to_ascii_lowercase();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let bucket = buckets.entry(key).or_insert(Bucket {
            tokens: self.burst,
            updated: now_secs,
        });
        // Continuous refill since the last update (clamped ≥ 0 so a
        // backwards clock never grants a windfall).
        let elapsed = (now_secs - bucket.updated).max(0) as f64;
        bucket.tokens = (bucket.tokens + elapsed * self.rate_per_min / 60.0).min(self.burst);
        bucket.updated = now_secs;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Drops buckets untouched for at least `idle_secs` (so the map
    /// cannot grow without bound as destinations come and go). Called
    /// opportunistically by the queue, not on the hot path.
    pub fn reap(&self, now_secs: i64, idle_secs: i64) {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        buckets.retain(|_, b| now_secs - b.updated < idle_secs);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn burst_then_throttle_then_refill() {
        // 60/min = 1/sec, burst 3.
        let limiter = SendRateLimiter::new(60, 3);
        let t = 1_000_000;
        // The initial burst of 3 is admitted at the same instant...
        assert!(limiter.try_acquire("gmail.com", t));
        assert!(limiter.try_acquire("gmail.com", t));
        assert!(limiter.try_acquire("gmail.com", t));
        // ...the fourth in the same second is throttled.
        assert!(!limiter.try_acquire("gmail.com", t));
        // One second later, one token has refilled (1/sec).
        assert!(limiter.try_acquire("gmail.com", t + 1));
        assert!(!limiter.try_acquire("gmail.com", t + 1));
        // After a long idle the bucket refills only up to `burst`.
        assert!(limiter.try_acquire("gmail.com", t + 10_000));
        assert!(limiter.try_acquire("gmail.com", t + 10_000));
        assert!(limiter.try_acquire("gmail.com", t + 10_000));
        assert!(!limiter.try_acquire("gmail.com", t + 10_000));
    }

    #[test]
    fn domains_are_independent_and_case_folded() {
        let limiter = SendRateLimiter::new(60, 1);
        let t = 500;
        assert!(limiter.try_acquire("example.com", t));
        assert!(
            !limiter.try_acquire("EXAMPLE.COM", t),
            "same domain, folded"
        );
        // A different domain has its own bucket.
        assert!(limiter.try_acquire("other.test", t));
    }

    #[test]
    fn disabled_admits_everything() {
        let limiter = SendRateLimiter::new(0, 0);
        assert!(limiter.is_disabled());
        for i in 0..1000 {
            assert!(limiter.try_acquire("x.test", i));
        }
    }

    #[test]
    fn backwards_clock_grants_no_windfall() {
        let limiter = SendRateLimiter::new(60, 2);
        assert!(limiter.try_acquire("d.test", 100));
        assert!(limiter.try_acquire("d.test", 100));
        // Clock jumps backwards: no negative elapsed, still throttled.
        assert!(!limiter.try_acquire("d.test", 50));
    }

    #[test]
    fn reap_drops_idle_buckets() {
        let limiter = SendRateLimiter::new(60, 1);
        assert!(limiter.try_acquire("a.test", 0));
        assert!(limiter.try_acquire("b.test", 100));
        limiter.reap(200, 150); // a.test idle 200s, b.test idle 100s
        assert_eq!(
            limiter.buckets.lock().unwrap().len(),
            1,
            "only b.test remains"
        );
    }
}
