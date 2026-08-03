//! Retry scheduling: exponential backoff with deterministic jitter.
//!
//! Jitter is derived from a hash of (message id, attempt) rather than
//! a random source — the same inputs always produce the same delay,
//! which keeps retry behavior reproducible in tests and incident
//! forensics, while still de-synchronizing herds of messages queued
//! at the same instant.

use std::time::Duration;

/// Jitter band: the base delay is scaled into [0.8, 1.2].
const JITTER_SPREAD: f64 = 0.2;

/// Delay before attempt number `attempt` (1-based: the delay after
/// the first failure is `next_delay(id, 1, ...)`). Exponential in the
/// attempt count, capped, then jittered.
pub fn next_delay(id: &str, attempt: u32, base: Duration, cap: Duration) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    let un_jittered = base
        .saturating_mul(2_u32.saturating_pow(exp))
        .min(cap)
        .as_secs_f64();
    let factor = 1.0 - JITTER_SPREAD + 2.0 * JITTER_SPREAD * unit_hash(id, attempt);
    Duration::from_secs_f64(un_jittered * factor)
}

/// splitmix64 over (id, attempt), folded to a unit interval [0, 1).
fn unit_hash(id: &str, attempt: u32) -> f64 {
    let mut x = 0xcbf2_9ce4_8422_2325_u64; // FNV offset basis as seed
    for byte in id.bytes() {
        x = (x ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    x ^= u64::from(attempt);
    // splitmix64 finalizer
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 11) as f64 / (1_u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Duration = Duration::from_secs(60);
    const CAP: Duration = Duration::from_secs(3600);

    #[test]
    fn delays_grow_exponentially_until_the_cap() {
        // Compare un-jittered midpoints via wide bounds.
        let d1 = next_delay("msg-a", 1, BASE, CAP);
        let d2 = next_delay("msg-a", 2, BASE, CAP);
        let d4 = next_delay("msg-a", 4, BASE, CAP);
        let d10 = next_delay("msg-a", 10, BASE, CAP);
        assert!(d1 >= Duration::from_secs(48) && d1 <= Duration::from_secs(72));
        assert!(d2 >= Duration::from_secs(96) && d2 <= Duration::from_secs(144));
        assert!(d4 >= Duration::from_secs(384) && d4 <= Duration::from_secs(576));
        // Attempt 10 un-jittered would be 60*512s — capped at 3600.
        assert!(d10 >= Duration::from_secs(2880) && d10 <= Duration::from_secs(4320));
    }

    #[test]
    fn jitter_is_deterministic_and_varies_by_id_and_attempt() {
        assert_eq!(
            next_delay("msg-a", 3, BASE, CAP),
            next_delay("msg-a", 3, BASE, CAP)
        );
        assert_ne!(
            next_delay("msg-a", 3, BASE, CAP),
            next_delay("msg-b", 3, BASE, CAP)
        );
        assert_ne!(
            next_delay("msg-a", 3, BASE, CAP),
            next_delay("msg-a", 4, BASE, CAP) / 2
        );
    }

    #[test]
    fn huge_attempt_numbers_do_not_overflow() {
        let d = next_delay("msg-a", u32::MAX, BASE, CAP);
        assert!(d <= Duration::from_secs(4320));
    }
}
