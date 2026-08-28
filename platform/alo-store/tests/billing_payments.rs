//! Payments against an invoice (alo Billing, wave B1.19): the partial-then-full
//! arc that flips a document's state, the refusals that keep money attached to
//! documents that can actually carry it, the overdue view, and the tenancy
//! proof (Law 1: isolation is tested, not assumed).
//!
//! The wrong-tenant assertions cover **every** path a payment can be reached
//! by — record, list, remove — and both directions of the link, because a
//! foreign key between two tenant-scoped tables is exactly where a cross-tenant
//! reference would otherwise slip in.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, BillingPaymentId, InvoiceStatus,
    NewCustomer, NewInvoice, NewLine, NewPayment, PaymentState, Store, StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Duration, OffsetDateTime};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is the typed state refusal, returning its message.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// Asserts a result is the typed input refusal, returning its message.
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
    let tenant = store.create_tenant(&format!("pay-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@payments.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    common::seed_default_chart(&account).await;
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 14,
            ..Default::default()
        })
        .await
        .unwrap();
    (account, tenant, customer)
}

/// A raw pool alongside the store, for backdating a due date (only time can do
/// that legitimately) and for counting rows the store's own reads would filter.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// 10 hours of consulting at €100.00, 21 % VAT: net 100 000, VAT 21 000, gross
/// 121 000 cents. Hand-computed, so every figure below is checked against
/// arithmetic done outside the code under test.
fn consulting() -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: 10_000,
        unit_price_cents: 10_000,
        vat_rate_bp: 2100,
    }
}

/// Raises a one-line draft and issues it, returning the numbered document's id.
async fn issued_invoice(account: &AccountStore, customer: &BillingCustomerId) -> BillingInvoiceId {
    let id = account
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    account
        .set_billing_invoice_lines(&id, &[consulting()])
        .await
        .unwrap();
    let issued = account.issue_billing_invoice(&id).await.unwrap();
    assert_eq!(issued.invoice.status, InvoiceStatus::Issued);
    assert_eq!(issued.totals.gross_cents, 121_000);
    assert_eq!(issued.paid_cents, 0, "nothing has arrived yet");
    id
}

/// A payment of `amount_cents`, dated today, with a plausible bank reference.
fn transfer(amount_cents: i64) -> NewPayment {
    NewPayment {
        paid_on: None,
        amount_cents,
        method: "bank transfer".to_owned(),
        reference: "NL02RABO0123456789/E2E-77".to_owned(),
    }
}

