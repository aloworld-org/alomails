//! Flag follow-up due-date: set, read back, clear, and per-account isolation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::{fresh_account, tenant_fixture, test_store};
use alo_store::StoreError;

#[tokio::test]
async fn set_read_and_clear_due() {
    let store = test_store().await;
    let fx = tenant_fixture(&store, "due").await;

    assert!(
        fx.acc.flag_due(&fx.message).await.unwrap().is_none(),
        "unset by default"
    );

    // 2027-01-01T00:00:00Z = 1_798_761_600.
    let due = 1_798_761_600;
    fx.acc.set_flag_due(&fx.message, Some(due)).await.unwrap();
    let got = fx
        .acc
        .flag_due(&fx.message)
        .await
        .unwrap()
        .expect("a due-date");
    assert_eq!(got.unix_timestamp(), due);

    fx.acc.set_flag_due(&fx.message, None).await.unwrap();
    assert!(
        fx.acc.flag_due(&fx.message).await.unwrap().is_none(),
        "cleared"
    );
}

#[tokio::test]
async fn due_is_per_account() {
    let store = test_store().await;
    let fx = tenant_fixture(&store, "due-iso").await;
    let (other, _u, _inbox) = fresh_account(&store, "due-other").await;

    // Another account cannot set a due-date on this account's message.
    assert!(matches!(
        other.set_flag_due(&fx.message, Some(1_798_761_600)).await,
        Err(StoreError::NotFound)
    ));
    // And still reads nothing for it.
    assert!(fx.acc.flag_due(&fx.message).await.unwrap().is_none());
}
