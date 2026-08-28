//! The draft-invoice lifecycle (alo Billing, wave B1): a document is editable
//! exactly while it is a draft, and a draft — and only a draft — can be thrown
//! away.
//!
//! Issuing itself is B1.08, so the "issue marker" these tests need is planted
//! with raw SQL: `status`, `number` and the two dates set together, which is
//! precisely the state the table's CHECK constraints define as *not a draft*.
//! That is deliberate — the guard under test must hold against the **stored**
//! state of the row, not against whatever the Rust API happened to write, and
//! planting the state directly is the only way to prove that today.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, InvoiceStatus, NewCustomer, NewInvoice,
    NewLine, Store, StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is the typed frozen-document refusal, returning its
/// message.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got: {other:?}"),
    }
}

/// A tenant with one user and one customer, returning the account door, the
/// tenant id and that customer.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("life-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@lifecycle.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("Customer {tag}"),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            payment_terms_days: 21,
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

/// A raw pool alongside the store, for planting the issue marker and for
/// counting rows the store's own reads would filter away.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// Plants the state B1.08 will write: a number, an issue date, a due date and
/// a non-draft status, all together — the table refuses any half of it.
async fn plant_issue_marker(
    pool: &PgPool,
    tenant: &TenantId,
    id: &BillingInvoiceId,
    status: InvoiceStatus,
    number: &str,
) {
    let done = sqlx::query(
        "UPDATE billing_invoices \
            SET status = $3, number = $4, issue_date = DATE '2026-01-31', \
                due_date = DATE '2026-02-21' \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .bind(status.as_str())
    .bind(number)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(done.rows_affected(), 1, "the marker must land on one row");
}

async fn line_count(pool: &PgPool, tenant: &TenantId, id: &BillingInvoiceId) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM billing_invoice_lines WHERE tenant_id = $1 AND invoice_id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Once a document is no longer a draft, every write path refuses it — with a
/// `Conflict`, not a silent no-op — and the document is byte-for-byte what it
/// was before the attempt.
#[tokio::test]
async fn a_document_that_is_no_longer_a_draft_refuses_every_write() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "frozen").await;

    // `paid` and `void` are as frozen as `issued`: both were issued first, and
    // cancelling a document does not reopen it for editing.
    for (index, status) in [
        InvoiceStatus::Issued,
        InvoiceStatus::Paid,
        InvoiceStatus::Void,
    ]
    .into_iter()
    .enumerate()
    {
        let id = a
            .create_billing_invoice(&NewInvoice {
                reference: "PO-77".to_owned(),
                ..NewInvoice::for_customer(customer.clone())
            })
            .await
            .unwrap();
        a.set_billing_invoice_lines(&id, &[consulting(3_000), consulting(1_500)])
            .await
            .unwrap();
        let number = format!("INV-2026-{:05}", index + 1);
        plant_issue_marker(&pool, &tenant, &id, status, &number).await;
        let before = a.billing_invoice(&id).await.unwrap().unwrap();
        assert_eq!(before.invoice.status, status);

        // ---- header, lines, delete: each refused, each naming the status ---
        for message in [
            assert_conflict(
                a.update_billing_invoice(
                    &id,
                    &NewInvoice {
                        reference: "PO-99".to_owned(),
                        ..NewInvoice::for_customer(customer.clone())
                    },
                )
                .await,
            ),
            assert_conflict(a.set_billing_invoice_lines(&id, &[consulting(9_000)]).await),
            assert_conflict(a.set_billing_invoice_lines(&id, &[]).await),
            assert_conflict(a.delete_billing_invoice(&id).await),
        ] {
            assert!(message.contains(status.as_str()), "{message}");
        }

        // ---- a frozen document refuses the edit whatever the payload -------
        // The state is the reason, so it outranks any complaint about content:
        // a caller must not be told to fix a field on a document that would
        // have refused a perfect one.
        assert_conflict(
            a.update_billing_invoice(
                &id,
                &NewInvoice {
                    note: "x".repeat(3_000),
                    ..NewInvoice::for_customer(customer.clone())
                },
            )
            .await,
        );
        assert_conflict(
            a.set_billing_invoice_lines(
                &id,
                &[NewLine {
                    description: "   ".to_owned(),
                    ..consulting(1_000)
                }],
            )
            .await,
        );

        // ---- nothing moved -------------------------------------------------
        let after = a.billing_invoice(&id).await.unwrap().unwrap();
        assert_eq!(after.invoice.status, status);
        assert_eq!(after.invoice.number.as_deref(), Some(number.as_str()));
        assert_eq!(after.invoice.reference, "PO-77");
        assert_eq!(after.invoice.note, "");
        assert_eq!(after.invoice.updated_at, before.invoice.updated_at);
        assert_eq!(after.lines.len(), 2);
        assert_eq!(after.totals, before.totals);
        assert_eq!(line_count(&pool, &tenant, &id).await, 2);
    }

    store.delete_tenant(&tenant).await.unwrap();
}