/// Moves a document's due date into the past, which is the one thing a test
/// cannot do by waiting.
async fn backdate_due(pool: &PgPool, tenant: &TenantId, id: &BillingInvoiceId, days: i64) {
    let done = sqlx::query(
        "UPDATE billing_invoices SET due_date = CURRENT_DATE - $3::int \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(i32::try_from(days).unwrap())
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(done.rows_affected(), 1);
}

async fn status_of(account: &AccountStore, id: &BillingInvoiceId) -> InvoiceStatus {
    account
        .billing_invoice(id)
        .await
        .unwrap()
        .unwrap()
        .invoice
        .status
}

#[tokio::test]
async fn a_partial_then_full_payment_settles_the_document_and_never_crosses_tenants() {
    let store = common::test_store().await;
    let (a, t1, customer_a) = tenant_with_customer(&store, "a").await;
    let (b, _t2, customer_b) = tenant_with_customer(&store, "b").await;
    let invoice = issued_invoice(&a, &customer_a).await;

    // ---- nothing yet: issued, owed in full ---------------------------------
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    let settlement = doc.settlement();
    assert_eq!(settlement.state, PaymentState::Unpaid);
    assert_eq!(settlement.outstanding_cents, 121_000);
    assert!(a.billing_payments(&invoice).await.unwrap().is_empty());

    // ---- a part payment: recorded, but the document is still owed ----------
    let first = a
        .record_billing_payment(
            &invoice,
            &NewPayment {
                method: "SEPA direct debit".to_owned(),
                ..transfer(50_000)
            },
        )
        .await
        .unwrap();
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(
        doc.invoice.status,
        InvoiceStatus::Issued,
        "half the money is not a settled document"
    );
    let settlement = doc.settlement();
    assert_eq!(settlement.paid_cents, 50_000);
    assert_eq!(settlement.outstanding_cents, 71_000);
    assert_eq!(settlement.state, PaymentState::PartiallyPaid);

    let ledger = a.billing_payments(&invoice).await.unwrap();
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger[0].id.as_str(), first.as_str());
    assert_eq!(ledger[0].amount_cents, 50_000);
    assert_eq!(ledger[0].method, "SEPA direct debit");
    assert_eq!(ledger[0].reference, "NL02RABO0123456789/E2E-77");
    assert_eq!(ledger[0].created_by, a.user().as_str());
    assert_eq!(ledger[0].invoice_id.as_str(), invoice.as_str());
    let today = OffsetDateTime::now_utc().date();
    assert!(
        (ledger[0].paid_on - today).abs() <= Duration::days(1),
        "an unstated date is the database's today, not an invented one"
    );

    // The list surface reports the same figures as the document.
    let listed = a.billing_invoices(None).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].paid_cents, 50_000);
    assert_eq!(listed[0].settlement().state, PaymentState::PartiallyPaid);

    // ---- the rest of it: settled, and the status projects that -------------
    let second = a
        .record_billing_payment(&invoice, &transfer(71_000))
        .await
        .unwrap();
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(
        doc.invoice.status,
        InvoiceStatus::Paid,
        "the whole gross has arrived"
    );
    let settlement = doc.settlement();
    assert_eq!(settlement.paid_cents, 121_000);
    assert_eq!(settlement.outstanding_cents, 0);
    assert_eq!(settlement.state, PaymentState::Paid);
    assert_eq!(a.billing_payments(&invoice).await.unwrap().len(), 2);
    // A settled document is frozen like any other non-draft, and is no longer
    // in the issued list.
    assert!(
        a.billing_invoices(Some(InvoiceStatus::Issued))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        a.billing_invoices(Some(InvoiceStatus::Paid))
            .await
            .unwrap()
            .len(),
        1
    );

    // ---- a payment keyed wrongly is removed, and the state goes back -------
    a.delete_billing_payment(&invoice, &second).await.unwrap();
    let doc = a.billing_invoice(&invoice).await.unwrap().unwrap();
    assert_eq!(
        doc.invoice.status,
        InvoiceStatus::Issued,
        "the money is not there, so the document is owed again"
    );
    assert_eq!(doc.settlement().state, PaymentState::PartiallyPaid);
    assert_eq!(doc.paid_cents, 50_000);

    // ---- wrong tenant: every path, both directions -------------------------
    assert_not_found(b.record_billing_payment(&invoice, &transfer(1_000)).await);
    assert_not_found(b.delete_billing_payment(&invoice, &first).await);
    assert!(
        b.billing_payments(&invoice).await.unwrap().is_empty(),
        "a list read is never an existence oracle"
    );
    assert!(b.billing_invoices(None).await.unwrap().is_empty());

    // B's own issued invoice cannot be settled with A's payment id either: the
    // payment is addressed through its document, so it is simply not there.
    let b_invoice = issued_invoice(&b, &customer_b).await;
    assert_not_found(b.delete_billing_payment(&b_invoice, &first).await);
    assert_eq!(b.billing_payments(&b_invoice).await.unwrap().len(), 0);
    assert_eq!(
        status_of(&a, &invoice).await,
        InvoiceStatus::Issued,
        "nothing B did touched A's document"
    );

    // And the same within one tenant: a payment belongs to one document.
    let other = issued_invoice(&a, &customer_a).await;
    assert_not_found(a.delete_billing_payment(&other, &first).await);
    assert_eq!(a.billing_payments(&invoice).await.unwrap().len(), 1);

    // The row itself is the tenant's, at the database level.
    let pool = raw_pool().await;
    let owner: String = sqlx::query_scalar("SELECT tenant_id FROM billing_payments WHERE id = $1")
        .bind(first.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, t1.as_str());
}

