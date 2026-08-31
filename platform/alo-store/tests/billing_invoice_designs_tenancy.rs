//! Tenant isolation and lifecycle rules for invoice presentation snapshots.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingInvoiceId, NewCustomer, NewInvoice, NewLine, Store, StoreError,
};
use serde_json::json;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

async fn tenant_with_invoice(store: &Store, tag: &str) -> (AccountStore, BillingInvoiceId) {
    let tenant = store
        .create_tenant(&format!("idesign-{tag}"))
        .await
        .unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@invoice-designs.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant, user);
    common::seed_default_chart(&account).await;
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "DE".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 14,
            ..Default::default()
        })
        .await
        .unwrap();
    let invoice = account
        .create_billing_invoice(&NewInvoice::for_customer(customer))
        .await
        .unwrap();
    (account, invoice)
}

#[tokio::test]
async fn invoice_designs_round_trip_and_never_cross_tenants() {
    let store = common::test_store().await;
    let (alpha, invoice) = tenant_with_invoice(&store, "alpha").await;
    let (beta, _) = tenant_with_invoice(&store, "beta").await;
    let design = json!({ "colors": { "accent": "#e76f51" }, "blocks": [] });

    assert_eq!(alpha.billing_invoice_design(&invoice).await.unwrap(), None);
    alpha
        .set_billing_invoice_design(&invoice, &design)
        .await
        .unwrap();
    assert_eq!(
        alpha
            .billing_invoice_design(&invoice)
            .await
            .unwrap()
            .unwrap()
            .design,
        design
    );
    assert_not_found(beta.billing_invoice_design(&invoice).await);
    assert_not_found(beta.set_billing_invoice_design(&invoice, &json!({})).await);
}

#[tokio::test]
async fn issuing_an_invoice_freezes_its_design() {
    let store = common::test_store().await;
    let (account, invoice) = tenant_with_invoice(&store, "frozen").await;
    let design = json!({ "blocks": [{ "id": "pricing-table", "kind": "pricing" }] });
    account
        .set_billing_invoice_design(&invoice, &design)
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(
            &invoice,
            &[NewLine {
                description: "Consulting".to_owned(),
                unit: "hour".to_owned(),
                qty_milli: 1_000,
                unit_price_cents: 10_000,
                vat_rate_bp: 1900,
            }],
        )
        .await
        .unwrap();
    account.issue_billing_invoice(&invoice).await.unwrap();

    match account
        .set_billing_invoice_design(&invoice, &json!({ "blocks": [] }))
        .await
    {
        Err(StoreError::Conflict(message)) => assert!(message.contains("frozen")),
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(
        account
            .billing_invoice_design(&invoice)
            .await
            .unwrap()
            .unwrap()
            .design,
        design
    );
}
