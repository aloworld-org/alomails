//! Issuing an invoice (alo Billing, wave B1): the transition that draws a
//! number from the tenant's legally gapless series, dates the document and
//! freezes it — and voiding, the only way out of an issued document that does
//! not create a second one.
//!
//! The centre of this suite is the concurrency proof. Gapless numbering is a
//! legal requirement (§14 UStG and its EU equivalents), so "two parallel
//! issues never share or skip a number" cannot be an argument about how the
//! SQL looks: it is a hundred issues fired at once against the real Postgres,
//! with the resulting numbers compared against the exact set they must be.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, INVOICE_NUMBER_PREFIX,
    INVOICE_SEQUENCE_KIND, InvoiceStatus, NewCustomer, NewInvoice, NewLine, Store, StoreError,
    TenantId, document_number,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::Date;

/// The payment terms every customer in this suite is created with, so a due
/// date is checkable against the issue date rather than against a default.
const TERMS_DAYS: i64 = 21;

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

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
    let tenant = store.create_tenant(&format!("issue-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@issue.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    common::seed_default_chart(&account).await;
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: i32::try_from(TERMS_DAYS).unwrap(),
            ..Default::default()
        })
        .await
        .unwrap();
    (account, tenant, customer)
}

fn consulting(hours_milli: i64) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: hours_milli,
        unit_price_cents: 10_000,
        vat_rate_bp: 2100,
    }
}

/// A raw pool alongside the store, for reading the counter row and for driving
/// a transaction the store's API does not expose.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The database's own today — the clock issuing uses, which is not necessarily
/// this process's UTC date.
async fn today(pool: &PgPool) -> Date {
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The counter's current state: the number the next document would take, or
/// `None` while the series has never been drawn from.
async fn next_value(pool: &PgPool, tenant: &TenantId, year: i32) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT next_value FROM billing_sequences \
         WHERE tenant_id = $1 AND kind = $2 AND year = $3",
    )
    .bind(tenant.as_str())
    .bind(INVOICE_SEQUENCE_KIND)
    .bind(year)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// A draft with one line, ready to be issued.
async fn draft_with_a_line(a: &AccountStore, customer: &BillingCustomerId) -> BillingInvoiceId {
    let id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&id, &[consulting(3_000)])
        .await
        .unwrap();
    id
}

/// Issuing assigns the first number of the year, dates the document from the
/// database's clock, and freezes it against every write path — including a
/// second issue.
#[tokio::test]
async fn issuing_numbers_dates_and_freezes_the_document() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "first").await;
    let day = today(&pool).await;

    let id = draft_with_a_line(&a, &customer).await;
    let before = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(before.invoice.status, InvoiceStatus::Draft);
    assert!(before.invoice.number.is_none(), "a draft carries no number");
    assert!(
        next_value(&pool, &tenant, day.year()).await.is_none(),
        "a series that has never been drawn from has no row"
    );

    let issued = a.issue_billing_invoice(&id).await.unwrap();

    assert_eq!(issued.invoice.status, InvoiceStatus::Issued);
    assert_eq!(
        issued.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 1).as_str()),
        "the first document of the year is number 1"
    );
    assert_eq!(
        issued.invoice.issue_date,
        Some(day),
        "dated today, not by us"
    );
    assert_eq!(
        issued.invoice.due_date,
        Some(day + time::Duration::days(TERMS_DAYS)),
        "the due date follows the terms snapshotted on the document"
    );
    // The document itself is untouched by issuing: same lines, same money.
    assert_eq!(issued.lines.len(), 1);
    assert_eq!(issued.totals, before.totals);
    assert_eq!(next_value(&pool, &tenant, day.year()).await, Some(2));

    // ---- frozen, and it says so ------------------------------------------
    let again = assert_conflict(a.issue_billing_invoice(&id).await);
    assert!(again.contains("issued"), "{again}");
    assert!(again.contains("draft"), "{again}");
    for message in [
        assert_conflict(
            a.update_billing_invoice(&id, &NewInvoice::for_customer(customer.clone()))
                .await,
        ),
        assert_conflict(a.set_billing_invoice_lines(&id, &[consulting(9_000)]).await),
        assert_conflict(a.delete_billing_invoice(&id).await),
    ] {
        assert!(message.contains("issued"), "{message}");
    }

    // ---- and the series carries on ---------------------------------------
    let second = draft_with_a_line(&a, &customer).await;
    let second = a.issue_billing_invoice(&second).await.unwrap();
    assert_eq!(
        second.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 2).as_str())
    );
    // Nothing about the first document moved when the second was issued.
    let first_again = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(first_again.invoice.number, issued.invoice.number);
    assert_eq!(first_again.invoice.updated_at, issued.invoice.updated_at);

    store.delete_tenant(&tenant).await.unwrap();
}

