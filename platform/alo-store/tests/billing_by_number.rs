//! Finding a document by the name a person uses for it — its **number** (alo
//! Billing, wave B1.25).
//!
//! The billing agent is only ever given what the user said ("send a reminder
//! for INV-2026-00001"), never an opaque id, so the lookup that turns a spoken
//! number into something the store can act on is a tenancy boundary like any
//! other: a number is unique *within* a tenant and says nothing outside it.
//! Two tenants issuing on the same day hold the *same* number — that is what a
//! per-tenant gapless sequence means (B1.08) — which makes this the one lookup
//! where a leak would hand a stranger a real document rather than a `None`.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BillingCustomerId, NewCustomer, NewInvoice, NewLine, NewQuote, Store, TenantId,
};

/// A tenant with one user and one customer.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("num-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@bynumber.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
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
    (account, tenant, customer)
}

fn consulting() -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: 2_000,
        unit_price_cents: 12_000,
        vat_rate_bp: 1900,
    }
}

/// An issued invoice, and the number it was stamped with.
async fn issued_invoice(account: &AccountStore, customer: &BillingCustomerId) -> String {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&id, &[consulting()])
        .await
        .unwrap();
    account
        .issue_billing_invoice(&id)
        .await
        .unwrap()
        .invoice
        .number
        .expect("issuing assigns the number")
}

/// A sent quote, and the number it was stamped with.
async fn sent_quote(account: &AccountStore, customer: &BillingCustomerId) -> String {
    let id = account
        .create_billing_quote(&NewQuote::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_quote_lines(&id, &[consulting().into()])
        .await
        .unwrap();
    account
        .send_billing_quote(&id)
        .await
        .unwrap()
        .quote
        .number
        .expect("sending assigns the number")
}

#[tokio::test]
async fn a_document_is_found_by_its_number_and_only_within_its_own_tenant() {
    let store = common::test_store().await;
    let (a, t1, customer_a) = tenant_with_customer(&store, "a").await;
    let (b, t2, customer_b) = tenant_with_customer(&store, "b").await;

    let invoice_number = issued_invoice(&a, &customer_a).await;
    let quote_number = sent_quote(&a, &customer_a).await;
    // Both tenants number from their own sequence, so B's first invoice bears
    // the *same* number as A's — the reason this lookup has to be scoped.
    let b_invoice_number = issued_invoice(&b, &customer_b).await;
    assert_eq!(invoice_number, b_invoice_number);
    assert_eq!(invoice_number, format!("INV-{}-00001", year()));
    assert_eq!(quote_number, format!("QUO-{}-00001", year()));

    // ---- the lookup finds the tenant's own document ----------------------
    let found = a
        .billing_invoice_id_by_number(&invoice_number)
        .await
        .unwrap()
        .expect("A's own invoice, by the number A was given");
    let document = a.billing_invoice(&found).await.unwrap().unwrap();
    assert_eq!(document.invoice.number.as_deref(), Some(&*invoice_number));
    assert_eq!(document.invoice.customer_id, customer_a);
    assert_eq!(document.totals.net_cents, 24_000);

    let quote_id = a
        .billing_quote_id_by_number(&quote_number)
        .await
        .unwrap()
        .expect("A's own quote, by number");
    assert_eq!(
        a.billing_quote(&quote_id)
            .await
            .unwrap()
            .unwrap()
            .quote
            .number
            .as_deref(),
        Some(&*quote_number)
    );

    // ---- a number is how a person writes it ------------------------------
    for spelling in [
        format!("  {invoice_number}  "),
        invoice_number.to_lowercase(),
        format!("\t{}\n", invoice_number.to_lowercase()),
    ] {
        assert_eq!(
            a.billing_invoice_id_by_number(&spelling).await.unwrap(),
            Some(found.clone()),
            "{spelling:?} is the same document"
        );
    }
    // …but a prefix, a fragment or a near miss is not that document.
    let y = year();
    for miss in [
        format!("INV-{y}"),        // a prefix is not a document
        format!("{y}-00001"),      // nor a fragment
        format!("INV-{y}-0001"),   // nor the number under-padded
        format!("INV-{y}-00001X"), // nor with something appended
        format!("INV-{y}-00002"),  // the next number does not exist yet
        String::new(),
        "   ".to_owned(),
    ] {
        let miss = miss.as_str();
        assert!(
            a.billing_invoice_id_by_number(miss)
                .await
                .unwrap()
                .is_none(),
            "{miss:?} must not resolve"
        );
    }
    // A quote number is not an invoice number, and the reverse.
    assert!(
        a.billing_invoice_id_by_number(&quote_number)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        a.billing_quote_id_by_number(&invoice_number)
            .await
            .unwrap()
            .is_none()
    );

    // ---- the neighbour's number resolves to the neighbour's document -----
    // B asks for the number A was given. B has a document by that number, so
    // B gets B's — never A's.
    let b_found = b
        .billing_invoice_id_by_number(&invoice_number)
        .await
        .unwrap()
        .expect("B's own document of that number");
    assert_ne!(b_found, found, "the same number, two different documents");
    assert_eq!(
        b.billing_invoice(&b_found)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .customer_id,
        customer_b
    );
    // A's id is not reachable from B's door even when B holds it.
    assert!(b.billing_invoice(&found).await.unwrap().is_none());
    // And a number only A has (the quote) is nothing at all to B.
    assert!(
        b.billing_quote_id_by_number(&quote_number)
            .await
            .unwrap()
            .is_none()
    );

    // ---- a draft has no number, so it is unreachable this way -------------
    let draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    assert!(
        a.billing_invoice(&draft)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .number
            .is_none()
    );
    let draft_quote = a
        .create_billing_quote(&NewQuote::for_customer(customer_a.clone()))
        .await
        .unwrap();
    assert!(
        a.billing_quote(&draft_quote)
            .await
            .unwrap()
            .unwrap()
            .quote
            .number
            .is_none()
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

/// The current year, four digits — the year a document issued now is numbered
/// in ([`alo_store::billing_sequence`]).
fn year() -> String {
    time::OffsetDateTime::now_utc().year().to_string()
}
