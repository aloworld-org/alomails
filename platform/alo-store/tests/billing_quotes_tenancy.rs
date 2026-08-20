//! Tenancy proof for alo Billing quotes (Law 1: isolation is tested, not
//! assumed), plus the document arc the queue item requires: raise a draft,
//! replace its header, replace its line set, read the totals back.
//!
//! The wrong-tenant assertions cover **every** path a quote can be reached by —
//! read, list, header update, line replacement, delete, and all four lifecycle
//! transitions — because a document another tenant can move is worse than one
//! they can merely read.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::{
    AccountStore, BillingCustomerId, BillingQuoteId, NewCustomer, NewLine, NewQuote, QuoteStatus,
    Store, StoreError, TenantId,
};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

/// A tenant with one user and one customer, returning the account door, the
/// tenant id and that customer.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("quo-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@quotes.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "BE".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap();
    (account, tenant, customer)
}

fn consulting(hours_milli: i64) -> NewQuoteLine {
    consulting_line(hours_milli).into()
}

/// The same line as a plain billing line, for the sites that build one by hand.
fn consulting_line(hours_milli: i64) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: hours_milli,
        unit_price_cents: 12_000,
        vat_rate_bp: 2100,
    }
}

#[tokio::test]
async fn billing_quotes_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, _t1, customer_a) = tenant_with_customer(&store, "a").await;
    let (b, _t2, customer_b) = tenant_with_customer(&store, "b").await;

    // ---- raise a draft: unnumbered, undated, customer defaults snapshotted
    let id = a
        .create_billing_quote(&NewQuote::for_customer(customer_a.clone()))
        .await
        .unwrap();
    let doc = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(doc.quote.status, QuoteStatus::Draft);
    assert_eq!(doc.quote.currency, "EUR", "taken from the customer");
    assert_eq!(doc.quote.valid_days, 30, "the default validity");
    assert!(
        doc.quote.number.is_none()
            && doc.quote.sent_date.is_none()
            && doc.quote.valid_until.is_none()
            && doc.quote.decided_date.is_none(),
        "a draft was never offered to anybody"
    );
    assert_eq!(doc.quote.created_by, a.user().as_str());
    assert!(doc.lines.is_empty(), "a new draft has no lines");
    assert_eq!(doc.totals.net_cents, 0);
    assert_eq!(doc.totals.gross_cents, 0);
    assert!(doc.totals.vat_by_rate.is_empty());

    // ---- lines: written as a set, in the caller's order -------------------
    a.set_billing_quote_lines(
        &id,
        &[
            consulting(10_000),
            NewQuoteLine::from(NewLine {
                description: "Travel".to_owned(),
                unit: "km".to_owned(),
                qty_milli: 120_000,
                unit_price_cents: 42,
                vat_rate_bp: 600,
            }),
            NewQuoteLine::from(NewLine {
                description: "Introductory discount".to_owned(),
                qty_milli: -1_000,
                unit_price_cents: 12_000,
                vat_rate_bp: 2100,
                ..Default::default()
            }),
        ],
    )
    .await
    .unwrap();
    let doc = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(doc.lines.len(), 3);
    assert_eq!(
        doc.lines
            .iter()
            .map(|line| (line.line.line_order, line.line.description.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (0, "Consulting"),
            (1, "Travel"),
            (2, "Introductory discount")
        ],
        "print order is the caller's order, 0-based"
    );
    // 10 h × €120 = €1200, less 1 h = €1080 at 21 %; 120 km × €0.42 = €50.40
    // at 6 %. The money is the store's arithmetic, never the caller's.
    assert_eq!(doc.totals.net_cents, 108_000 + 5_040);
    assert_eq!(doc.lines[2].line.net_cents(), -12_000);
    let by_rate: Vec<(i32, i64, i64)> = doc
        .totals
        .vat_by_rate
        .iter()
        .map(|sub| (sub.rate_bp, sub.net_cents, sub.vat_cents))
        .collect();
    assert_eq!(by_rate, vec![(600, 5_040, 302), (2100, 108_000, 22_680)]);
    assert_eq!(doc.totals.gross_cents, 108_000 + 5_040 + 302 + 22_680);

    // ---- header replace ---------------------------------------------------
    a.update_billing_quote(
        &id,
        &NewQuote {
            valid_days: Some(7),
            reference: "RFQ-2026-14".to_owned(),
            note: "Prices exclude on-site work.".to_owned(),
            ..NewQuote::for_customer(customer_a.clone())
        },
    )
    .await
    .unwrap();
    let doc = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(doc.quote.valid_days, 7);
    assert_eq!(doc.quote.reference, "RFQ-2026-14");
    assert_eq!(doc.lines.len(), 3, "a header edit never touches the lines");
    assert_eq!(doc.totals.net_cents, 113_040);

    // ---- list: this tenant's quotes only ----------------------------------
    let listed = a.billing_quotes(None).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].quote.id.as_str(), id.as_str());
    assert_eq!(
        listed[0].totals.gross_cents, doc.totals.gross_cents,
        "a list entry is worth exactly what the document is worth"
    );
    assert!(
        b.billing_quotes(None).await.unwrap().is_empty(),
        "another tenant's list is empty, not a window"
    );

    // ---- the other tenant: every path is a clean denial --------------------
    assert!(
        b.billing_quote(&id).await.unwrap().is_none(),
        "a foreign id reads as absent, never as data"
    );
    assert_not_found(
        b.update_billing_quote(&id, &NewQuote::for_customer(customer_b.clone()))
            .await,
    );
    assert_not_found(b.set_billing_quote_lines(&id, &[consulting(1_000)]).await);
    assert_not_found(b.delete_billing_quote(&id).await);
    assert_not_found(b.send_billing_quote(&id).await);
    assert_not_found(b.accept_billing_quote(&id).await);
    assert_not_found(b.decline_billing_quote(&id).await);
    assert_not_found(b.expire_billing_quote(&id).await);

    // ---- and the attempts changed nothing ---------------------------------
    let after = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(after.quote.status, QuoteStatus::Draft);
    assert_eq!(after.quote.reference, "RFQ-2026-14");
    assert_eq!(after.lines.len(), 3);
    assert_eq!(after.totals.gross_cents, doc.totals.gross_cents);

    // ---- a quote can never be raised for another tenant's customer --------
    assert_not_found(
        a.create_billing_quote(&NewQuote::for_customer(customer_b.clone()))
            .await,
    );
    assert_not_found(
        a.update_billing_quote(&id, &NewQuote::for_customer(customer_b.clone()))
            .await,
    );
    let still = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(
        still.quote.customer_id.as_str(),
        customer_a.as_str(),
        "the refused move left the customer where it was"
    );

    // ---- an invented id is the same denial as a foreign one ---------------
    let invented = BillingQuoteId::generate();
    assert!(a.billing_quote(&invented).await.unwrap().is_none());
    assert_not_found(a.delete_billing_quote(&invented).await);
    assert_not_found(a.send_billing_quote(&invented).await);

    // ---- delete: a draft, and its lines with it ---------------------------
    a.delete_billing_quote(&id).await.unwrap();
    assert!(a.billing_quote(&id).await.unwrap().is_none());
    assert!(a.billing_quotes(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_quote_refuses_the_content_rules_every_billing_document_shares() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "rules").await;

    // An archived customer is not offered new business — but the message says
    // what to do about it rather than simply refusing.
    let archived_customer = a
        .create_billing_customer(&NewCustomer {
            name: "Former client".to_owned(),
            country: "NL".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    a.set_billing_customer_archived(&archived_customer, true)
        .await
        .unwrap();
    let message = assert_validation(
        a.create_billing_quote(&NewQuote::for_customer(archived_customer))
            .await,
    );
    assert!(message.contains("archived"), "{message}");

    // Validity is ranged, and the refusal names the rule, never the value.
    let message = assert_validation(
        a.create_billing_quote(&NewQuote {
            valid_days: Some(400),
            ..NewQuote::for_customer(customer.clone())
        })
        .await,
    );
    assert!(message.contains("validity"), "{message}");

    let id = a
        .create_billing_quote(&NewQuote::for_customer(customer.clone()))
        .await
        .unwrap();

    // A line set is validated as a whole before anything is written: a bad
    // line at the end leaves the document untouched, and the message names
    // which line without quoting the customer's data.
    a.set_billing_quote_lines(&id, &[consulting(1_000)])
        .await
        .unwrap();
    let message = assert_validation(
        a.set_billing_quote_lines(
            &id,
            &[
                consulting(2_000),
                NewQuoteLine::from(NewLine {
                    description: "Secret project".to_owned(),
                    qty_milli: i64::MAX,
                    ..consulting_line(0)
                }),
            ],
        )
        .await,
    );
    assert!(message.contains("line 2"), "{message}");
    assert!(!message.contains("Secret"), "{message}");
    let doc = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(doc.lines.len(), 1, "the rejected set wrote nothing");
    assert_eq!(doc.lines[0].line.qty_milli, 1_000);
}