/// A refused issue spends nothing. An empty document is not a document, and
/// the number it would have taken is still there for the next real one.
#[tokio::test]
async fn an_invoice_with_no_lines_never_consumes_a_number() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "empty").await;
    let day = today(&pool).await;

    let empty = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    let message = assert_validation(a.issue_billing_invoice(&empty).await);
    assert!(message.contains("no lines"), "{message}");

    // Not a `Conflict`: the caller can fix this by adding a line, which is
    // what `422` means at the route edge, while `409` means "the state is
    // wrong". The document is still an editable draft.
    assert_eq!(
        a.billing_invoice(&empty)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .status,
        InvoiceStatus::Draft
    );
    assert!(
        next_value(&pool, &tenant, day.year()).await.is_none(),
        "the counter was never even created"
    );

    // The next real document takes number 1 — nothing was burned.
    let real = draft_with_a_line(&a, &customer).await;
    let issued = a.issue_billing_invoice(&real).await.unwrap();
    assert_eq!(
        issued.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 1).as_str())
    );

    // And the same invoice becomes issuable once it says something.
    a.set_billing_invoice_lines(&empty, &[consulting(1_000)])
        .await
        .unwrap();
    let now_issued = a.issue_billing_invoice(&empty).await.unwrap();
    assert_eq!(
        now_issued.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 2).as_str())
    );

    store.delete_tenant(&tenant).await.unwrap();
}

/// The item's gate: a hundred issues fired concurrently against one tenant's
/// series produce exactly the numbers 1..=100 — none shared, none skipped.
///
/// Sharing would mean two legal documents with one number; skipping would mean
/// a hole a tax inspection reads as a deleted invoice. Both are failures of
/// the same test.
#[tokio::test]
async fn a_hundred_parallel_issues_never_share_or_skip_a_number() {
    const ISSUES: usize = 100;

    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "parallel").await;
    let day = today(&pool).await;

    let mut drafts = Vec::with_capacity(ISSUES);
    for _ in 0..ISSUES {
        drafts.push(draft_with_a_line(&a, &customer).await);
    }

    // All of them at once. They contend on one counter row, so this is the
    // real race, not a staggered sequence of calls.
    let mut running = Vec::with_capacity(ISSUES);
    for id in drafts {
        let account = a.clone();
        running.push(tokio::spawn(async move {
            account.issue_billing_invoice(&id).await
        }));
    }

    let mut numbers = BTreeSet::new();
    for task in running {
        let issued = task.await.unwrap().unwrap();
        let number = issued
            .invoice
            .number
            .expect("an issued document is numbered");
        assert!(numbers.insert(number.clone()), "number {number} was shared");
    }

    let expected: BTreeSet<String> = (1..=ISSUES as i64)
        .map(|value| document_number(INVOICE_NUMBER_PREFIX, day.year(), value))
        .collect();
    assert_eq!(numbers, expected, "the series must be exactly 1..={ISSUES}");
    assert_eq!(
        next_value(&pool, &tenant, day.year()).await,
        Some(ISSUES as i64 + 1),
        "and the counter agrees with what was handed out"
    );

    // Read back from the table rather than from the returned documents: the
    // unique index on (tenant_id, number) is the last line of defence.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT number) FROM billing_invoices \
         WHERE tenant_id = $1 AND status = 'issued'",
    )
    .bind(tenant.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, ISSUES as i64);

    store.delete_tenant(&tenant).await.unwrap();
}

