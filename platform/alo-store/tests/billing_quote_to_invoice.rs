//! Accepting a quote raises the invoice for it (alo Billing, wave B1.12).
//!
//! The done-when of the queue item is one sentence — *an accepted quote yields
//! an editable draft invoice with identical totals* — and it hides four claims
//! only a database can settle: that the copy is a copy (the offer's frozen
//! prices, in the offer's order, to the cent), that the two writes are **one
//! act** (no accepted quote without its invoice, and no invoice for an offer
//! that is still open), that the link back is single and permanent, and that
//! none of it crosses a tenant boundary.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::{
    AccountStore, BillingCustomerId, BillingQuoteId, InvoiceStatus, NewCustomer, NewLine, NewQuote,
    QuoteStatus, Store, StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A tenant with one user and one customer on 30-day terms in euro.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("q2i-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@quote2invoice.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    common::seed_default_chart(&account).await;
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            ..Default::default()
        })
        .await
        .unwrap();
    (account, tenant, customer)
}

async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// Three lines across two VAT rates, one of them a discount and one of them a
/// fractional quantity — a set whose totals only come out right if the copy is
/// exact and the arithmetic rounds once per rate.
fn offered_lines() -> Vec<NewLine> {
    vec![
        NewLine {
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 7_500,
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
        },
        NewLine {
            description: "Printed manual".to_owned(),
            unit: "piece".to_owned(),
            qty_milli: 3_000,
            unit_price_cents: 999,
            vat_rate_bp: 900,
        },
        NewLine {
            description: "Introductory discount".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: -1_000,
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
        },
    ]
}

/// The same offer as quote lines. **None of them names a catalog item**, which
/// is the whole point of this suite: consultancy, a printed manual charged in
/// words and a discount are services, so accepting must go on raising a draft
/// invoice directly (ADR 0054 §5). The day one of these grows a `product_id` is
/// the day this suite stops testing the services path.
fn offered_quote_lines() -> Vec<NewQuoteLine> {
    offered_lines()
        .into_iter()
        .map(NewQuoteLine::from)
        .collect()
}

/// A sent quote with the three lines above, and the reference the customer
/// will look for on both documents.
async fn sent_quote(account: &AccountStore, customer: &BillingCustomerId) -> BillingQuoteId {
    let id = account
        .create_billing_quote(&NewQuote {
            valid_days: Some(14),
            reference: "RFQ-2026-88".to_owned(),
            note: "This offer stands for a fortnight.".to_owned(),
            ..NewQuote::for_customer(customer.clone())
        })
        .await
        .unwrap();
    account
        .set_billing_quote_lines(&id, &offered_quote_lines())
        .await
        .unwrap();
    account.send_billing_quote(&id).await.unwrap();
    id
}

