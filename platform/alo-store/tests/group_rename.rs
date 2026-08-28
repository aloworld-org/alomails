//! Renaming a group / distribution list: the new name sticks, and a foreign or
//! absent group id is NotFound (never silently succeeds).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::test_store;
use alo_store::{GroupId, StoreError};

#[tokio::test]
async fn rename_sticks_and_scopes_to_tenant() {
    let store = test_store().await;

    let tenant = store.create_tenant("t-grp-rename").await.unwrap();
    let ts = store.for_tenant(tenant);
    let g = ts.create_group("Team").await.unwrap();

    ts.rename_group(&g, "Squad").await.unwrap();
    let groups = ts.list_groups().await.unwrap();
    assert!(
        groups
            .iter()
            .any(|x| x.id == g.as_str() && x.name == "Squad"),
        "the rename is reflected in the list",
    );

    // A group id that isn't this tenant's is NotFound.
    assert!(matches!(
        ts.rename_group(&GroupId::new("does-not-exist"), "X").await,
        Err(StoreError::NotFound)
    ));

    // Another tenant cannot rename this tenant's group.
    let other = store.create_tenant("t-grp-other").await.unwrap();
    let ots = store.for_tenant(other);
    assert!(matches!(
        ots.rename_group(&g, "Hijack").await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(
        ts.list_groups().await.unwrap()[0].name,
        "Squad",
        "unchanged"
    );
}
