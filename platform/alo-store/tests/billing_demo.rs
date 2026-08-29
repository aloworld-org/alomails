//! The development Billing corpus is real tenant data with a hard production
//! guard, deterministic ids, and relationships that survive every API read.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_demo::DemoEnvironment;

#[test]
fn production_and_remote_databases_cannot_mint_a_demo_permit() {
    assert!(
        DemoEnvironment::validate("postgres://alo:pw@127.0.0.1:5432/alo", "production").is_err()
    );
    assert!(
        DemoEnvironment::validate("postgres://alo:pw@db.example.com:5432/alo", "development")
            .is_err()
    );
    assert!(
        DemoEnvironment::validate(
            "postgres://alo:pw@db.example.com:5432/alo?note=@localhost:5432",
            "development"
        )
        .is_err()
    );
    assert!(
        DemoEnvironment::validate("postgres://alo:pw@127.0.0.1:5432/ficina", "development")
            .is_err()
    );
    assert!(
        DemoEnvironment::validate("postgres://alo:pw@127.0.0.1:5432/alo", "development").is_ok()
    );
}

#[tokio::test]
async fn seed_is_idempotent_connected_and_tenant_isolated() {
    let store = common::test_store().await;
    let (a, _, _) = common::fresh_account(&store, "billing-demo-a").await;
    let (b, _, _) = common::fresh_account(&store, "billing-demo-b").await;
    let permit = DemoEnvironment::validate(&common::database_url(), "test").unwrap();

    let first = a.seed_billing_demo(&permit).await.unwrap();
    let second = a.seed_billing_demo(&permit).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(first.customers, 100);
    assert_eq!(first.products, 100);
    assert_eq!(first.price_connections, 100);
    assert_eq!(first.quotes, 100);
    assert_eq!(first.invoices, 120);
    assert_eq!(first.schedules, 100);
    assert!(first.vat_source_invoices >= 100);
    assert!(first.quote_product_links >= 100);
    assert!(first.invoice_product_links >= 100);
    assert!(first.schedule_product_links >= 100);

    assert_eq!(a.billing_customers(true).await.unwrap().len(), 100);
    assert_eq!(a.billing_products(true).await.unwrap().len(), 100);
    assert_eq!(a.billing_price_connections().await.unwrap().len(), 100);
    assert_eq!(a.billing_quotes(None).await.unwrap().len(), 100);
    assert_eq!(a.billing_invoices(None).await.unwrap().len(), 120);
    assert_eq!(a.billing_schedules().await.unwrap().len(), 100);

    assert!(b.billing_customers(true).await.unwrap().is_empty());
    assert!(b.billing_price_connections().await.unwrap().is_empty());
    b.seed_billing_demo(&permit).await.unwrap();
    assert_eq!(a.billing_customers(true).await.unwrap().len(), 100);
    assert_eq!(b.billing_customers(true).await.unwrap().len(), 100);
}

#[tokio::test]
async fn demo_numbers_totals_vat_and_relationships_are_sound() {
    let store = common::test_store().await;
    let (account, _, _) = common::fresh_account(&store, "billing-demo-math").await;
    let permit = DemoEnvironment::validate(&common::database_url(), "test").unwrap();
    account.seed_billing_demo(&permit).await.unwrap();

    let customers = account.billing_customers(true).await.unwrap();
    let contacts = account.contacts().await.unwrap();
    let products = account.billing_products(true).await.unwrap();
    let quotes = account.billing_quotes(None).await.unwrap();
    let invoices = account.billing_invoices(None).await.unwrap();
    let schedules = account.billing_schedules().await.unwrap();
    let connections = account.billing_price_connections().await.unwrap();

    let quote_numbers: std::collections::HashSet<_> = quotes
        .iter()
        .filter_map(|q| q.quote.number.as_ref())
        .collect();
    let invoice_numbers: std::collections::HashSet<_> = invoices
        .iter()
        .filter_map(|i| i.invoice.number.as_ref())
        .collect();
    assert_eq!(
        quote_numbers.len(),
        quotes.iter().filter(|q| q.quote.number.is_some()).count()
    );
    assert_eq!(
        invoice_numbers.len(),
        invoices
            .iter()
            .filter(|i| i.invoice.number.is_some())
            .count()
    );
    assert!(
        quote_numbers
            .iter()
            .all(|number| number.starts_with("QUO-"))
    );
    assert!(
        invoice_numbers
            .iter()
            .all(|number| number.starts_with("INV-"))
    );

    let customer_ids: std::collections::HashSet<_> =
        customers.iter().map(|c| c.id.as_str()).collect();
    let contact_ids: std::collections::HashSet<_> =
        contacts.iter().map(|contact| contact.id.as_str()).collect();
    let product_ids: std::collections::HashSet<_> =
        products.iter().map(|p| p.id.as_str()).collect();
    assert!(
        quotes
            .iter()
            .all(|q| customer_ids.contains(q.quote.customer_id.as_str()))
    );
    assert_eq!(contacts.len(), 100);
    assert!(contacts.iter().all(|contact| !contact.phones.is_empty()));
    assert!(
        customers
            .iter()
            .filter_map(|customer| customer.contact_id.as_ref())
            .all(|contact| contact_ids.contains(contact.as_str()))
    );
    assert!(
        invoices
            .iter()
            .all(|i| customer_ids.contains(i.invoice.customer_id.as_str()))
    );
    assert!(
        schedules
            .iter()
            .all(|s| customer_ids.contains(s.schedule.customer_id.as_str()))
    );
    assert!(
        connections
            .iter()
            .flat_map(|c| c.product_ids.iter())
            .all(|id| product_ids.contains(id.as_str()))
    );
    for invoice in invoices.iter().take(20) {
        let full = account
            .billing_invoice(&invoice.invoice.id)
            .await
            .unwrap()
            .unwrap();
        let recomputed = alo_store::billing_totals::totals(
            &full
                .lines
                .iter()
                .map(|line| line.figures())
                .collect::<Vec<_>>(),
        );
        assert_eq!(full.totals, recomputed);
        assert_eq!(
            full.totals.gross_cents,
            full.totals.net_cents + full.totals.vat_cents
        );
    }
}

#[tokio::test]
async fn reset_removes_only_the_demo_namespace() {
    use alo_store::billing_customers::NewCustomer;
    use alo_store::billing_settings::NewBillingSettings;

    let store = common::test_store().await;
    let (account, _, _) = common::fresh_account(&store, "billing-demo-reset").await;
    let ordinary = account
        .create_billing_customer(&NewCustomer {
            name: "Kept Customer".to_owned(),
            country: "BE".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    account
        .save_billing_settings(&NewBillingSettings {
            legal_name: "Kept Issuer".to_owned(),
            country: "BE".to_owned(),
            base_currency: "EUR".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let permit = DemoEnvironment::validate(&common::database_url(), "test").unwrap();
    account.seed_billing_demo(&permit).await.unwrap();
    account.reset_billing_demo(&permit).await.unwrap();

    let customers = account.billing_customers(true).await.unwrap();
    assert_eq!(customers.len(), 1);
    assert_eq!(customers[0].id, ordinary);
    assert_eq!(
        account.billing_settings().await.unwrap().legal_name,
        "Kept Issuer"
    );
    assert!(account.contacts().await.unwrap().is_empty());
    assert!(
        account
            .billing_price_connections()
            .await
            .unwrap()
            .is_empty()
    );
}