/// A draft is fully editable right up to the moment it is not: the same calls
/// that are refused above all succeed while the status is `draft`.
#[tokio::test]
async fn a_draft_is_editable_until_the_issue_marker_is_set() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "editable").await;

    let id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&id, &[consulting(2_000)])
        .await
        .unwrap();
    a.update_billing_invoice(
        &id,
        &NewInvoice {
            reference: "PO-1".to_owned(),
            note: "Second half".to_owned(),
            ..NewInvoice::for_customer(customer.clone())
        },
    )
    .await
    .unwrap();
    let draft = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(draft.invoice.reference, "PO-1");
    assert_eq!(draft.invoice.note, "Second half");
    assert_eq!(draft.totals.net_cents, 20_000);
    assert_eq!(draft.totals.gross_cents, 24_200);
    // While it is a draft, a bad payload is still judged on its content.
    match a
        .set_billing_invoice_lines(
            &id,
            &[NewLine {
                description: String::new(),
                ..consulting(1_000)
            }],
        )
        .await
    {
        Err(StoreError::Validation(message)) => assert!(message.contains("line 1"), "{message}"),
        other => panic!("expected Validation while draft, got {other:?}"),
    }

    plant_issue_marker(&pool, &tenant, &id, InvoiceStatus::Issued, "INV-2026-00042").await;

    // The very same edit that worked a moment ago is now refused.
    assert_conflict(
        a.update_billing_invoice(
            &id,
            &NewInvoice {
                reference: "PO-2".to_owned(),
                ..NewInvoice::for_customer(customer.clone())
            },
        )
        .await,
    );
    assert_eq!(
        a.billing_invoice(&id)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .reference,
        "PO-1"
    );

    store.delete_tenant(&tenant).await.unwrap();
}

/// Deleting a draft takes its lines with it and touches nothing else. A draft
/// never consumed a number, so nothing about the sequence is disturbed.
#[tokio::test]
async fn deleting_a_draft_removes_it_and_its_lines_only() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "delete").await;

    let doomed = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    let keeper = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&doomed, &[consulting(1_000), consulting(2_000)])
        .await
        .unwrap();
    a.set_billing_invoice_lines(&keeper, &[consulting(4_000)])
        .await
        .unwrap();

    a.delete_billing_invoice(&doomed).await.unwrap();

    assert!(a.billing_invoice(&doomed).await.unwrap().is_none());
    assert_eq!(
        line_count(&pool, &tenant, &doomed).await,
        0,
        "the lines went with the document, read straight from the table"
    );
    assert!(
        !a.billing_invoices(None)
            .await
            .unwrap()
            .iter()
            .any(|summary| summary.invoice.id == doomed),
        "and it is gone from the list"
    );

    // The other draft is exactly as it was.
    let kept = a.billing_invoice(&keeper).await.unwrap().unwrap();
    assert_eq!(kept.lines.len(), 1);
    assert_eq!(kept.totals.net_cents, 40_000);
    assert_eq!(line_count(&pool, &tenant, &keeper).await, 1);

    // Deleting it twice, or deleting an id that never existed, is the same
    // clean not-found — no existence oracle, and no second cascade.
    assert_not_found(a.delete_billing_invoice(&doomed).await);
    assert_not_found(
        a.delete_billing_invoice(&BillingInvoiceId::generate())
            .await,
    );

    // The customer it named is untouched: deleting a document is not deleting
    // the party it was raised for.
    assert!(a.billing_customer(&customer).await.unwrap().is_some());

    store.delete_tenant(&tenant).await.unwrap();
}

