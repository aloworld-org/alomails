//! Tenancy proof for alo Billing invoices (Law 1: isolation is tested, not
//! assumed), plus the document arc the queue item requires: raise a draft,
//! replace its header, replace its line set, read the totals back.
//!
//! The wrong-tenant assertions cover **every** path a document can be reached
//! by — read, list, header update, line replacement — and the two links a
//! document carries (its customer, and the line rows underneath it), because a
//! foreign key is exactly where a cross-tenant reference would otherwise slip
//! in.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, InvoiceStatus, NewCustomer, NewInvoice,
    NewLine, Store, StoreError, TenantId,
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
    let tenant = store.create_tenant(&format!("inv-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@invoices.test"))
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

fn consulting(hours_milli: i64) -> NewLine {
    NewLine {
        description: "Consulting".to_owned(),
        unit: "hour".to_owned(),
        qty_milli: hours_milli,
        unit_price_cents: 12_000,
        vat_rate_bp: 1900,
    }
}

#[tokio::test]
async fn billing_invoices_round_trip_and_never_cross_tenant() {
    let store = common::test_store().await;
    let (a, t1, customer_a) = tenant_with_customer(&store, "a").await;
    let (b, t2, customer_b) = tenant_with_customer(&store, "b").await;

    // ---- raise a draft: unnumbered, undated, customer defaults snapshotted
    let id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    let doc = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(doc.invoice.status, InvoiceStatus::Draft);
    assert_eq!(doc.invoice.currency, "EUR", "taken from the customer");
    assert_eq!(doc.invoice.payment_terms_days, 14, "and so are the terms");
    assert!(
        doc.invoice.number.is_none()
            && doc.invoice.issue_date.is_none()
            && doc.invoice.due_date.is_none(),
        "a draft can never consume a number"
    );
    assert!(!doc.invoice.is_credit_note && doc.invoice.credits_invoice_id.is_none());
    assert_eq!(doc.invoice.created_by, a.user().as_str());
    assert!(doc.lines.is_empty(), "a new draft has no lines");
    assert_eq!(doc.totals.net_cents, 0);
    assert_eq!(doc.totals.gross_cents, 0);
    assert!(doc.totals.vat_by_rate.is_empty());

    // ---- lines: written as a set, in the caller's order -------------------
    a.set_billing_invoice_lines(
        &id,
        &[
            consulting(10_000),
            NewLine {
                description: "Travel".to_owned(),
                unit: "km".to_owned(),
                qty_milli: 120_000,
                unit_price_cents: 42,
                vat_rate_bp: 700,
            },
            NewLine {
                description: "Loyalty discount".to_owned(),
                qty_milli: -1_000,
                unit_price_cents: 12_000,
                vat_rate_bp: 1900,
                ..Default::default()
            },
        ],
    )
    .await
    .unwrap();

    let doc = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(doc.lines.len(), 3);
    assert_eq!(
        doc.lines.iter().map(|l| l.line_order).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "positions are 0-based and follow the caller's order"
    );
    assert_eq!(doc.lines[1].description, "Travel");
    // Hand-computed: 10 h × €120 = €1200.00, less a €120.00 discount, both at
    // 19 % → net 1080.00, VAT 205.20. Travel: 120 km × €0.42 = €50.40 at 7 %
    // → VAT 3.53 (50.40 × 0.07 = 3.528, half away from zero).
    assert_eq!(doc.lines[0].net_cents(), 120_000);
    assert_eq!(doc.lines[1].net_cents(), 5_040);
    assert_eq!(doc.lines[2].net_cents(), -12_000);
    assert_eq!(doc.totals.net_cents, 113_040);
    assert_eq!(doc.totals.vat_by_rate.len(), 2);
    assert_eq!(doc.totals.vat_by_rate[0].rate_bp, 700);
    assert_eq!(doc.totals.vat_by_rate[0].vat_cents, 353);
    assert_eq!(doc.totals.vat_by_rate[1].rate_bp, 1900);
    assert_eq!(doc.totals.vat_by_rate[1].net_cents, 108_000);
    assert_eq!(doc.totals.vat_by_rate[1].vat_cents, 20_520);
    assert_eq!(doc.totals.vat_cents, 20_873);
    assert_eq!(doc.totals.gross_cents, 133_913);
    // The same arithmetic is available before anything is written, so a draft
    // editor never has to compute money in the browser.
    let previewed = a
        .billing_line_totals(&[
            consulting(10_000),
            NewLine {
                description: "Travel".to_owned(),
                unit: "km".to_owned(),
                qty_milli: 120_000,
                unit_price_cents: 42,
                vat_rate_bp: 700,
            },
            NewLine {
                description: "Loyalty discount".to_owned(),
                qty_milli: -1_000,
                unit_price_cents: 12_000,
                vat_rate_bp: 1900,
                ..Default::default()
            },
        ])
        .unwrap();
    assert_eq!(previewed, doc.totals, "preview and stored agree exactly");

    // Replacing the set replaces it wholly — no leftovers from the old one.
    a.set_billing_invoice_lines(&id, &[consulting(2_000)])
        .await
        .unwrap();
    let doc = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(doc.lines.len(), 1);
    assert_eq!(doc.totals.net_cents, 24_000);
    assert_eq!(doc.totals.gross_cents, 28_560);
    // ... and emptying it is legitimate for a draft.
    a.set_billing_invoice_lines(&id, &[]).await.unwrap();
    assert!(
        a.billing_invoice(&id)
            .await
            .unwrap()
            .unwrap()
            .lines
            .is_empty()
    );
    a.set_billing_invoice_lines(&id, &[consulting(2_000)])
        .await
        .unwrap();

    // ---- header update: a full replace, dates and number untouched --------
    a.update_billing_invoice(
        &id,
        &NewInvoice {
            currency: Some("chf".to_owned()),
            payment_terms_days: Some(30),
            reference: "  PO-4471  ".to_owned(),
            note: "Thank you".to_owned(),
            ..NewInvoice::for_customer(customer_a.clone())
        },
    )
    .await
    .unwrap();
    let doc = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(doc.invoice.currency, "CHF", "normalised, like every code");
    assert_eq!(doc.invoice.payment_terms_days, 30);
    assert_eq!(doc.invoice.reference, "PO-4471");
    assert_eq!(doc.invoice.note, "Thank you");
    assert_eq!(doc.invoice.status, InvoiceStatus::Draft);
    assert!(doc.invoice.number.is_none());
    assert!(doc.invoice.updated_at >= doc.invoice.created_at);

    // ---- list: newest first, with totals, filterable by status ------------
    let second = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    let listed = a.billing_invoices(None).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .any(|s| s.invoice.id == id && s.totals.net_cents == 24_000),
        "the list carries computed totals, not a stored column"
    );
    assert!(
        listed
            .iter()
            .any(|s| s.invoice.id == second && s.totals.net_cents == 0),
        "a document with no lines totals to zero"
    );
    assert_eq!(
        a.billing_invoices(Some(InvoiceStatus::Draft))
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        a.billing_invoices(Some(InvoiceStatus::Issued))
            .await
            .unwrap()
            .is_empty(),
        "nothing is issued yet — issuing arrives with the sequence (B1.08)"
    );

    // ---- another tenant: the clean denial on every path -------------------
    assert!(b.billing_invoice(&id).await.unwrap().is_none());
    assert!(b.billing_invoices(None).await.unwrap().is_empty());
    assert_not_found(
        b.update_billing_invoice(
            &id,
            &NewInvoice {
                reference: "hijacked".to_owned(),
                ..NewInvoice::for_customer(customer_b.clone())
            },
        )
        .await,
    );
    assert_not_found(b.set_billing_invoice_lines(&id, &[consulting(1_000)]).await);
    // Nothing they tried changed A's document — header or lines.
    let after = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(after.invoice.reference, "PO-4471");
    assert_eq!(after.lines.len(), 1);
    assert_eq!(after.totals.net_cents, 24_000);

    // An id that never existed is the same answer as another tenant's id —
    // there is no existence oracle.
    let ghost = BillingInvoiceId::generate();
    assert!(a.billing_invoice(&ghost).await.unwrap().is_none());
    assert_not_found(
        a.update_billing_invoice(&ghost, &NewInvoice::for_customer(customer_a.clone()))
            .await,
    );
    assert_not_found(
        a.set_billing_invoice_lines(&ghost, &[consulting(1_000)])
            .await,
    );

    // ---- the customer link can never cross the tenant boundary -----------
    // B's customer id is a real id — just not A's. Raising or re-pointing a
    // document at it is a NotFound, not a cross-tenant link.
    assert_not_found(
        a.create_billing_invoice(&NewInvoice::for_customer(customer_b.clone()))
            .await,
    );
    assert_not_found(
        a.update_billing_invoice(&id, &NewInvoice::for_customer(customer_b.clone()))
            .await,
    );
    let unchanged = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(unchanged.invoice.customer_id, customer_a);

    // ---- validation guards every write path ------------------------------
    let long = "x".repeat(3_000);
    assert!(
        assert_validation(
            a.create_billing_invoice(&NewInvoice {
                currency: Some("EURO".to_owned()),
                ..NewInvoice::for_customer(customer_a.clone())
            })
            .await
        )
        .contains("currency")
    );
    assert!(
        assert_validation(
            a.update_billing_invoice(
                &id,
                &NewInvoice {
                    payment_terms_days: Some(-1),
                    ..NewInvoice::for_customer(customer_a.clone())
                }
            )
            .await
        )
        .contains("payment terms")
    );
    assert!(
        assert_validation(
            a.update_billing_invoice(
                &id,
                &NewInvoice {
                    note: long.clone(),
                    ..NewInvoice::for_customer(customer_a.clone())
                }
            )
            .await
        )
        .contains("note")
    );
    // A bad line is named by its position, and nothing is written: the
    // document still reads exactly as it did.
    let message = assert_validation(
        a.set_billing_invoice_lines(
            &id,
            &[
                consulting(1_000),
                NewLine {
                    description: String::new(),
                    ..consulting(1_000)
                },
            ],
        )
        .await,
    );
    assert!(message.contains("line 2"), "{message}");
    let after_reject = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(after_reject.lines.len(), 1, "the set was left alone");
    assert_eq!(after_reject.lines[0].qty_milli, 2_000);

    // ---- an archived customer is not billed again ------------------------
    a.set_billing_customer_archived(&customer_a, true)
        .await
        .unwrap();
    assert!(
        assert_validation(
            a.create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
                .await
        )
        .contains("archived")
    );
    a.set_billing_customer_archived(&customer_a, false)
        .await
        .unwrap();
    assert!(
        a.create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
            .await
            .is_ok()
    );

    // ---- deleting the tenant purges its documents and their lines --------
    // Read the rows directly: the claim is that they were cascaded away, not
    // merely hidden behind the tenant predicate of the list call.
    store.delete_tenant(&t1).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    for table in ["billing_invoices", "billing_invoice_lines"] {
        let remaining: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        ))
        .bind(t1.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0, "{table} is purged with the tenant");
    }
    // B's tenant is untouched by A's deletion.
    assert!(b.billing_invoices(None).await.unwrap().is_empty());
    store.delete_tenant(&t2).await.unwrap();
}

