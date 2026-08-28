//! The quote lifecycle (alo Billing, wave B1): `draft → sent → accepted |
//! declined | expired`, on the real wire.
//!
//! The pure transition table is unit-tested over all twenty-five ordered pairs
//! in `billing_quotes.rs`. What these tests prove is the part only a database
//! can answer: that sending stamps a number and the dates from the tenant's
//! quote series, that the stored row and the guards agree about what may
//! happen next, that a frozen quote refuses every write, and that a quote's
//! numbers are drawn from a series of their own — an unaccepted offer must
//! never leave a hole in the invoice series.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_quote_lines::NewQuoteLine;
use alo_store::{
    AccountStore, BillingCustomerId, BillingQuoteId, InvoiceStatus, NewCustomer, NewInvoice,
    NewLine, NewQuote, QuoteStatus, Store, StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};

/// Asserts a result is the typed lifecycle refusal, returning its message.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

fn assert_validation<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got: {other:?}"),
    }
}

async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("qlife-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@quotelife.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
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
    (account, tenant, customer)
}

/// A charge in words: consultancy names no catalog item, which is what makes
/// every offer in this suite the **services** path — the one that must go on
/// becoming a draft invoice directly (ADR 0054 §5).
fn consulting(hours_milli: i64) -> NewQuoteLine {
    consulting_line(hours_milli).into()
}

/// The same line as a plain billing line, for the few places that build one by
/// hand to break a field rule.
fn consulting_line(hours_milli: i64) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: hours_milli,
        unit_price_cents: 10_000,
        vat_rate_bp: 1900,
    }
}

/// A raw pool alongside the store, for reading columns the store's own reads
/// would not surface and for ageing a quote past its validity.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// A draft with one line — the smallest quote that can legitimately be sent.
async fn drafted(account: &AccountStore, customer: &BillingCustomerId) -> BillingQuoteId {
    let id = account
        .create_billing_quote(&NewQuote {
            valid_days: Some(14),
            ..NewQuote::for_customer(customer.clone())
        })
        .await
        .unwrap();
    account
        .set_billing_quote_lines(&id, &[consulting(3_000)])
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn sending_numbers_and_dates_a_quote_and_freezes_it() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "send").await;
    let id = drafted(&a, &customer).await;

    let sent = a.send_billing_quote(&id).await.unwrap();
    assert_eq!(sent.quote.status, QuoteStatus::Sent);
    let number = sent.quote.number.clone().expect("sending assigns a number");
    let today = OffsetDateTime::now_utc().date();
    assert_eq!(
        number,
        format!("QUO-{}-00001", today.year()),
        "the first quote of this tenant's year"
    );
    let sent_date = sent.quote.sent_date.expect("sending stamps the day");
    assert_eq!(sent_date, today, "the database's clock, not the caller's");
    assert_eq!(
        sent.quote.valid_until,
        Some(sent_date + Duration::days(14)),
        "the validity comes from the days snapshotted on the document"
    );
    assert!(
        sent.quote.decided_date.is_none(),
        "an open offer has no decision date"
    );
    assert!(!sent.quote.is_expired(today), "it stands for a fortnight");
    assert!(
        sent.quote.is_expired(sent_date + Duration::days(15)),
        "and has lapsed the day after"
    );
    assert_eq!(sent.totals.gross_cents, 30_000 + 5_700);

    // ---- frozen: every write path refuses, naming the state ---------------
    for message in [
        assert_conflict(
            a.update_billing_quote(
                &id,
                &NewQuote {
                    reference: "RFQ-9".to_owned(),
                    ..NewQuote::for_customer(customer.clone())
                },
            )
            .await,
        ),
        assert_conflict(a.set_billing_quote_lines(&id, &[consulting(9_000)]).await),
        assert_conflict(a.set_billing_quote_lines(&id, &[]).await),
        assert_conflict(a.delete_billing_quote(&id).await),
    ] {
        assert!(message.contains("sent"), "{message}");
    }
    // A frozen quote refuses the edit whatever the payload says: the state is
    // the reason, so it outranks any complaint about content.
    assert_conflict(
        a.set_billing_quote_lines(
            &id,
            &[NewQuoteLine::from(NewLine {
                description: "   ".to_owned(),
                ..consulting_line(1_000)
            })],
        )
        .await,
    );

    // ---- re-sending is refused, not a quiet no-op -------------------------
    let message = assert_conflict(a.send_billing_quote(&id).await);
    assert!(message.contains("sent"), "{message}");
    let again = a.billing_quote(&id).await.unwrap().unwrap();
    assert_eq!(
        again.quote.number,
        Some(number),
        "and the refusal drew no second number"
    );
    assert_eq!(again.lines.len(), 1);
}