/// The guard is under the row lock, not merely before the write: a save that
/// was composed against a draft and arrives while an issue is in flight waits
/// for that issue and is then refused, rather than landing new lines on a
/// document that has just been numbered and frozen.
#[tokio::test]
async fn a_save_that_races_an_issue_loses_cleanly() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "race").await;

    let id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&id, &[consulting(1_000)])
        .await
        .unwrap();

    // An issue is under way: the row is locked and frozen, but not yet
    // committed — exactly the window B1.08's transaction will occupy.
    let mut issuing = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE billing_invoices \
            SET status = 'issued', number = 'INV-2026-00500', \
                issue_date = DATE '2026-02-01', due_date = DATE '2026-02-22' \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(id.as_str())
    .execute(&mut *issuing)
    .await
    .unwrap();

    let saver = tokio::spawn({
        let (a, id) = (a.clone(), id.clone());
        async move { a.set_billing_invoice_lines(&id, &[consulting(9_000)]).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(
        !saver.is_finished(),
        "the save must wait on the row lock, not read a status the issue is about to change"
    );

    issuing.commit().await.unwrap();

    let message = assert_conflict(saver.await.unwrap());
    assert!(message.contains("issued"), "{message}");
    let after = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(after.invoice.number.as_deref(), Some("INV-2026-00500"));
    assert_eq!(after.lines.len(), 1, "the losing save wrote nothing");
    assert_eq!(after.lines[0].qty_milli, 1_000);
    assert_eq!(after.totals.net_cents, 10_000);

    store.delete_tenant(&tenant).await.unwrap();
}

/// Law 1: another tenant reaches nothing — and learns nothing. Tenant B gets
/// the same `NotFound` whether A's document is an editable draft (where B's
/// own copy of that call would have worked) or a frozen issued document (where
/// a `Conflict` would have confirmed both that the id exists and what state it
/// is in).
#[tokio::test]
async fn another_tenant_can_neither_delete_nor_learn_the_state() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "own").await;
    let (b, tenant_b, customer_b) = tenant_with_customer(&store, "other").await;

    let draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    let issued = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    for id in [&draft, &issued] {
        a.set_billing_invoice_lines(id, &[consulting(1_000)])
            .await
            .unwrap();
    }
    plant_issue_marker(
        &pool,
        &tenant_a,
        &issued,
        InvoiceStatus::Issued,
        "INV-2026-00007",
    )
    .await;

    assert_not_found(b.delete_billing_invoice(&draft).await);
    assert_not_found(b.delete_billing_invoice(&issued).await);
    assert_not_found(
        b.update_billing_invoice(&issued, &NewInvoice::for_customer(customer_b.clone()))
            .await,
    );
    assert_not_found(
        b.set_billing_invoice_lines(&issued, &[consulting(1_000)])
            .await,
    );
    // A ghost id gets the identical answer, so nothing above was an oracle.
    assert_not_found(
        b.delete_billing_invoice(&BillingInvoiceId::generate())
            .await,
    );

    // A's documents are both still there, with their lines.
    assert!(a.billing_invoice(&draft).await.unwrap().is_some());
    let still_issued = a.billing_invoice(&issued).await.unwrap().unwrap();
    assert_eq!(still_issued.invoice.status, InvoiceStatus::Issued);
    assert_eq!(
        still_issued.invoice.number.as_deref(),
        Some("INV-2026-00007")
    );
    assert_eq!(line_count(&pool, &tenant_a, &draft).await, 1);
    assert_eq!(line_count(&pool, &tenant_a, &issued).await, 1);
    // And B's own draft of the same shape is deletable, so the denial above
    // was about ownership, not about the operation being unavailable.
    let mine = b
        .create_billing_invoice(&NewInvoice::for_customer(customer_b))
        .await
        .unwrap();
    b.delete_billing_invoice(&mine).await.unwrap();

    store.delete_tenant(&tenant_a).await.unwrap();
    store.delete_tenant(&tenant_b).await.unwrap();
}