#[tokio::test]
async fn an_accepted_offer_becomes_an_editable_draft_worth_exactly_the_same() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "arc").await;
    let quote_id = sent_quote(&a, &customer).await;
    let offered = a.billing_quote(&quote_id).await.unwrap().unwrap();

    let accepted = a.accept_billing_quote(&quote_id).await.unwrap();
    assert_eq!(accepted.quote.quote.status, QuoteStatus::Accepted);
    assert_eq!(
        accepted.quote.quote.decided_date,
        Some(OffsetDateTime::now_utc().date()),
        "the day the offer stopped being open"
    );

    let invoice = a
        .billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
        )
        .await
        .unwrap()
        .expect("acceptance raised the invoice");

    // ---- the done-when: an editable draft worth the same -------------------
    assert_eq!(invoice.invoice.status, InvoiceStatus::Draft);
    assert!(invoice.invoice.number.is_none(), "a draft has no number");
    assert!(invoice.invoice.issue_date.is_none() && invoice.invoice.due_date.is_none());
    assert_eq!(invoice.totals.net_cents, offered.totals.net_cents);
    assert_eq!(invoice.totals.vat_cents, offered.totals.vat_cents);
    assert_eq!(invoice.totals.gross_cents, offered.totals.gross_cents);
    assert_eq!(
        invoice.totals.vat_by_rate, offered.totals.vat_by_rate,
        "including the breakdown per rate, not just the bottom line"
    );
    // Hand-computed, so a change to the arithmetic cannot silently agree with
    // itself on both documents: 7.5 h × €125 = €937.50, less 1 h = €812.50 at
    // 21 % (€170.63, rounded half away from zero), plus 3 × €9.99 = €29.97 at
    // 9 % (€2.70).
    assert_eq!(offered.totals.net_cents, 81_250 + 2_997);
    assert_eq!(invoice.totals.vat_cents, 17_063 + 270);
    assert_eq!(invoice.totals.gross_cents, 81_250 + 2_997 + 17_063 + 270);

    // ---- the lines are the offer's lines, in the offer's order -------------
    assert_eq!(invoice.lines.len(), 3);
    for (copy, original) in invoice.lines.iter().zip(offered.lines.iter()) {
        assert_eq!(copy.description, original.line.description);
        assert_eq!(copy.unit, original.line.unit);
        assert_eq!(copy.qty_milli, original.line.qty_milli);
        assert_eq!(copy.unit_price_cents, original.line.unit_price_cents);
        assert_eq!(copy.vat_rate_bp, original.line.vat_rate_bp);
        assert_eq!(copy.line_order, original.line.line_order);
        assert_ne!(
            copy.id.as_str(),
            original.line.id.as_str(),
            "a copied line is a line of its own, not a shadow of the offer's"
        );
    }
    assert_eq!(invoice.lines[2].qty_milli, -1_000, "a discount copies too");

    // ---- the header: copied where the offer decided it, current where it
    // ---- said nothing ------------------------------------------------------
    assert_eq!(invoice.invoice.customer_id.as_str(), customer.as_str());
    assert_eq!(invoice.invoice.currency, "EUR");
    assert_eq!(
        invoice.invoice.reference, "RFQ-2026-88",
        "the customer's own reference follows them onto the invoice"
    );
    assert!(
        invoice.invoice.note.is_empty(),
        "a quote's note states the terms of an offer; it is not true of a bill"
    );
    assert_eq!(
        invoice.invoice.payment_terms_days, 30,
        "a quote carries no payment terms: the customer's own are snapshotted"
    );
    assert!(!invoice.invoice.is_credit_note);

    // ---- the link back, in both directions ---------------------------------
    assert_eq!(
        invoice.invoice.quote_id.as_ref().map(|q| q.as_str()),
        Some(quote_id.as_str())
    );
    assert_eq!(
        a.billing_invoice_for_quote(&quote_id)
            .await
            .unwrap()
            .map(|i| i.as_str().to_owned()),
        Some(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice")
                .clone()
                .as_str()
                .to_owned()
        )
    );

    // ---- and it really is editable, and issues like any other draft --------
    let mut edited = offered_lines();
    edited.push(NewLine {
        description: "Travel".to_owned(),
        unit: "km".to_owned(),
        qty_milli: 120_000,
        unit_price_cents: 25,
        vat_rate_bp: 2100,
    });
    a.set_billing_invoice_lines(
        accepted
            .outcome
            .invoice_id()
            .expect("a services offer becomes an invoice"),
        &edited,
    )
    .await
    .unwrap();
    let issued = a
        .issue_billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
        )
        .await
        .unwrap();
    let year = OffsetDateTime::now_utc().date().year();
    assert_eq!(
        issued.invoice.number,
        Some(format!("INV-{year}-00001")),
        "the invoice series is untouched by the quote's own numbering"
    );
    assert_eq!(issued.lines.len(), 4);
    assert_eq!(
        issued.invoice.quote_id.as_ref().map(|q| q.as_str()),
        Some(quote_id.as_str()),
        "issuing keeps the document's origin"
    );
    assert_eq!(
        a.billing_quote(&quote_id)
            .await
            .unwrap()
            .unwrap()
            .lines
            .len(),
        3,
        "and editing the invoice never rewrote the offer"
    );
}