/// The property a Postgres `SEQUENCE` cannot give us, and the reason the
/// counter is a row: a transaction that draws a number and then fails gives
/// the number back.
#[tokio::test]
async fn a_rolled_back_draw_gives_its_number_back() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "rollback").await;
    let day = today(&pool).await;

    // The same upsert `billing_sequence::draw_next` runs, in a transaction
    // that then fails — standing in for an issue that dies after drawing.
    let mut doomed = pool.begin().await.unwrap();
    let drawn: i64 = sqlx::query_scalar(
        "INSERT INTO billing_sequences AS s (tenant_id, kind, year, next_value) \
         VALUES ($1, $2, $3, 2) \
         ON CONFLICT (tenant_id, kind, year) \
         DO UPDATE SET next_value = s.next_value + 1, updated_at = now() \
         RETURNING s.next_value - 1",
    )
    .bind(tenant.as_str())
    .bind(INVOICE_SEQUENCE_KIND)
    .bind(day.year())
    .fetch_one(&mut *doomed)
    .await
    .unwrap();
    assert_eq!(drawn, 1);
    doomed.rollback().await.unwrap();

    assert!(
        next_value(&pool, &tenant, day.year()).await.is_none(),
        "the counter row went with the rolled-back transaction"
    );
    let id = draft_with_a_line(&a, &customer).await;
    let issued = a.issue_billing_invoice(&id).await.unwrap();
    assert_eq!(
        issued.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 1).as_str()),
        "a real invoice takes the number the failed attempt had drawn"
    );

    store.delete_tenant(&tenant).await.unwrap();
}

/// Each tenant counts alone, and so does each year: numbers only have to be
/// unique and unbroken within one tenant's series for one year.
#[tokio::test]
async fn each_tenant_and_each_year_counts_alone() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "alpha").await;
    let (b, tenant_b, customer_b) = tenant_with_customer(&store, "beta").await;
    let day = today(&pool).await;

    // A year that is already well advanced must not move this year's counter.
    sqlx::query(
        "INSERT INTO billing_sequences (tenant_id, kind, year, next_value) VALUES ($1, $2, $3, 900)",
    )
    .bind(tenant_a.as_str())
    .bind(INVOICE_SEQUENCE_KIND)
    .bind(day.year() - 1)
    .execute(&pool)
    .await
    .unwrap();

    let first_a = a
        .issue_billing_invoice(&draft_with_a_line(&a, &customer_a).await)
        .await
        .unwrap();
    let first_b = b
        .issue_billing_invoice(&draft_with_a_line(&b, &customer_b).await)
        .await
        .unwrap();

    let one = document_number(INVOICE_NUMBER_PREFIX, day.year(), 1);
    assert_eq!(first_a.invoice.number.as_deref(), Some(one.as_str()));
    assert_eq!(
        first_b.invoice.number.as_deref(),
        Some(one.as_str()),
        "two tenants issuing the identical number is correct — the series is per tenant"
    );
    assert_eq!(
        next_value(&pool, &tenant_a, day.year() - 1).await,
        Some(900)
    );

    store.delete_tenant(&tenant_a).await.unwrap();
    store.delete_tenant(&tenant_b).await.unwrap();
    assert_eq!(
        next_value(&pool, &tenant_b, day.year()).await,
        None,
        "deleting a tenant takes its counters with it"
    );
}