#[tokio::test]
async fn money_is_refused_where_recording_it_would_mean_nothing() {
    let store = common::test_store().await;
    let (a, _t, customer) = tenant_with_customer(&store, "refuse").await;

    // ---- a draft is owed by nobody ----------------------------------------
    let draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&draft, &[consulting()])
        .await
        .unwrap();
    let message = assert_conflict(a.record_billing_payment(&draft, &transfer(1_000)).await);
    assert!(
        message.contains("draft") && message.contains("issue"),
        "{message}"
    );

    // ---- a void document was cancelled -------------------------------------
    let voided = issued_invoice(&a, &customer).await;
    a.void_billing_invoice(&voided).await.unwrap();
    let message = assert_conflict(a.record_billing_payment(&voided, &transfer(1_000)).await);
    assert!(message.contains("void"), "{message}");

    // ---- a credit note is money owed the other way -------------------------
    let original = issued_invoice(&a, &customer).await;
    let credit = a.create_billing_credit_note(&original).await.unwrap();
    let credit = a.issue_billing_invoice(&credit).await.unwrap();
    assert_eq!(credit.totals.gross_cents, -121_000, "the exact mirror");
    let message = assert_conflict(
        a.record_billing_payment(&credit.invoice.id, &transfer(1_000))
            .await,
    );
    assert!(message.contains("credit note"), "{message}");

    // ---- amounts: zero is as wrong as negative -----------------------------
    let payable = issued_invoice(&a, &customer).await;
    for bad in [0, -1, -121_000] {
        let message = assert_validation(a.record_billing_payment(&payable, &transfer(bad)).await);
        assert!(message.contains("greater than zero"), "{message}");
    }
    let message = assert_validation(
        a.record_billing_payment(&payable, &transfer(1_000_000_000_001))
            .await,
    );
    assert!(message.contains("at most"), "{message}");

    // ---- a date in the future is money that has not arrived ----------------
    let tomorrow = OffsetDateTime::now_utc()
        .date()
        .checked_add(Duration::days(2))
        .unwrap();
    let message = assert_validation(
        a.record_billing_payment(
            &payable,
            &NewPayment {
                paid_on: Some(tomorrow),
                ..transfer(1_000)
            },
        )
        .await,
    );
    assert!(message.contains("future"), "{message}");

    // ---- bounded text ------------------------------------------------------
    let message = assert_validation(
        a.record_billing_payment(
            &payable,
            &NewPayment {
                method: "x".repeat(61),
                ..transfer(1_000)
            },
        )
        .await,
    );
    assert!(message.contains("method"), "{message}");
    let message = assert_validation(
        a.record_billing_payment(
            &payable,
            &NewPayment {
                reference: "x".repeat(141),
                ..transfer(1_000)
            },
        )
        .await,
    );
    assert!(message.contains("reference"), "{message}");

    // Nothing above was written: a refusal leaves the ledger empty.
    assert!(a.billing_payments(&payable).await.unwrap().is_empty());
    assert_eq!(status_of(&a, &payable).await, InvoiceStatus::Issued);

    // ---- an absent document, and one that never existed --------------------
    assert_not_found(
        a.record_billing_payment(&BillingInvoiceId::new("ghost"), &transfer(1_000))
            .await,
    );
    assert_not_found(
        a.delete_billing_payment(&payable, &BillingPaymentId::new("ghost"))
            .await,
    );

    // ---- a deposit dated before the invoice is legitimate ------------------
    let yesterday = OffsetDateTime::now_utc()
        .date()
        .checked_sub(Duration::days(30))
        .unwrap();
    a.record_billing_payment(
        &payable,
        &NewPayment {
            paid_on: Some(yesterday),
            ..transfer(121_000)
        },
    )
    .await
    .unwrap();
    assert_eq!(status_of(&a, &payable).await, InvoiceStatus::Paid);
}

