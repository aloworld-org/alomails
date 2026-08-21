//! Personal signup provisioning (ADR 0018) against the live store: an
//! address is claimed once, a second claim is refused with no dangling user,
//! reserved/invalid names are refused, and each personal user is its own
//! isolated tenant that inbound resolution finds unambiguously.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use alo_identity::signup::SignupError;
use common::setup;

/// A globally-unique personal domain per test run (the shared test DB means
/// the login username `localpart@domain` must not collide across reruns).
fn unique_domain(tag: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{tag}{n}.alomails.test")
}

#[tokio::test]
async fn provisions_a_usable_personal_account() {
    let (store, identity) = setup().await;
    let domain = unique_domain("prov");

    let acct = identity
        .provision_personal(
            &domain,
            "JohnSmith",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .expect("provisioned");
    assert_eq!(acct.email, format!("johnsmith@{domain}"));

    // Inbound resolution finds exactly this account.
    let resolved = store.account_by_email(&acct.email).await.unwrap();
    assert_eq!(resolved, Some((acct.tenant.clone(), acct.user.clone())));

    // Standard mailboxes exist (Inbox + the role folders).
    let accs = store.for_account(acct.tenant.clone(), acct.user.clone());
    assert!(accs.mailbox_by_role("sent").await.unwrap().is_some());
    assert!(accs.mailbox_by_role("trash").await.unwrap().is_some());
    // Inbox is get-or-create; it must already be there.
    assert!(accs.mailbox_by_role("inbox").await.unwrap().is_some());

    // **And they administer the tenant they just made.** Nobody else can grant
    // this: the tenant is one second old and they are its only member, so a
    // signup that leaves the flag false produces a tenant with no admin at all
    // and every admin surface in it dark forever. That shipped — the flag had
    // to be set by hand in psql on the deployment's own owner — because this
    // assertion did not exist.
    assert!(
        accs.is_admin().await.unwrap(),
        "the person who created the tenant does not administer it"
    );
}

/// Signing up makes an admin **of that tenant only**.
///
/// The guard on the line above: "the creator is an admin" must never decay into
/// "signing up grants admin", so this proves the flag stops at the tenant
/// boundary. Two people sign up, and neither can administer the other — which
/// is the whole tenancy promise, stated where the flag is granted.
#[tokio::test]
async fn a_signup_admin_administers_nobody_else() {
    let (store, identity) = setup().await;

    let mine = identity
        .provision_personal(
            &unique_domain("mine"),
            "Ada",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .expect("provisioned");
    let theirs = identity
        .provision_personal(
            &unique_domain("theirs"),
            "Grace",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .expect("provisioned");

    // Each is an admin at home.
    assert!(
        store
            .for_account(mine.tenant.clone(), mine.user.clone())
            .is_admin()
            .await
            .unwrap()
    );
    assert!(
        store
            .for_account(theirs.tenant.clone(), theirs.user.clone())
            .is_admin()
            .await
            .unwrap()
    );

    // Neither tenant is the other's, and one person's id carries no standing in
    // the other's tenant — the lookup is scoped by both, so it simply is not
    // them.
    assert_ne!(mine.tenant, theirs.tenant);
    assert!(
        !store
            .for_account(theirs.tenant.clone(), mine.user.clone())
            .is_admin()
            .await
            .unwrap(),
        "a signup admin reaches into another tenant"
    );
}

#[tokio::test]
async fn duplicate_address_is_taken_with_no_dangling_user() {
    let (store, identity) = setup().await;
    let domain = unique_domain("dup");

    let first = identity
        .provision_personal(
            &domain,
            "jane",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .expect("first claim");

    // A second claim of the same address is refused …
    let err = identity
        .provision_personal(
            &domain,
            "jane",
            "another-password-xyz",
            "recover@example.test",
        )
        .await
        .unwrap_err();
    assert_eq!(err, SignupError::AddressTaken);

    // … and crucially leaves NO second user row: inbound resolution stays
    // unambiguous (account_by_email refuses on 2+ matches, so a dangling user
    // would surface here as None). It must still resolve to the first account.
    let resolved = store
        .account_by_email(&format!("jane@{domain}"))
        .await
        .unwrap();
    assert_eq!(resolved, Some((first.tenant, first.user)));
}

#[tokio::test]
async fn reserved_and_invalid_addresses_are_refused() {
    let (store, identity) = setup().await;
    let domain = unique_domain("bad");

    assert_eq!(
        identity
            .provision_personal(
                &domain,
                "postmaster",
                "correct-horse-battery",
                "recover@example.test"
            )
            .await
            .unwrap_err(),
        SignupError::Reserved
    );
    assert_eq!(
        identity
            .provision_personal(
                &domain,
                "ab",
                "correct-horse-battery",
                "recover@example.test"
            )
            .await
            .unwrap_err(),
        SignupError::InvalidAddress
    );
    // Nothing was created for either.
    assert!(
        store
            .account_by_email(&format!("postmaster@{domain}"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn personal_users_are_isolated_tenants() {
    let (store, identity) = setup().await;
    let da = unique_domain("isoa");
    let db = unique_domain("isob");

    let a = identity
        .provision_personal(
            &da,
            "alice",
            "correct-horse-battery",
            "recover@example.test",
        )
        .await
        .unwrap();
    let b = identity
        .provision_personal(&db, "bob", "correct-horse-battery", "recover@example.test")
        .await
        .unwrap();

    // One tenant per person — never shared.
    assert_ne!(a.tenant.as_str(), b.tenant.as_str());

    // Each address resolves only to its own account.
    assert_eq!(
        store.account_by_email(&a.email).await.unwrap(),
        Some((a.tenant.clone(), a.user.clone()))
    );
    assert_eq!(
        store.account_by_email(&b.email).await.unwrap(),
        Some((b.tenant.clone(), b.user.clone()))
    );

    // Alice's mailboxes are not Bob's (distinct tenants → distinct data).
    let a_inbox = store
        .for_account(a.tenant.clone(), a.user.clone())
        .inbox()
        .await
        .unwrap();
    let b_inbox = store
        .for_account(b.tenant.clone(), b.user.clone())
        .inbox()
        .await
        .unwrap();
    assert_ne!(a_inbox.as_str(), b_inbox.as_str());
}
