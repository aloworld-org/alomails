//! Tenancy proof for quotation designs (Law 1: isolation is tested, not
//! assumed): the design of one tenant's quote can be neither read nor written
//! through another tenant's handle, both answering exactly as an id that never
//! existed. Plus the rules the store holds on its own: a design is a bounded
//! JSON object, a sent offer's design is frozen, and a deleted quote takes its
//! design with it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_quote_designs::QUOTE_DESIGN_MAX_BYTES;
use alo_store::{
    AccountStore, BillingCustomerId, BillingQuoteId, NewCustomer, NewLine, NewQuote, Store,
    StoreError,
};
use serde_json::json;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

async fn tenant_with_quote(store: &Store, tag: &str) -> (AccountStore, BillingQuoteId) {
    let tenant = store
        .create_tenant(&format!("qdesign-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@quote-designs.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    let customer: BillingCustomerId = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "BE".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap();
    let quote = account
        .create_billing_quote(&NewQuote::for_customer(customer))
        .await
        .unwrap();
    (account, quote)
}

#[tokio::test]
async fn a_design_round_trips_and_never_crosses_tenants() {
    let store = common::test_store().await;
    let (alpha, quote) = tenant_with_quote(&store, "alpha").await;
    let (beta, _) = tenant_with_quote(&store, "beta").await;

    // An undesigned quote answers "no design", not "no quote".
    assert_eq!(alpha.billing_quote_design(&quote).await.unwrap(), None);

    let design = json!({
        "theme": "modern",
        "blocks": [
            { "id": "h", "kind": "heading", "level": 2, "text": "Scope" },
            { "id": "p", "kind": "paragraph", "text": "<p>Three phases.</p>", "columns": 2 },
            { "id": "pricing-table", "kind": "pricing" }
        ],
        "colors": { "accent": "#e76f51" },
        "aFieldTheServerDoesNotKnow": { "kept": true }
    });
    alpha
        .set_billing_quote_design(&quote, &design)
        .await
        .unwrap();
    let stored = alpha.billing_quote_design(&quote).await.unwrap().unwrap();
    // Whole, including what the server does not understand: the client owns
    // the shape.
    assert_eq!(stored.design, design);

    // A second write replaces, never merges.
    let smaller = json!({ "blocks": [] });
    alpha
        .set_billing_quote_design(&quote, &smaller)
        .await
        .unwrap();
    assert_eq!(
        alpha
            .billing_quote_design(&quote)
            .await
            .unwrap()
            .unwrap()
            .design,
        smaller
    );

    // The other tenant can neither see nor write it — and cannot tell the
    // quote exists at all.
    assert_not_found(beta.billing_quote_design(&quote).await);
    assert_not_found(beta.set_billing_quote_design(&quote, &design).await);
    // …and alpha's design is untouched by the attempt.
    assert_eq!(
        alpha
            .billing_quote_design(&quote)
            .await
            .unwrap()
            .unwrap()
            .design,
        smaller
    );

    // Nor does a quote that never existed answer differently.
    assert_not_found(
        alpha
            .billing_quote_design(&BillingQuoteId::new("nope"))
            .await,
    );
}

#[tokio::test]
async fn a_design_is_a_bounded_json_object() {
    let store = common::test_store().await;
    let (account, quote) = tenant_with_quote(&store, "bounds").await;

    for not_an_object in [json!([1, 2]), json!("text"), json!(null), json!(7)] {
        match account
            .set_billing_quote_design(&quote, &not_an_object)
            .await
        {
            Err(StoreError::Validation(message)) => assert!(message.contains("JSON object")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    let too_big = json!({ "blob": "x".repeat(QUOTE_DESIGN_MAX_BYTES) });
    match account.set_billing_quote_design(&quote, &too_big).await {
        Err(StoreError::Validation(message)) => assert!(message.contains("bytes")),
        other => panic!("expected Validation, got {other:?}"),
    }
    assert_eq!(account.billing_quote_design(&quote).await.unwrap(), None);
}

#[tokio::test]
async fn a_sent_offer_freezes_its_design_and_a_deleted_one_takes_it_along() {
    let store = common::test_store().await;
    let (account, quote) = tenant_with_quote(&store, "frozen").await;
    let design = json!({ "blocks": [{ "id": "pricing-table", "kind": "pricing" }] });
    account
        .set_billing_quote_design(&quote, &design)
        .await
        .unwrap();

    // An offer is sent with something on it.
    account
        .set_billing_quote_lines(
            &quote,
            &[NewLine {
                description: "Consulting".to_owned(),
                unit: "hour".to_owned(),
                qty_milli: 2_000,
                unit_price_cents: 12_000,
                vat_rate_bp: 2100,
            }
            .into()],
        )
        .await
        .unwrap();
    account.send_billing_quote(&quote).await.unwrap();
    match account
        .set_billing_quote_design(&quote, &json!({ "blocks": [] }))
        .await
    {
        Err(StoreError::Conflict(message)) => assert!(message.contains("frozen")),
        other => panic!("expected Conflict, got {other:?}"),
    }
    // Reading the frozen design still works: it is what the customer received.
    assert_eq!(
        account
            .billing_quote_design(&quote)
            .await
            .unwrap()
            .unwrap()
            .design,
        design
    );

    let (account, draft) = tenant_with_quote(&store, "deleted").await;
    account
        .set_billing_quote_design(&draft, &design)
        .await
        .unwrap();
    account.delete_billing_quote(&draft).await.unwrap();
    assert_not_found(account.billing_quote_design(&draft).await);
}
