//! Tenancy and behaviour proof for alo's first scoped role — the accountant
//! (ADR 0035, wave B4.12; `docs/design/finance.md`, "The accountant role").
//!
//! A role row is an **access fact**, so the isolation question here is sharper
//! than for a record: a leak does not show somebody a number they should not
//! see, it hands them a door. Three properties carry the file:
//!
//! - a grant proves tenant membership before it writes, so another tenant's
//!   user id can never become a role holder here (`users.id` is globally
//!   unique, which is exactly what makes the naive `INSERT` wrong);
//! - a read is tenant-bound, so the same user id holding the role *there*
//!   holds nothing *here*;
//! - the admin flag and the role set are independent facts, and the one query
//!   that reads them together (`access_facts`, which every request runs) agrees
//!   with the per-user read the admin console uses.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::{Store, StoreError, TenantId, TenantRole, UserId};

use crate::common::test_store;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got: {value:?}"),
    }
}

/// A tenant with one user, returned as the ids the role table speaks in.
async fn tenant_with_user(store: &Store, tag: &str) -> (TenantId, UserId) {
    let tenant = store.create_tenant(&format!("role-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@roles.test"))
        .await
        .unwrap();
    (tenant, user)
}

#[tokio::test]
async fn a_grant_is_readable_revocable_and_idempotent() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "grant").await;
    let (_, admin) = (tenant.clone(), {
        store
            .for_tenant(tenant.clone())
            .create_user("boss@roles.test")
            .await
            .unwrap()
    });
    let ts = store.for_tenant(tenant.clone());

    assert!(ts.user_roles(&user).await.unwrap().is_empty());

    ts.grant_role(&user, TenantRole::Accountant, &admin)
        .await
        .unwrap();
    assert_eq!(
        ts.user_roles(&user).await.unwrap(),
        vec![TenantRole::Accountant]
    );

    // Granting twice is one grant, not two rows and not an error: the caller's
    // intent — this person has the books — is already true.
    ts.grant_role(&user, TenantRole::Accountant, &admin)
        .await
        .unwrap();
    assert_eq!(
        ts.user_roles(&user).await.unwrap(),
        vec![TenantRole::Accountant],
        "a repeated grant stays one role"
    );
    assert_eq!(
        ts.role_grants().await.unwrap(),
        vec![(user.clone(), TenantRole::Accountant)],
        "and one row in the tenant-wide read"
    );

    ts.revoke_role(&user, TenantRole::Accountant).await.unwrap();
    assert!(ts.user_roles(&user).await.unwrap().is_empty());
    // Revoking what nobody holds is a no-op for the same reason.
    ts.revoke_role(&user, TenantRole::Accountant).await.unwrap();
    assert!(ts.role_grants().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_role_and_the_admin_flag_are_independent_facts() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "facts").await;
    let ts = store.for_tenant(tenant.clone());
    let door = store.for_account(tenant.clone(), user.clone());

    let facts = door.access_facts().await.unwrap();
    assert!(!facts.is_admin);
    assert!(!facts.has(TenantRole::Accountant));

    ts.grant_role(&user, TenantRole::Accountant, &user)
        .await
        .unwrap();
    let facts = door.access_facts().await.unwrap();
    assert!(
        facts.has(TenantRole::Accountant),
        "the one query every request runs sees the grant"
    );
    assert!(!facts.is_admin, "a role never confers the admin flag");

    ts.set_admin(&user, true).await.unwrap();
    let facts = door.access_facts().await.unwrap();
    assert!(facts.is_admin && facts.has(TenantRole::Accountant));

    // And the admin flag is not a role either: dropping the role leaves it.
    ts.revoke_role(&user, TenantRole::Accountant).await.unwrap();
    let facts = door.access_facts().await.unwrap();
    assert!(facts.is_admin && !facts.has(TenantRole::Accountant));
}

#[tokio::test]
async fn a_role_cannot_be_granted_across_tenants() {
    let store = test_store().await;
    let (ours, our_user) = tenant_with_user(&store, "ours").await;
    let (theirs, their_user) = tenant_with_user(&store, "theirs").await;
    let our_ts = store.for_tenant(ours.clone());
    let their_ts = store.for_tenant(theirs.clone());

    // `users.id` is globally unique, so an INSERT that trusted the id would
    // make their user an accountant of our tenant. It does not exist here.
    assert_not_found(
        our_ts
            .grant_role(&their_user, TenantRole::Accountant, &our_user)
            .await,
    );
    assert_not_found(
        our_ts
            .revoke_role(&their_user, TenantRole::Accountant)
            .await,
    );
    assert!(
        our_ts.role_grants().await.unwrap().is_empty(),
        "the refused grant wrote nothing"
    );

    // The same user id, granted the role where they DO belong, is invisible
    // from our tenant — and the account door they open in ours (which they can
    // never authenticate into, but the store must still answer safely) carries
    // no role.
    their_ts
        .grant_role(&their_user, TenantRole::Accountant, &their_user)
        .await
        .unwrap();
    assert_eq!(
        their_ts.user_roles(&their_user).await.unwrap(),
        vec![TenantRole::Accountant]
    );
    assert!(
        our_ts.user_roles(&their_user).await.unwrap().is_empty(),
        "a role is held in a tenant, not by a user id"
    );
    assert!(our_ts.role_grants().await.unwrap().is_empty());
    let foreign_door = store.for_account(ours.clone(), their_user.clone());
    let facts = foreign_door.access_facts().await.unwrap();
    assert!(!facts.is_admin && !facts.has(TenantRole::Accountant));
}

#[tokio::test]
async fn deleting_a_user_takes_their_access_with_them() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "gone").await;
    let ts = store.for_tenant(tenant.clone());
    ts.grant_role(&user, TenantRole::Accountant, &user)
        .await
        .unwrap();
    ts.delete_user(&user).await.unwrap();
    assert!(
        ts.role_grants().await.unwrap().is_empty(),
        "a deleted user leaves no dangling grant behind"
    );
    assert_not_found(ts.grant_role(&user, TenantRole::Accountant, &user).await);
}
