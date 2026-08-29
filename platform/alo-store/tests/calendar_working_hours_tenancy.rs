//! Working hours persistence and its tenant boundary. The schedule is keyed
//! by the account door's `(tenant, user)` pair, so another tenant holding the
//! same user id must read the default — never the stored row — and a write
//! through a foreign door must never land on it. Runs against the real
//! Postgres from compose.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::WorkingHours;

#[tokio::test]
async fn a_schedule_round_trips_and_unset_reads_as_the_default() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "wh-rt").await;

    // Never set → the default, so scheduling works before any settings visit.
    assert_eq!(acc.working_hours().await.unwrap(), WorkingHours::default());

    let mine = WorkingHours {
        days: 0b0001_0111, // Mon–Wed + Fri
        start_minute: 8 * 60 + 30,
        end_minute: 16 * 60,
        zone: Some("Europe/Brussels".to_owned()),
    };
    acc.set_working_hours(&mine).await.unwrap();
    assert_eq!(acc.working_hours().await.unwrap(), mine);

    // A second set replaces, not duplicates.
    let later = WorkingHours {
        end_minute: 18 * 60,
        ..mine.clone()
    };
    acc.set_working_hours(&later).await.unwrap();
    assert_eq!(acc.working_hours().await.unwrap(), later);
}

#[tokio::test]
async fn an_invalid_schedule_is_refused_at_write() {
    let store = common::test_store().await;
    let (acc, _, _) = common::fresh_account(&store, "wh-bad").await;

    let backwards = WorkingHours {
        start_minute: 17 * 60,
        end_minute: 9 * 60,
        ..WorkingHours::default()
    };
    assert!(matches!(
        acc.set_working_hours(&backwards).await,
        Err(alo_store::StoreError::Validation(_))
    ));
    // Nothing stuck: the read is still the default.
    assert_eq!(acc.working_hours().await.unwrap(), WorkingHours::default());
}

#[tokio::test]
async fn working_hours_never_cross_the_tenant_boundary() {
    let store = common::test_store().await;
    let t1 = store.create_tenant("t-wh-iso-a").await.unwrap();
    let t2 = store.create_tenant("t-wh-iso-b").await.unwrap();
    let ua = store
        .for_tenant(t1.clone())
        .create_user("u-wh-iso-a@example.test")
        .await
        .unwrap();
    let ub = store
        .for_tenant(t2.clone())
        .create_user("u-wh-iso-b@example.test")
        .await
        .unwrap();
    let a = store.for_account(t1, ua.clone());
    let b = store.for_account(t2.clone(), ub);

    let a_hours = WorkingHours {
        days: 0b0000_0001, // Mondays only — unmistakably not the default
        start_minute: 10 * 60,
        end_minute: 12 * 60,
        zone: Some("Europe/Brussels".to_owned()),
    };
    a.set_working_hours(&a_hours).await.unwrap();

    // B's own read is the default — A's row is invisible.
    assert_eq!(b.working_hours().await.unwrap(), WorkingHours::default());

    // A door forged with B's tenant and A's user id reads the default too:
    // the row is keyed by the pair, and half a pair reaches nothing.
    let forged = store.for_account(t2, ua);
    assert_eq!(
        forged.working_hours().await.unwrap(),
        WorkingHours::default()
    );

    // Writing through the forged door lands on its own pair, never A's row.
    forged
        .set_working_hours(&WorkingHours::default())
        .await
        .unwrap();
    assert_eq!(a.working_hours().await.unwrap(), a_hours);
}
