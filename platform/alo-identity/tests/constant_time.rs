//! The pinned precondition, proven not asserted: an unknown user and a
//! known-user-wrong-password take **comparable** time, because the unknown
//! path still pays one argon2 verification (the dummy hash). Without that
//! dummy hash the unknown path would skip argon2 entirely and be
//! orders of magnitude faster — a user-existence timing oracle.
//!
//! This is a statistical test with a deliberately generous band (it must
//! not flake in CI); its job is to catch a *missing* dummy hash, which
//! would show as a ~100× speedup, not to measure a few-percent skew.
//!
//! We compare the **minimum** observed time of each path, not the median:
//! both paths run exactly one argon2 hash, so their compute floor is the
//! same; scheduler/IO noise only ever *adds* time, so the minimum is the
//! stable estimator of that floor and does not flake under load. A missing
//! dummy hash collapses the unknown-user minimum to a bare DB miss.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Instant;

use common::{make_user, setup};

#[tokio::test]
async fn unknown_user_and_wrong_password_are_timing_comparable() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "timing").await;

    // Warm up (pool warm, first argon2 allocation, page-ins).
    for _ in 0..3 {
        let _ = id.authenticate_password(&u.email, "warmup").await;
        let _ = id.authenticate_password("nobody@ex.test", "warmup").await;
    }

    let iterations = 15;
    let mut wrong_min = u128::MAX;
    let mut unknown_min = u128::MAX;
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = id.authenticate_password(&u.email, "wrong-password").await;
        wrong_min = wrong_min.min(start.elapsed().as_nanos());

        let start = Instant::now();
        let _ = id
            .authenticate_password("nobody-unknown@ex.test", "wrong-password")
            .await;
        unknown_min = unknown_min.min(start.elapsed().as_nanos());
    }

    let ratio = unknown_min as f64 / wrong_min as f64;
    // Evidence: printed with `--nocapture`.
    println!(
        "constant-time: wrong_password_min={wrong_min}ns unknown_user_min={unknown_min}ns ratio={ratio:.3}"
    );

    // Both paths pay one argon2 hash, so the minima are close (ratio ≈ 1).
    // A missing dummy hash makes the unknown path skip argon2 → ratio well
    // below 0.1. The band is generous to survive scheduler noise.
    assert!(
        ratio > 0.35,
        "unknown-user path is suspiciously fast (ratio {ratio:.3}); the dummy argon2 hash may be missing"
    );
    assert!(
        ratio < 3.0,
        "unknown-user path is far slower than wrong-password (ratio {ratio:.3}); unexpected extra work"
    );
}

/// The same property for app passwords (mail M1.1): an unknown username
/// pays one dummy argon2 verify on `verify_app_password`, so it is not
/// measurably faster than a known user's wrong app password. The user
/// under test holds exactly one app password — the paths then run one
/// argon2 pass each, and a missing dummy hash collapses the unknown-user
/// minimum to a bare DB miss.
#[tokio::test]
async fn unknown_user_and_wrong_app_password_are_timing_comparable() {
    let (store, id) = setup().await;
    let u = make_user(&store, &id, "ap-timing").await;
    id.create_app_password(&u.tenant, &u.user, "timing probe")
        .await
        .unwrap();

    // Warm up (pool warm, first argon2 allocation, page-ins).
    for _ in 0..3 {
        let _ = id
            .verify_app_password(&u.email, "aaaa-bbbb-cccc-dddd")
            .await;
        let _ = id
            .verify_app_password("nobody@ex.test", "aaaa-bbbb-cccc-dddd")
            .await;
    }

    let iterations = 15;
    let mut wrong_min = u128::MAX;
    let mut unknown_min = u128::MAX;
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = id
            .verify_app_password(&u.email, "aaaa-bbbb-cccc-dddd")
            .await;
        wrong_min = wrong_min.min(start.elapsed().as_nanos());

        let start = Instant::now();
        let _ = id
            .verify_app_password("nobody-unknown@ex.test", "aaaa-bbbb-cccc-dddd")
            .await;
        unknown_min = unknown_min.min(start.elapsed().as_nanos());
    }

    let ratio = unknown_min as f64 / wrong_min as f64;
    println!(
        "constant-time: wrong_app_password_min={wrong_min}ns unknown_user_min={unknown_min}ns ratio={ratio:.3}"
    );
    assert!(
        ratio > 0.35,
        "unknown-user path is suspiciously fast (ratio {ratio:.3}); the dummy argon2 hash may be missing"
    );
    assert!(
        ratio < 3.0,
        "unknown-user path is far slower than wrong-app-password (ratio {ratio:.3}); unexpected extra work"
    );
}