#[tokio::test]
async fn a_quote_with_nothing_in_it_cannot_be_sent() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "empty").await;
    let pool = raw_pool().await;

    let empty = a
        .create_billing_quote(&NewQuote::for_customer(customer.clone()))
        .await
        .unwrap();
    let message = assert_validation(a.send_billing_quote(&empty).await);
    assert!(message.contains("no lines"), "{message}");
    assert_eq!(
        a.billing_quote(&empty).await.unwrap().unwrap().quote.status,
        QuoteStatus::Draft,
        "the refusal left it a draft"
    );

    // The refused send drew nothing: the series is untouched, so the next real
    // quote is still number one.
    let drawn: Option<i64> = sqlx::query_scalar(
        "SELECT next_value FROM billing_sequences \
         WHERE tenant_id = $1 AND kind = 'quote'",
    )
    .bind(tenant.as_str())
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(drawn.is_none(), "a series never drawn from has no row");

    let real = drafted(&a, &customer).await;
    let sent = a.send_billing_quote(&real).await.unwrap();
    assert!(
        sent.quote
            .number
            .as_deref()
            .is_some_and(|n| n.ends_with("-00001")),
        "the abandoned draft left no hole: {:?}",
        sent.quote.number
    );
}

#[tokio::test]
async fn an_open_offer_can_be_answered_exactly_once_and_each_way() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "answer").await;

    // Each closing state, reached from `sent`, on a document of its own.
    let accepted_id = drafted(&a, &customer).await;
    a.send_billing_quote(&accepted_id).await.unwrap();
    // Acceptance also raises the draft invoice for the offer (B1.12); the
    // document it produces is proved in `billing_quote_to_invoice`.
    let accepted = a.accept_billing_quote(&accepted_id).await.unwrap().quote;
    assert_eq!(accepted.quote.status, QuoteStatus::Accepted);

    let declined_id = drafted(&a, &customer).await;
    a.send_billing_quote(&declined_id).await.unwrap();
    let declined = a.decline_billing_quote(&declined_id).await.unwrap();
    assert_eq!(declined.quote.status, QuoteStatus::Declined);

    let expired_id = drafted(&a, &customer).await;
    a.send_billing_quote(&expired_id).await.unwrap();
    let expired = a.expire_billing_quote(&expired_id).await.unwrap();
    assert_eq!(expired.quote.status, QuoteStatus::Expired);

    let today = OffsetDateTime::now_utc().date();
    for closed in [&accepted, &declined, &expired] {
        assert_eq!(
            closed.quote.decided_date,
            Some(today),
            "closing stamps the day the offer stopped being open"
        );
        assert!(
            closed.quote.number.is_some() && closed.quote.sent_date.is_some(),
            "closing keeps the number and the send date"
        );
        assert_eq!(closed.lines.len(), 1, "and the document itself");
        assert!(
            !closed.quote.is_expired(today + Duration::days(365)),
            "a closed offer has its answer; it does not go on lapsing"
        );
    }

    // ---- closed is closed: every further move is refused ------------------
    for id in [&accepted_id, &declined_id, &expired_id] {
        for message in [
            assert_conflict(a.send_billing_quote(id).await),
            assert_conflict(a.accept_billing_quote(id).await),
            assert_conflict(a.decline_billing_quote(id).await),
            assert_conflict(a.expire_billing_quote(id).await),
            assert_conflict(a.delete_billing_quote(id).await),
        ] {
            assert!(!message.is_empty(), "the refusal always says why");
        }
    }
    assert_eq!(
        a.billing_quote(&accepted_id)
            .await
            .unwrap()
            .unwrap()
            .quote
            .status,
        QuoteStatus::Accepted,
        "and none of those attempts changed anything"
    );

    // ---- a draft cannot be answered: it was never an offer ----------------
    let draft_id = drafted(&a, &customer).await;
    for message in [
        assert_conflict(a.accept_billing_quote(&draft_id).await),
        assert_conflict(a.decline_billing_quote(&draft_id).await),
        assert_conflict(a.expire_billing_quote(&draft_id).await),
    ] {
        assert!(message.contains("draft"), "{message}");
        assert!(
            message.contains("sent"),
            "a draft can only be sent: {message}"
        );
    }

    // ---- the status filter answers from the stored state ------------------
    let sent_only = a.billing_quotes(Some(QuoteStatus::Sent)).await.unwrap();
    assert!(sent_only.is_empty(), "all three offers were answered");
    let drafts = a.billing_quotes(Some(QuoteStatus::Draft)).await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].quote.id.as_str(), draft_id.as_str());
    assert_eq!(
        a.billing_quotes(Some(QuoteStatus::Accepted))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(a.billing_quotes(None).await.unwrap().len(), 4);
}