/// A document's lines belong to that document alone: replacing one invoice's
/// lines never touches another's, not even the same tenant's.
#[tokio::test]
async fn replacing_lines_touches_only_that_document() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "lines").await;

    let first = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    let second = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&first, &[consulting(1_000), consulting(2_000)])
        .await
        .unwrap();
    a.set_billing_invoice_lines(&second, &[consulting(5_000)])
        .await
        .unwrap();

    a.set_billing_invoice_lines(&first, &[consulting(3_000)])
        .await
        .unwrap();

    let first_doc = a.billing_invoice(&first).await.unwrap().unwrap();
    let second_doc = a.billing_invoice(&second).await.unwrap().unwrap();
    assert_eq!(first_doc.lines.len(), 1);
    assert_eq!(first_doc.totals.net_cents, 36_000);
    assert_eq!(second_doc.lines.len(), 1, "the other document is untouched");
    assert_eq!(second_doc.totals.net_cents, 60_000);
    // Every line id is distinct across the tenant.
    assert_ne!(first_doc.lines[0].id, second_doc.lines[0].id);

    store.delete_tenant(&tenant).await.unwrap();
}

/// The line set survives the round trip exactly — quantities in milli-units,
/// prices in cents, at the bounds — and a full-size document totals correctly.
#[tokio::test]
async fn a_full_size_document_round_trips_exactly() {
    let store = common::test_store().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "bulk").await;
    let id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer))
        .await
        .unwrap();

    // The largest document the store accepts, with awkward quantities and a
    // price that forces rounding at the subtotal rather than per line.
    let lines: Vec<NewLine> = (0..alo_store::billing_line::MAX_LINES)
        .map(|i| NewLine {
            description: format!("Item {i}"),
            unit: "piece".to_owned(),
            qty_milli: 333,
            unit_price_cents: 999,
            vat_rate_bp: 2100,
        })
        .collect();
    a.set_billing_invoice_lines(&id, &lines).await.unwrap();

    let doc = a.billing_invoice(&id).await.unwrap().unwrap();
    assert_eq!(doc.lines.len(), alo_store::billing_line::MAX_LINES);
    assert_eq!(doc.lines[0].qty_milli, 333);
    assert_eq!(doc.lines[0].unit_price_cents, 999);
    // 0.333 × 999 = 332.667 → 333 cents a line, 500 lines → 166 500 net;
    // 21 % of that, rounded once, is 34 965.
    assert_eq!(doc.lines[0].net_cents(), 333);
    assert_eq!(doc.totals.net_cents, 166_500);
    assert_eq!(doc.totals.vat_cents, 34_965);
    assert_eq!(doc.totals.gross_cents, 201_465);
    assert_eq!(
        doc.lines.last().map(|l| l.line_order),
        Some(i32::try_from(alo_store::billing_line::MAX_LINES).unwrap() - 1)
    );

    // One line more than the cap is refused, and the stored set is untouched.
    let mut too_many = lines.clone();
    too_many.push(consulting(1_000));
    assert!(
        assert_validation(a.set_billing_invoice_lines(&id, &too_many).await).contains("at most")
    );
    assert_eq!(
        a.billing_invoice(&id).await.unwrap().unwrap().lines.len(),
        alo_store::billing_line::MAX_LINES
    );

    store.delete_tenant(&tenant).await.unwrap();
}