#[tokio::test]
async fn a_document_with_money_against_it_is_credited_not_voided() {
    let store = common::test_store().await;
    let (a, _t, customer) = tenant_with_customer(&store, "void").await;
    let invoice = issued_invoice(&a, &customer).await;

    let payment = a
        .record_billing_payment(&invoice, &transfer(50_000))
        .await
        .unwrap();
    // Still `issued` (half paid), so the ordinary voidable check would allow
    // it; the payment ledger is what refuses.
    let message = assert_conflict(a.void_billing_invoice(&invoice).await);
    assert!(
        message.contains("credit note") && message.contains("received"),
        "{message}"
    );
    assert_eq!(status_of(&a, &invoice).await, InvoiceStatus::Issued);

    // A fully paid one is refused earlier, by its status.
    a.record_billing_payment(&invoice, &transfer(71_000))
        .await
        .unwrap();
    assert_eq!(status_of(&a, &invoice).await, InvoiceStatus::Paid);
    let message = assert_conflict(a.void_billing_invoice(&invoice).await);
    assert!(message.contains("paid"), "{message}");

    // Remove the money and the document is cancellable again — which is the
    // whole point of the ledger being the authority.
    for recorded in a.billing_payments(&invoice).await.unwrap() {
        a.delete_billing_payment(&invoice, &recorded.id)
            .await
            .unwrap();
    }
    assert_not_found(a.delete_billing_payment(&invoice, &payment).await);
    let voided = a.void_billing_invoice(&invoice).await.unwrap();
    assert_eq!(voided.invoice.status, InvoiceStatus::Void);
    assert!(voided.invoice.number.is_some(), "the number is kept");
}

#[tokio::test]
async fn the_overdue_view_is_what_is_still_owed_past_its_date() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, t1, customer) = tenant_with_customer(&store, "overdue").await;
    let (b, _t2, customer_b) = tenant_with_customer(&store, "overdue-b").await;

    // Four documents, each past its date except where said otherwise.
    let unpaid = issued_invoice(&a, &customer).await;
    backdate_due(&pool, &t1, &unpaid, 10).await;

    let half = issued_invoice(&a, &customer).await;
    backdate_due(&pool, &t1, &half, 3).await;
    a.record_billing_payment(&half, &transfer(60_000))
        .await
        .unwrap();

    let settled = issued_invoice(&a, &customer).await;
    backdate_due(&pool, &t1, &settled, 40).await;
    a.record_billing_payment(&settled, &transfer(121_000))
        .await
        .unwrap();

    let not_yet_due = issued_invoice(&a, &customer).await;

    let draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();

    // A credit note, issued and long past the date it was stamped with: money
    // owed *to* the customer makes nobody late.
    let credit = a.create_billing_credit_note(&unpaid).await.unwrap();
    let credit = a.issue_billing_invoice(&credit).await.unwrap().invoice.id;
    backdate_due(&pool, &t1, &credit, 60).await;

    let overdue = a.billing_overdue_invoices().await.unwrap();
    let ids: Vec<&str> = overdue.iter().map(|s| s.invoice.id.as_str()).collect();
    assert!(ids.contains(&unpaid.as_str()), "nothing has arrived");
    assert!(
        ids.contains(&half.as_str()),
        "a partially paid document is overdue for the remainder"
    );
    assert!(!ids.contains(&settled.as_str()), "it was paid in full");
    assert!(!ids.contains(&not_yet_due.as_str()));
    assert!(!ids.contains(&draft.as_str()), "a draft has no due date");
    assert!(!ids.contains(&credit.as_str()));
    assert_eq!(overdue.len(), 2);

    // The list carries the money, so an overdue screen shows what is left
    // without a second call per row.
    let partial = overdue
        .iter()
        .find(|s| s.invoice.id.as_str() == half.as_str())
        .unwrap();
    assert_eq!(partial.paid_cents, 60_000);
    assert_eq!(partial.settlement().outstanding_cents, 61_000);
    assert_eq!(partial.settlement().state, PaymentState::PartiallyPaid);

    // The per-row flag and the list agree, by construction: one predicate.
    let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(&pool)
        .await
        .unwrap();
    for summary in &overdue {
        assert!(summary.invoice.is_overdue(today), "{:?}", summary.invoice);
    }
    for summary in a.billing_invoices(None).await.unwrap() {
        assert_eq!(
            summary.invoice.is_overdue(today),
            ids.contains(&summary.invoice.id.as_str()),
            "the flag and the view must never disagree: {:?}",
            summary.invoice
        );
    }

    // Wrong tenant: B's overdue view is B's, and A's documents are not in it.
    let b_overdue_doc = issued_invoice(&b, &customer_b).await;
    let b_tenant = b.tenant().clone();
    backdate_due(&pool, &b_tenant, &b_overdue_doc, 5).await;
    let b_overdue = b.billing_overdue_invoices().await.unwrap();
    assert_eq!(b_overdue.len(), 1);
    assert_eq!(b_overdue[0].invoice.id.as_str(), b_overdue_doc.as_str());
}