#[tokio::test]
async fn a_lapsed_offer_is_readable_as_lapsed_and_may_still_be_honoured() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "lapsed").await;
    let pool = raw_pool().await;

    let id = drafted(&a, &customer).await;
    a.send_billing_quote(&id).await.unwrap();

    // Age it past its validity — the only thing the calendar would have done:
    // sent seventeen days ago, so its fourteen days ran out three days back.
    // (Moving the dates as a pair is not optional: the table's own CHECK
    // refuses an offer that expires before it was made.)
    sqlx::query(
        "UPDATE billing_quotes \
            SET sent_date = CURRENT_DATE - 17, valid_until = CURRENT_DATE - 3 \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .execute(&pool)
    .await
    .unwrap();

    let today = OffsetDateTime::now_utc().date();
    let lapsed = a.billing_quote(&id).await.unwrap().unwrap();
    assert!(
        lapsed.quote.is_expired(today),
        "the reader is told it has lapsed without anything having run"
    );
    assert_eq!(
        lapsed.quote.status,
        QuoteStatus::Sent,
        "no background sweep closed it behind the tenant's back"
    );

    // Honouring it a few days late is the tenant's decision to make: the store
    // refuses on state, never on a date it read for itself.
    let accepted = a.accept_billing_quote(&id).await.unwrap().quote;
    assert_eq!(accepted.quote.status, QuoteStatus::Accepted);
    assert!(!accepted.quote.is_expired(today));
}

#[tokio::test]
async fn quotes_and_invoices_count_in_separate_series() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "series").await;
    let pool = raw_pool().await;

    // Two quotes and one invoice, interleaved.
    let first = drafted(&a, &customer).await;
    let sent_first = a.send_billing_quote(&first).await.unwrap();

    let invoice = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&invoice, &[consulting_line(1_000)])
        .await
        .unwrap();
    let issued = a.issue_billing_invoice(&invoice).await.unwrap();

    let second = drafted(&a, &customer).await;
    let sent_second = a.send_billing_quote(&second).await.unwrap();

    let year = OffsetDateTime::now_utc().date().year();
    assert_eq!(sent_first.quote.number, Some(format!("QUO-{year}-00001")));
    assert_eq!(sent_second.quote.number, Some(format!("QUO-{year}-00002")));
    assert_eq!(
        issued.invoice.number,
        Some(format!("INV-{year}-00001")),
        "the quotes in between took nothing from the invoice series"
    );
    assert_eq!(issued.invoice.status, InvoiceStatus::Issued);

    let counters: Vec<(String, i64)> = sqlx::query_as(
        "SELECT kind, next_value FROM billing_sequences WHERE tenant_id = $1 ORDER BY kind",
    )
    .bind(tenant.as_str())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        counters,
        vec![("invoice".to_owned(), 2), ("quote".to_owned(), 3)],
        "one row per series, each counting alone"
    );

    // The two documents live in separate tables and neither list shows the
    // other's rows.
    assert_eq!(a.billing_quotes(None).await.unwrap().len(), 2);
    assert_eq!(a.billing_invoices(None).await.unwrap().len(), 1);
}
