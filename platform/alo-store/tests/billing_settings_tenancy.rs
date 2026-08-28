//! Tenancy proof for the issuer identity behind every billing document
//! (Law 1: isolation is tested, not assumed).
//!
//! `billing_settings` is the one billing table keyed by the tenant alone, so
//! the isolation question is sharper than elsewhere: there is no id to guess
//! and no `NotFound` to return. What has to hold is that a tenant's save is
//! invisible to every other tenant and that a tenant which has never saved
//! reads its **own blanks** rather than anybody's data — including after a
//! neighbour has filled the table in.
//!
//! Also covers the round trip the queue item requires (blank → save → read →
//! replace), that the canonical forms survive the database, that a co-tenant
//! shares the identity (it is the tenant's, not the user's), and that a
//! tenant deletion purges the row.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_settings::NewBillingSettings;
use alo_store::{AccountStore, BillingSettings, Store, StoreError, TenantId, UserId};

/// A fully-specified identity, so the round-trip assertions cover every
/// column rather than the handful a smoke test would touch.
fn dutch_issuer() -> NewBillingSettings {
    NewBillingSettings {
        legal_name: "Alo Werkplaats B.V.".to_owned(),
        address_line1: "Keizersgracht 1".to_owned(),
        address_line2: "Unit 4".to_owned(),
        postal_code: "1015 CJ".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "nl".to_owned(),
        vat_id: Some("nl 8123.45.678.B01".to_owned()),
        registration_no: "KVK 90123456".to_owned(),
        email: "billing@alo.test".to_owned(),
        phone: "+31 20 123 4567".to_owned(),
        website: "alo.test".to_owned(),
        iban: Some("nl91 abna 0417 1643 00".to_owned()),
        bic: Some("abnanl2a".to_owned()),
        bank_name: "ABN AMRO".to_owned(),
        account_holder: "Alo Werkplaats".to_owned(),
        footer_note: "Payable within the stated terms.".to_owned(),
        base_currency: "eur".to_owned(),
    }
}

/// A tenant with one user, returning the account door, the tenant id and the
/// user id (which is what `updated_by` records — not the address).
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId, UserId) {
    let tenant = store.create_tenant(&format!("set-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@settings.test"))
        .await
        .unwrap();
    (
        store.for_account(tenant.clone(), user.clone()),
        tenant,
        user,
    )
}

#[tokio::test]
async fn billing_settings_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1, ua) = tenant_with_user(&store, "a").await;
    // A co-tenant user of the same tenant: the identity is the tenant's.
    let uc = store
        .for_tenant(t1.clone())
        .create_user("c@settings.test")
        .await
        .unwrap();
    let c = store.for_account(t1.clone(), uc.clone());
    let (b, _t2, _ub) = tenant_with_user(&store, "b").await;

    // ---- unstated: the blanks, never a NotFound ---------------------------
    let blank = a.billing_settings().await.unwrap();
    assert_eq!(blank, BillingSettings::default());
    assert!(!blank.is_stated());
    assert!(blank.legal_name.is_empty() && blank.iban.is_none());

    // ---- save: canonicalised on the way in, exact on the way out ----------
    let saved = a.save_billing_settings(&dutch_issuer()).await.unwrap();
    assert!(saved.is_stated());
    assert_eq!(saved.legal_name, "Alo Werkplaats B.V.");
    assert_eq!(saved.country, "NL");
    assert_eq!(saved.vat_id.as_deref(), Some("NL812345678B01"));
    assert_eq!(saved.iban.as_deref(), Some("NL91ABNA0417164300"));
    assert_eq!(saved.bic.as_deref(), Some("ABNANL2A"));
    assert_eq!(saved.effective_account_holder(), "Alo Werkplaats");
    assert_eq!(saved.updated_by.as_deref(), Some(ua.as_str()));

    // What the writer got back is what a fresh read gets.
    assert_eq!(a.billing_settings().await.unwrap(), saved);

    // ---- the identity is the TENANT's, shared by its users ----------------
    assert_eq!(c.billing_settings().await.unwrap(), saved);

    // ---- the neighbour is untouched, and still reads its own blanks -------
    let neighbour = b.billing_settings().await.unwrap();
    assert_eq!(neighbour, BillingSettings::default());
    assert!(!neighbour.is_stated());

    // ---- and its own save does not reach back ----------------------------
    let other = b
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Nachbar GmbH".to_owned(),
            country: "DE".to_owned(),
            iban: Some("DE89370400440532013000".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(other.legal_name, "Nachbar GmbH");
    assert_eq!(a.billing_settings().await.unwrap(), saved);
    assert_eq!(
        a.billing_settings().await.unwrap().iban.as_deref(),
        Some("NL91ABNA0417164300"),
        "tenant A must never read tenant B's bank account"
    );

    // ---- replace: a save is a full replace, by the co-tenant this time ----
    let replaced = c
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            city: "Rotterdam".to_owned(),
            country: "NL".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(replaced.city, "Rotterdam");
    // Every field the second save did not state is now blank — this is a
    // replace, not a merge; the route layer is what merges a PATCH.
    assert_eq!(replaced.address_line1, "");
    assert_eq!(replaced.vat_id, None);
    assert_eq!(replaced.iban, None);
    // The record says who last saved it, and the co-tenant is not A.
    assert_eq!(replaced.updated_by.as_deref(), Some(uc.as_str()));
    assert_ne!(replaced.updated_by, saved.updated_by);
    // No stated holder now: the legal name is who the account is in.
    assert_eq!(replaced.effective_account_holder(), "Alo Werkplaats B.V.");
    // Still exactly one row per tenant, and still nothing of B's in it.
    assert_eq!(a.billing_settings().await.unwrap(), replaced);
    assert_eq!(b.billing_settings().await.unwrap(), other);
}

#[tokio::test]
async fn billing_settings_refuse_what_a_document_could_not_print() {
    let store = common::test_store().await;
    let (a, _t, _u) = tenant_with_user(&store, "v").await;

    // A document that does not name its issuer is not an invoice.
    let blank_name = a
        .save_billing_settings(&NewBillingSettings {
            legal_name: "   ".to_owned(),
            ..Default::default()
        })
        .await;
    assert!(matches!(blank_name, Err(StoreError::Validation(_))));

    // An IBAN that fails its mod-97 check is money that never arrives.
    let bad_iban = a
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Alo".to_owned(),
            iban: Some("NL92ABNA0417164300".to_owned()),
            ..Default::default()
        })
        .await;
    assert!(matches!(bad_iban, Err(StoreError::Validation(_))));

    // A refused save leaves the tenant unstated: nothing was half-written.
    assert!(!a.billing_settings().await.unwrap().is_stated());
}

#[tokio::test]
async fn deleting_a_tenant_purges_its_issuer_identity() {
    let store = common::test_store().await;
    let (a, tenant, _u) = tenant_with_user(&store, "purge").await;
    a.save_billing_settings(&dutch_issuer()).await.unwrap();

    store.delete_tenant(&tenant).await.unwrap();

    // Read the row directly: the claim is that it was cascaded away, not
    // merely hidden behind the tenant predicate of the read.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_settings WHERE tenant_id = $1")
            .bind(tenant.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0, "the issuer identity must not outlive its tenant");
}
