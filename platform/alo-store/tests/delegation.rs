//! Mailbox delegation (ADR 0017): grants carry an access level (read-only vs
//! manage) and a send mode; they're set/read/revoked correctly, a user can't
//! delegate to themselves, and — the acceptance gate — a grant is scoped to its
//! tenant so it can NEVER authorize across tenants.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common::test_store;
use alo_store::StoreError;

#[tokio::test]
async fn grant_levels_and_revoke() {
    let store = test_store().await;
    let tenant = store.create_tenant("t-deleg").await.unwrap();
    let ts = store.for_tenant(tenant);
    let owner = ts.create_user("owner@deleg.test").await.unwrap();
    let delegate = ts.create_user("del@deleg.test").await.unwrap();

    assert!(
        ts.delegation(&owner, &delegate).await.unwrap().is_none(),
        "no grant yet"
    );

    // Read-only grant.
    ts.grant_delegate(&owner, &delegate, false, "none")
        .await
        .unwrap();
    assert_eq!(
        ts.delegation(&owner, &delegate).await.unwrap(),
        Some((false, "none".to_owned()))
    );

    // Upgrade to manage + send-on-behalf. A send mode implies write.
    ts.grant_delegate(&owner, &delegate, false, "on_behalf")
        .await
        .unwrap();
    assert_eq!(
        ts.delegation(&owner, &delegate).await.unwrap(),
        Some((true, "on_behalf".to_owned())),
        "any send mode forces can_write",
    );

    // Listings carry the level + mode.
    let of_owner = ts.delegates_of(&owner).await.unwrap();
    assert_eq!(of_owner.len(), 1);
    assert_eq!(of_owner[0].1, "del@deleg.test");
    assert!(of_owner[0].2, "can_write");
    assert_eq!(of_owner[0].3, "on_behalf");
    let for_delegate = ts.delegations_for(&delegate).await.unwrap();
    assert_eq!(for_delegate[0].1, "owner@deleg.test");

    // A user cannot delegate to themselves; a bad send mode is rejected.
    assert!(matches!(
        ts.grant_delegate(&owner, &owner, true, "as").await,
        Err(StoreError::Conflict(_))
    ));
    assert!(matches!(
        ts.grant_delegate(&owner, &delegate, true, "bogus").await,
        Err(StoreError::Conflict(_))
    ));

    ts.revoke_delegate(&owner, &delegate).await.unwrap();
    assert!(ts.delegation(&owner, &delegate).await.unwrap().is_none());
}

#[tokio::test]
async fn grants_never_cross_tenants() {
    let store = test_store().await;
    let ta = store.create_tenant("t-deleg-a").await.unwrap();
    let tsa = store.for_tenant(ta);
    let owner = tsa.create_user("owner@a.test").await.unwrap();
    let delegate = tsa.create_user("del@a.test").await.unwrap();
    tsa.grant_delegate(&owner, &delegate, true, "as")
        .await
        .unwrap();

    // A different tenant's store, querying the very same user ids, sees nothing:
    // the grant row is stamped with tenant A, and every query is tenant-scoped.
    let tb = store.create_tenant("t-deleg-b").await.unwrap();
    let tsb = store.for_tenant(tb);
    assert!(
        tsb.delegation(&owner, &delegate).await.unwrap().is_none(),
        "a grant in tenant A must be invisible to tenant B",
    );
    assert!(tsb.delegations_for(&delegate).await.unwrap().is_empty());
    assert!(tsb.delegates_of(&owner).await.unwrap().is_empty());

    assert_eq!(
        tsa.delegation(&owner, &delegate).await.unwrap(),
        Some((true, "as".to_owned()))
    );
}