/// Voiding keeps the number (that is what keeps the series unbroken) and is
/// available only from `issued`.
#[tokio::test]
async fn voiding_keeps_the_number_and_only_an_issued_document_can_be_voided() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "void").await;
    let day = today(&pool).await;

    // A draft is deleted, never voided: it never took a number.
    let draft = draft_with_a_line(&a, &customer).await;
    let refused = assert_conflict(a.void_billing_invoice(&draft).await);
    assert!(refused.contains("draft"), "{refused}");

    let issued = a.issue_billing_invoice(&draft).await.unwrap();
    let number = issued.invoice.number.clone().unwrap();
    let voided = a.void_billing_invoice(&draft).await.unwrap();

    assert_eq!(voided.invoice.status, InvoiceStatus::Void);
    assert_eq!(
        voided.invoice.number.as_deref(),
        Some(number.as_str()),
        "the number stays — a cancelled document is still part of the series"
    );
    assert_eq!(voided.invoice.issue_date, Some(day));
    assert_eq!(voided.lines.len(), 1, "and it stays readable");
    assert_eq!(voided.totals, issued.totals);

    // Voiding is not a way back into editing, and it does not repeat.
    let twice = assert_conflict(a.void_billing_invoice(&draft).await);
    assert!(twice.contains("void"), "{twice}");
    for message in [
        assert_conflict(a.set_billing_invoice_lines(&draft, &[consulting(1)]).await),
        assert_conflict(a.delete_billing_invoice(&draft).await),
        assert_conflict(a.issue_billing_invoice(&draft).await),
    ] {
        assert!(message.contains("void"), "{message}");
    }

    // A void document does not release its number back to the series.
    let next = a
        .issue_billing_invoice(&draft_with_a_line(&a, &customer).await)
        .await
        .unwrap();
    assert_eq!(
        next.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 2).as_str())
    );

    store.delete_tenant(&tenant).await.unwrap();
}

/// Law 1. Another tenant can neither issue nor void a document of ours, and
/// learns nothing by trying: the refusal is the same `NotFound` a ghost id
/// gets, never the `Conflict` that would confirm the document exists and what
/// state it is in — and their attempt does not move our counter.
#[tokio::test]
async fn another_tenant_can_neither_issue_nor_void_nor_learn_the_state() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "mine").await;
    let (b, tenant_b, customer_b) = tenant_with_customer(&store, "theirs").await;
    let day = today(&pool).await;

    let draft = draft_with_a_line(&a, &customer_a).await;
    let issued_id = draft_with_a_line(&a, &customer_a).await;
    let issued = a.issue_billing_invoice(&issued_id).await.unwrap();

    // A draft of ours (which B's own copy of this call would have issued) and
    // an issued one (where a `Conflict` would leak its state) answer alike.
    assert_not_found(b.issue_billing_invoice(&draft).await);
    assert_not_found(b.issue_billing_invoice(&issued_id).await);
    assert_not_found(b.void_billing_invoice(&draft).await);
    assert_not_found(b.void_billing_invoice(&issued_id).await);
    assert_not_found(b.issue_billing_invoice(&BillingInvoiceId::generate()).await);
    assert_not_found(b.void_billing_invoice(&BillingInvoiceId::generate()).await);

    // Our documents are exactly as we left them.
    let still_draft = a.billing_invoice(&draft).await.unwrap().unwrap();
    assert_eq!(still_draft.invoice.status, InvoiceStatus::Draft);
    assert!(still_draft.invoice.number.is_none());
    let still_issued = a.billing_invoice(&issued_id).await.unwrap().unwrap();
    assert_eq!(still_issued.invoice.status, InvoiceStatus::Issued);
    assert_eq!(still_issued.invoice.number, issued.invoice.number);
    assert_eq!(still_issued.invoice.updated_at, issued.invoice.updated_at);
    assert_eq!(
        next_value(&pool, &tenant_a, day.year()).await,
        Some(2),
        "their refused attempts drew nothing from our series"
    );

    // And B can issue their own document of the same shape, so the denial was
    // about ownership rather than the operation being unavailable.
    let theirs = b
        .issue_billing_invoice(&draft_with_a_line(&b, &customer_b).await)
        .await
        .unwrap();
    assert_eq!(
        theirs.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, day.year(), 1).as_str())
    );
    assert!(
        !b.billing_invoices(None)
            .await
            .unwrap()
            .iter()
            .any(|summary| summary.invoice.id == issued_id),
        "and our documents were never in their list"
    );

    store.delete_tenant(&tenant_a).await.unwrap();
    store.delete_tenant(&tenant_b).await.unwrap();
}

