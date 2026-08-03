//! A small in-process failure limiter for the credential endpoints. We use
//! **exponential backoff keyed on `(client, username)`**, not a hard
//! account lockout: a lockout is a denial-of-service lever a third party
//! can pull against a known username, whereas backoff slows an online
//! brute force without letting an attacker lock a victim out
//! (`docs/design/identity.md`). The SMTP/IMAP per-connection caps are
//! unchanged; this adds the same discipline to the token/authorize path.
//!
//! Single-node and in-memory: it bounds one process's exposure. A
//! cross-node limiter belongs with the gateway/ops layer, recorded there.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Failures tolerated before backoff engages.
const FREE_ATTEMPTS: u32 = 5;
/// Ceiling on a single backoff window.
const MAX_BACKOFF_SECS: u64 = 300;
/// Idle time after which a stale entry is forgotten (keeps the map bounded).
const FORGET_AFTER: Duration = Duration::from_secs(3600);

#[derive(Clone, Copy)]
struct Attempt {
    failures: u32,
    blocked_until: Option<Instant>,
    last_seen: Instant,
}

/// A cloneable handle to the shared limiter state.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Attempt>>>,
}

impl RateLimiter {
    /// A fresh, empty limiter.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// If the key is currently backed off, the remaining wait; otherwise
    /// `None` (the attempt may proceed). Poisoned-lock recovery treats the
    /// limiter as open (fail-safe for availability; the per-connection caps
    /// still bound brute force).
    pub fn retry_after(&self, key: &str) -> Option<Duration> {
        let map = self.inner.lock().ok()?;
        let now = Instant::now();
        let entry = map.get(key).copied()?;
        match entry.blocked_until {
            Some(until) if until > now => Some(until - now),
            _ => None,
        }
    }

    /// Records a failed attempt and arms backoff once the free attempts are
    /// spent.
    pub fn record_failure(&self, key: &str) {
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        let now = Instant::now();
        prune(&mut map, now);
        let entry = map.entry(key.to_owned()).or_insert(Attempt {
            failures: 0,
            blocked_until: None,
            last_seen: now,
        });
        entry.failures = entry.failures.saturating_add(1);
        entry.last_seen = now;
        if entry.failures > FREE_ATTEMPTS {
            let over = entry.failures - FREE_ATTEMPTS;
            // 1s, 2s, 4s, … capped — `min(over, 32)` keeps the shift safe.
            let secs = 1u64
                .checked_shl(over.min(32))
                .unwrap_or(MAX_BACKOFF_SECS)
                .min(MAX_BACKOFF_SECS);
            entry.blocked_until = Some(now + Duration::from_secs(secs));
        }
    }

    /// Clears the counter after a successful authentication.
    pub fn record_success(&self, key: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(key);
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn prune(map: &mut HashMap<String, Attempt>, now: Instant) {
    map.retain(|_, a| now.duration_since(a.last_seen) < FORGET_AFTER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_attempts_are_not_blocked() {
        let rl = RateLimiter::new();
        for _ in 0..FREE_ATTEMPTS {
            rl.record_failure("client|alice");
        }
        assert!(rl.retry_after("client|alice").is_none());
    }

    #[test]
    fn backoff_engages_after_free_attempts() {
        let rl = RateLimiter::new();
        for _ in 0..(FREE_ATTEMPTS + 1) {
            rl.record_failure("client|alice");
        }
        assert!(rl.retry_after("client|alice").is_some());
        // A different key is unaffected — no collateral lockout.
        assert!(rl.retry_after("client|bob").is_none());
    }

    #[test]
    fn success_clears_the_counter() {
        let rl = RateLimiter::new();
        for _ in 0..(FREE_ATTEMPTS + 2) {
            rl.record_failure("client|alice");
        }
        rl.record_success("client|alice");
        assert!(rl.retry_after("client|alice").is_none());
    }
}
