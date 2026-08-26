//! Distribution lists at the store layer (Law 1: isolation is tested, not
//! assumed). A group is a tenant's membership set; giving it an address turns
//! it into a mail destination, so every operation on it is probed from the
//! wrong tenant, and the loop-safety promise — members are users, never other
//! lists — is proven at membership-write time rather than assumed at delivery
//! time.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{GroupId, Store, StoreError, TenantStore, UserId};
use common::test_store;

/// A tenant with one user, behind the tenant door.
async fn tenant(store: &Store, tag: &str) -> (TenantStore, UserId, String) {
    let t = store.create_tenant(&format!("glst-{tag}")).await.unwrap();
    let ts = store.for_tenant(t.clone());
    let email = format!("{tag}-{t}@glst.test");
    let user = ts.create_user(&email).await.unwrap();
    (ts, user, email)
}

#[tokio::test]
async fn wrong_tenant_is_denied_on_every_group_operation() {
    let store = test_store().await;
    let (a, a_user, _) = tenant(&store, "own").await;
    let (b, b_user, _) = tenant(&store, "other").await;

    let g = a.create_group("Sales").await.unwrap();
    a.add_group_member(&g, &a_user).await.unwrap();

    // Read paths: the foreign tenant gets NotFound, never an empty view it
    // could mistake for "exists but has no members".
    assert!(matches!(
        b.group_members(&g).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.group_members_detailed(&g).await,
        Err(StoreError::NotFound)
    ));
    assert!(
        b.list_groups().await.unwrap().is_empty(),
        "another tenant's groups never appear in the listing"
    );

    // Write paths: rename, address, membership, delete — all NotFound, and
    // the group is proven untouched after each.
    assert!(matches!(
        b.rename_group(&g, "Hijack").await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.set_group_address(&g, Some("sales@glst.test")).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.add_group_member(&g, &b_user).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.remove_group_member(&g, &a_user).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        b.delete_group(&g).await,
        Err(StoreError::NotFound)
    ));

    // A cannot enroll B's user either: the member must be A's own user.
    assert!(matches!(
        a.add_group_member(&g, &b_user).await,
        Err(StoreError::NotFound)
    ));

    let groups = a.list_groups().await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Sales", "name survived the foreign rename");
    assert_eq!(groups[0].address, None, "address survived the foreign set");
    assert_eq!(
        a.group_members(&g).await.unwrap(),
        vec![a_user.clone()],
        "membership survived the foreign add/remove, and only A's user is in it"
    );
}

#[tokio::test]
async fn a_list_can_never_contain_a_list() {
    // Loop safety is enforced where membership is written: a member is a user
    // id, and a group id presented as one is refused — so expansion at
    // delivery time is single-level and termination needs no cycle detection.
    let store = test_store().await;
    let (ts, user, _) = tenant(&store, "loop").await;

    let outer = ts.create_group("Outer").await.unwrap();
    let inner = ts.create_group("Inner").await.unwrap();
    ts.add_group_member(&inner, &user).await.unwrap();

    assert!(
        matches!(
            ts.add_group_member(&outer, &UserId::new(inner.as_str().to_owned()))
                .await,
            Err(StoreError::NotFound)
        ),
        "a group id is not a user, so a list inside a list is refused at write time"
    );
    // Self-containment is the same refusal.
    assert!(matches!(
        ts.add_group_member(&outer, &UserId::new(outer.as_str().to_owned()))
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(ts.group_members(&outer).await.unwrap().is_empty());
}

#[tokio::test]
async fn list_addresses_are_globally_unique_and_expansion_is_tenant_true() {
    let store = test_store().await;
    let (a, a_user, _) = tenant(&store, "addr-a").await;
    let (b, b_user, _) = tenant(&store, "addr-b").await;

    let ga = a.create_group("Team A").await.unwrap();
    a.add_group_member(&ga, &a_user).await.unwrap();
    let address = format!("everyone-{}@glst.test", ga.as_str());
    a.set_group_address(&ga, Some(&address)).await.unwrap();

    // The same inbound address cannot belong to two tenants' lists.
    let gb = b.create_group("Team B").await.unwrap();
    b.add_group_member(&gb, &b_user).await.unwrap();
    assert!(matches!(
        b.set_group_address(&gb, Some(&address)).await,
        Err(StoreError::Conflict(_))
    ));

    // Inbound expansion resolves to A's tenant and A's member — never B's.
    let members = store.list_members_by_address(&address).await.unwrap();
    assert_eq!(members.len(), 1);
    let (t, u) = &members[0];
    assert_eq!(u, &a_user);
    // The pair is A's account door: the tenant id is the group's tenant.
    assert!(t.as_str() != b.tenant().as_str(), "never the other tenant");

    // The address is stored lowercase and matched case-insensitively.
    let upper = address.to_uppercase();
    assert_eq!(
        store.list_members_by_address(&upper).await.unwrap().len(),
        1,
        "matching is on the lowercased address"
    );

    // Clearing the address turns the list off.
    a.set_group_address(&ga, None).await.unwrap();
    assert!(
        store
            .list_members_by_address(&address)
            .await
            .unwrap()
            .is_empty()
    );

    // An id that was never a group is NotFound, not a silent success.
    assert!(matches!(
        a.set_group_address(&GroupId::new("never-existed"), Some("x@glst.test"))
            .await,
        Err(StoreError::NotFound)
    ));
}
