//! Tenancy and behaviour proof for the admin console's per-user app switches
//! (migration 0208; `platform/alo-store/src/user_modules.rs`).
//!
//! A denial row is an **access fact** like a role, so the same sharpness
//! applies: a leak here does not show somebody a number, it hands them a door —
//! or, in this direction, fails to shut one. Four properties carry the file:
//!
//! - the default is open, and it is open because the table is empty rather
//!   than because something backfilled it;
//! - a switch proves tenant membership before it writes, so another tenant's
//!   user id can never be denied here (`users.id` is globally unique, which is
//!   what makes the naive `INSERT` wrong);
//! - a read is tenant-bound, so the same user id denied *there* is allowed
//!   *here*;
//! - `access_facts` — the single query every request runs — agrees with the
//!   per-user read the admin console uses, and a tenant admin is never denied.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{AppModule, Store, StoreError, TenantId, TenantRole, UserId};

use common::test_store;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got: {value:?}"),
    }
}

/// A tenant with one user, returned as the ids the denial table speaks in.
async fn tenant_with_user(store: &Store, tag: &str) -> (TenantId, UserId) {
    let tenant = store.create_tenant(&format!("mods-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@modules.test"))
        .await
        .unwrap();
    (tenant, user)
}

#[tokio::test]
async fn everybody_starts_with_every_app() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "default").await;
    let ts = store.for_tenant(tenant.clone());

    // The point of storing denials: a brand new account is allowed everything
    // without a single row having been written for it.
    assert!(ts.denied_modules(&user).await.unwrap().is_empty());

    let facts = store
        .for_account(tenant, user)
        .access_facts()
        .await
        .unwrap();
    assert!(facts.denied_modules.is_empty());
    for module in alo_store::ALL_MODULES {
        assert!(facts.may_open(module), "{module} should be open by default");
    }
}

#[tokio::test]
async fn a_switch_is_readable_reversible_and_idempotent() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "switch").await;
    let admin = store
        .for_tenant(tenant.clone())
        .create_user("boss@modules.test")
        .await
        .unwrap();
    let ts = store.for_tenant(tenant.clone());

    ts.set_module_access(&user, AppModule::Billing, false, &admin)
        .await
        .unwrap();
    assert_eq!(
        ts.denied_modules(&user).await.unwrap(),
        vec![AppModule::Billing]
    );

    // Switching off twice is one denial, not two rows and not an error: the
    // caller's intent — this person does not have Billing — is already true,
    // and keeping the first row keeps the answer to "since when?".
    ts.set_module_access(&user, AppModule::Billing, false, &admin)
        .await
        .unwrap();
    assert_eq!(
        ts.denied_modules(&user).await.unwrap(),
        vec![AppModule::Billing],
        "a repeated switch-off stays one denial"
    );

    ts.set_module_access(&user, AppModule::Billing, true, &admin)
        .await
        .unwrap();
    assert!(ts.denied_modules(&user).await.unwrap().is_empty());
    // Switching on what was never off is a no-op for the same reason.
    ts.set_module_access(&user, AppModule::Billing, true, &admin)
        .await
        .unwrap();
    assert!(ts.denied_modules(&user).await.unwrap().is_empty());
}

#[tokio::test]
async fn denials_are_per_module_and_sorted() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "several").await;
    let ts = store.for_tenant(tenant.clone());

    for module in [AppModule::Sites, AppModule::Chat, AppModule::Billing] {
        ts.set_module_access(&user, module, false, &user)
            .await
            .unwrap();
    }
    // Sorted, so the console renders the same order every time and a test can
    // compare without sorting first.
    assert_eq!(
        ts.denied_modules(&user).await.unwrap(),
        vec![AppModule::Billing, AppModule::Chat, AppModule::Sites]
    );

    let facts = store
        .for_account(tenant, user)
        .access_facts()
        .await
        .unwrap();
    assert!(!facts.may_open(AppModule::Chat));
    assert!(
        facts.may_open(AppModule::Drive),
        "switching off three apps must not touch a fourth"
    );
}

#[tokio::test]
async fn a_switch_proves_membership_before_it_writes() {
    let store = test_store().await;
    let (theirs, their_user) = tenant_with_user(&store, "theirs").await;
    let (ours, _) = tenant_with_user(&store, "ours").await;

    // `users.id` is globally unique, so this id is a real user — just not one
    // of ours. Denying them here must refuse rather than write a row.
    assert_not_found(
        store
            .for_tenant(ours.clone())
            .set_module_access(&their_user, AppModule::Drive, false, &their_user)
            .await,
    );
    assert_not_found(store.for_tenant(ours).denied_modules(&their_user).await);

    // And nothing happened to them in their own tenant.
    assert!(
        store
            .for_tenant(theirs)
            .denied_modules(&their_user)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_tenant_admin_is_never_denied() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "admin").await;
    let ts = store.for_tenant(tenant.clone());

    ts.set_module_access(&user, AppModule::Finance, false, &user)
        .await
        .unwrap();
    ts.set_admin(&user, true).await.unwrap();

    let facts = store
        .for_account(tenant, user)
        .access_facts()
        .await
        .unwrap();
    // The row is still there and still readable — the console should show the
    // switch as the admin left it — but it does not shut the door on somebody
    // who can walk into the console and open it again anyway.
    assert_eq!(facts.denied_modules, vec![AppModule::Finance]);
    assert!(facts.may_open(AppModule::Finance));
}

#[tokio::test]
async fn denials_and_roles_are_independent_facts() {
    let store = test_store().await;
    let (tenant, user) = tenant_with_user(&store, "both").await;
    let ts = store.for_tenant(tenant.clone());

    ts.grant_role(&user, TenantRole::Accountant, &user)
        .await
        .unwrap();
    ts.set_module_access(&user, AppModule::Finance, false, &user)
        .await
        .unwrap();

    let facts = store
        .for_account(tenant, user)
        .access_facts()
        .await
        .unwrap();
    // Holding the role that opens Finance does not survive the app being
    // switched off: this narrows, and a role never widens past it.
    assert!(facts.has(TenantRole::Accountant));
    assert!(!facts.may_open(AppModule::Finance));
}

#[test]
fn an_unknown_module_word_is_refused_rather_than_ignored() {
    // An admin who typed a module this build has no gate for would otherwise
    // get a confirmation for a switch that was thrown away.
    assert!(AppModule::parse("billing").is_ok());
    assert!(AppModule::parse("  drive  ").is_ok());
    assert!(AppModule::parse("mail").is_err());
    assert!(AppModule::parse("").is_err());
}