/// A save composed against a draft that arrives while an issue is in flight
/// waits for the issue and is then refused — the B1.07 guard, now proven
/// against the real issuing transaction rather than a planted marker.
#[tokio::test]
async fn a_save_that_races_the_real_issue_loses_cleanly() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "race").await;

    let id = draft_with_a_line(&a, &customer).await;

    let issuing = tokio::spawn({
        let (a, id) = (a.clone(), id.clone());
        async move { a.issue_billing_invoice(&id).await }
    });
    let saving = tokio::spawn({
        let (a, id) = (a.clone(), id.clone());
        async move { a.set_billing_invoice_lines(&id, &[consulting(9_000)]).await }
    });

    let issued = issuing.await.unwrap().unwrap();
    assert!(issued.invoice.number.is_some());
    let after = a.billing_invoice(&id).await.unwrap().unwrap();

    // Whichever won, the document is coherent: either the save landed on the
    // draft and the issue then froze that content, or the save was refused.
    match saving.await.unwrap() {
        Ok(()) => assert_eq!(
            after.lines[0].qty_milli, 9_000,
            "the save won the lock, so its lines are what was issued"
        ),
        Err(StoreError::Conflict(message)) => {
            assert!(message.contains("issued"), "{message}");
            assert_eq!(
                after.lines[0].qty_milli, 3_000,
                "the refused save wrote nothing"
            );
        }
        other => panic!("expected either success or Conflict, got {other:?}"),
    }
    assert_eq!(after.invoice.status, InvoiceStatus::Issued);

    store.delete_tenant(&tenant).await.unwrap();
}

/// **B7.01, at the issue.** Issuing books the document into the journal in the
/// same transaction; voiding takes it back with a reversal; and issuing while
/// the period the entry would land in is closed is refused whole — no number
/// drawn, no half-entry, the document still a draft.
#[tokio::test]
async fn issuing_books_voiding_reverses_and_a_closed_period_refuses_whole() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "books").await;
    let pool = raw_pool().await;
    let today = today(&pool).await;

    // Issue books, in the same act.
    let first = draft_with_a_line(&a, &customer).await;
    let issued = a.issue_billing_invoice(&first).await.unwrap();
    let entry = a
        .fin_invoice_entry(&first)
        .await
        .unwrap()
        .expect("the journal learned about the document as it was issued");
    let booked = a.fin_journal_entry(&entry).await.unwrap().unwrap();
    assert_eq!(
        booked.entry.entry_date,
        issued.invoice.issue_date.unwrap(),
        "booked at the document's date"
    );

    // Void reverses — the entry stays, its mirror lands beside it.
    a.void_billing_invoice(&first).await.unwrap();
    let entries = a.fin_entries(None, None, 10).await.unwrap();
    assert_eq!(entries.len(), 2, "the issue entry and its reversal");
    let trial = a.fin_trial_balance(None, None).await.unwrap();
    assert!(
        trial.accounts.iter().all(|row| row.balance_cents == 0),
        "a voided document leaves every account where it started"
    );

    // A closed period refuses the whole issue and burns no number.
    let period = a
        .create_fin_period(today.replace_day(1).unwrap(), today)
        .await
        .unwrap();
    a.close_fin_period(&period.id, "trial close").await.unwrap();
    let second = draft_with_a_line(&a, &customer).await;
    let refusal = assert_conflict(a.issue_billing_invoice(&second).await);
    assert!(refusal.contains("closed"), "{refusal}");
    let document = a.billing_invoice(&second).await.unwrap().unwrap();
    assert_eq!(document.invoice.status, InvoiceStatus::Draft);
    assert_eq!(document.invoice.number, None, "no number was spent on it");

    // Reopened, the same draft issues with the next number of the series —
    // nothing was burned by the refusal.
    a.reopen_fin_period(&period.id, "trial reopen")
        .await
        .unwrap();
    let document = a.issue_billing_invoice(&second).await.unwrap();
    assert_eq!(
        document.invoice.number.as_deref(),
        Some(document_number(INVOICE_NUMBER_PREFIX, today.year(), 2).as_str()),
        "the second number, not the third"
    );
}