#[tokio::test]
async fn an_offer_is_billed_once_and_only_when_it_was_accepted() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "once").await;

    // ---- an offer nobody accepted raises nothing ---------------------------
    let draft = a
        .create_billing_quote(&NewQuote::for_customer(customer.clone()))
        .await
        .unwrap();
    assert_conflict(a.accept_billing_quote(&draft).await);
    assert!(a.billing_invoice_for_quote(&draft).await.unwrap().is_none());

    let declined = sent_quote(&a, &customer).await;
    a.decline_billing_quote(&declined).await.unwrap();
    assert!(
        a.billing_invoice_for_quote(&declined)
            .await
            .unwrap()
            .is_none(),
        "a turned-down offer is not billed"
    );

    let expired = sent_quote(&a, &customer).await;
    a.expire_billing_quote(&expired).await.unwrap();
    assert!(
        a.billing_invoice_for_quote(&expired)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        a.billing_invoices(None).await.unwrap().is_empty(),
        "three unaccepted offers, no documents"
    );

    // ---- accepted once, billed once ----------------------------------------
    let accepted_quote = sent_quote(&a, &customer).await;
    let first = a.accept_billing_quote(&accepted_quote).await.unwrap();
    let message = assert_conflict(a.accept_billing_quote(&accepted_quote).await);
    assert!(message.contains("accepted"), "{message}");
    assert_eq!(
        a.billing_invoices(None).await.unwrap().len(),
        1,
        "the refused second acceptance raised no second document"
    );
    assert_eq!(
        a.billing_invoice_for_quote(&accepted_quote)
            .await
            .unwrap()
            .map(|i| i.as_str().to_owned()),
        Some(
            first
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice")
                .as_str()
                .to_owned()
        )
    );

    // ---- a closed offer stays closed, and its invoice stands alone ---------
    for message in [
        assert_conflict(a.decline_billing_quote(&accepted_quote).await),
        assert_conflict(a.expire_billing_quote(&accepted_quote).await),
        assert_conflict(a.delete_billing_quote(&accepted_quote).await),
    ] {
        assert!(!message.is_empty());
    }
    // Deleting the *invoice* is allowed while it is a draft — it never
    // consumed a number — and it leaves the offer accepted, with its record of
    // what was agreed intact.
    a.delete_billing_invoice(
        first
            .outcome
            .invoice_id()
            .expect("a services offer becomes an invoice"),
    )
    .await
    .unwrap();
    assert_eq!(
        a.billing_quote(&accepted_quote)
            .await
            .unwrap()
            .unwrap()
            .quote
            .status,
        QuoteStatus::Accepted
    );
    assert!(
        a.billing_invoice_for_quote(&accepted_quote)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_offer_to_a_customer_since_archived_can_still_be_honoured() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "archived").await;
    let quote_id = sent_quote(&a, &customer).await;

    // The customer is archived after the offer was made — "we raise no new
    // business for them". Billing an offer they already accepted is not new
    // business, and refusing it would strand the acceptance with nothing to
    // invoice it with.
    a.set_billing_customer_archived(&customer, true)
        .await
        .unwrap();
    let accepted = a.accept_billing_quote(&quote_id).await.unwrap();
    let invoice = a
        .billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(invoice.invoice.customer_id.as_str(), customer.as_str());
    assert_eq!(invoice.lines.len(), 3);
    assert_eq!(
        invoice.totals.gross_cents,
        accepted.quote.totals.gross_cents
    );

    // A *new* offer to that customer is still refused — the rule that archiving
    // stops new business is unchanged.
    match a
        .create_billing_quote(&NewQuote::for_customer(customer.clone()))
        .await
    {
        Err(StoreError::Validation(message)) => assert!(message.contains("archived"), "{message}"),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[tokio::test]
async fn another_tenant_can_neither_accept_an_offer_nor_see_what_it_billed() {
    let store = common::test_store().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "own").await;
    let (b, _tenant_b, _customer_b) = tenant_with_customer(&store, "other").await;
    let pool = raw_pool().await;

    let quote_id = sent_quote(&a, &customer_a).await;

    // B holds A's id — the strongest position an attacker reaches — and every
    // door is the same clean denial as an id that never existed.
    assert_not_found(b.accept_billing_quote(&quote_id).await);
    assert!(
        b.billing_invoice_for_quote(&quote_id)
            .await
            .unwrap()
            .is_none(),
        "and the link read is not an existence oracle either"
    );
    assert!(b.billing_invoices(None).await.unwrap().is_empty());
    assert_eq!(
        a.billing_quote(&quote_id)
            .await
            .unwrap()
            .unwrap()
            .quote
            .status,
        QuoteStatus::Sent,
        "the refused acceptance changed nothing"
    );

    // The refusal wrote no invoice anywhere in the database, for either tenant.
    let raised: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_invoices WHERE quote_id = $1")
            .bind(quote_id.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(raised, 0);

    // A accepts their own offer; B still sees nothing, and the invoice sits in
    // A's tenant with A's quote.
    let accepted = a.accept_billing_quote(&quote_id).await.unwrap();
    assert!(b.billing_invoices(None).await.unwrap().is_empty());
    assert!(
        b.billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice")
        )
        .await
        .unwrap()
        .is_none(),
        "a foreign invoice id reads as absent, never as data"
    );
    assert_not_found(
        b.set_billing_invoice_lines(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
            &offered_lines(),
        )
        .await,
    );
    assert_not_found(
        b.issue_billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
        )
        .await,
    );
    assert_not_found(
        b.delete_billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice"),
        )
        .await,
    );

    let owner: Option<String> =
        sqlx::query_scalar("SELECT tenant_id FROM billing_invoices WHERE quote_id = $1")
            .bind(quote_id.as_str())
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(owner.as_deref(), Some(tenant_a.as_str()));
    assert_eq!(
        a.billing_invoice(
            accepted
                .outcome
                .invoice_id()
                .expect("a services offer becomes an invoice")
        )
        .await
        .unwrap()
        .unwrap()
        .lines
        .len(),
        3,
        "and B's attempts left A's document exactly as acceptance made it"
    );
}
