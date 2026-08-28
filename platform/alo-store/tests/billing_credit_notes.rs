//! Credit notes (alo Billing, wave B1, item B1.09): the document that corrects
//! an invoice the customer already holds.
//!
//! The properties under test are the ones an accountant would ask about, not
//! the ones the code happens to have:
//!
//! - **The ledger closes.** An issued invoice plus its full credit note sum to
//!   zero — net, VAT, gross, and every row of the per-rate VAT breakdown — with
//!   no residual cent anywhere.
//! - **The series stays one series.** A credit note draws its number from the
//!   same per-tenant counter as the invoice it credits, so the ledger has no
//!   holes and no parallel numbering.
//! - **Only a real document can be credited.** Crediting a draft, a void
//!   document, or another credit note is refused *typed*, and refused before
//!   anything is written.
//! - **Tenancy holds**, as everywhere: another tenant's document cannot be
//!   credited, cannot be discovered by trying, and its counter cannot be moved.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    AccountStore, BillingCustomerId, BillingInvoiceId, InvoiceStatus, NewCustomer, NewInvoice,
    NewLine, Store, StoreError, TenantId, Totals,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error, and never a `Conflict` (which would confirm both that
/// the id exists and what state it is in).
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

/// Asserts a result is the typed fixable-input refusal, returning its message.
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
    let tenant = store.create_tenant(&format!("credit-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@credit.test"))
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

fn line(description: &str, qty_milli: i64, unit_price_cents: i64, vat_rate_bp: i32) -> NewLine {
    NewLine {
        description: description.to_owned(),
        unit: "hour".to_owned(),
        qty_milli,
        unit_price_cents,
        vat_rate_bp,
    }
}

/// The awkward document, on purpose: three VAT rates, a discount line with a
/// negative quantity, and quantities whose nets land on a half cent — so the
/// mirror property is exercised against rounding rather than around it.
fn awkward_lines() -> Vec<NewLine> {
    vec![
        // 0.333 h × €99.99 → 3 329.667 milli-cents → rounds to 3330.
        line("Consulting", 333, 9_999, 2100),
        line("Materials", 7_500, 1_233, 900),
        line("Training", 2_500, 4_567, 2100),
        line("Zero-rated export", 1_000, 12_345, 0),
        // A discount inside the original, so the credit's mirror is not simply
        // "all lines negative".
        line("Loyalty discount", -500, 1_111, 2100),
    ]
}

/// Raises a draft with the given lines and issues it, returning its id.
async fn issued_invoice(
    account: &AccountStore,
    customer: &BillingCustomerId,
    lines: &[NewLine],
) -> BillingInvoiceId {
    let id = account
        .create_billing_invoice(&NewInvoice {
            reference: "PO-4471".to_owned(),
            note: "Payable within 14 days.".to_owned(),
            ..NewInvoice::for_customer(customer.clone())
        })
        .await
        .unwrap();
    account.set_billing_invoice_lines(&id, lines).await.unwrap();
    account.issue_billing_invoice(&id).await.unwrap();
    id
}

/// A raw pool alongside the store, for reading rows the store's own reads would
/// shape or filter.
async fn raw_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// The tenant's counter for the invoice series in the given year, or `None`
/// when the series has never been drawn from.
async fn counter(pool: &PgPool, tenant: &TenantId, year: i32) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT next_value FROM billing_sequences \
         WHERE tenant_id = $1 AND kind = 'invoice' AND year = $2",
    )
    .bind(tenant.as_str())
    .bind(year)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Adds two documents' totals the way a ledger does — every figure, including
/// each row of the VAT breakdown.
fn ledger_sum(original: &Totals, credit: &Totals) -> Totals {
    let mut vat_by_rate: Vec<alo_store::VatSubtotal> = Vec::new();
    for row in original.vat_by_rate.iter().chain(credit.vat_by_rate.iter()) {
        match vat_by_rate
            .iter_mut()
            .find(|kept| kept.rate_bp == row.rate_bp)
        {
            Some(kept) => {
                kept.net_cents += row.net_cents;
                kept.vat_cents += row.vat_cents;
            }
            None => vat_by_rate.push(*row),
        }
    }
    vat_by_rate.sort_by_key(|row| row.rate_bp);
    Totals {
        net_cents: original.net_cents + credit.net_cents,
        vat_cents: original.vat_cents + credit.vat_cents,
        gross_cents: original.gross_cents + credit.gross_cents,
        vat_by_rate,
    }
}

/// The item's headline property: an issued invoice and its full credit note
/// cancel each other out exactly — and the credit note is a proper document of
/// the same series, not a marker on the original.
#[tokio::test]
async fn an_issued_invoice_and_its_credit_note_sum_to_zero() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "ledger").await;

    let original_id = issued_invoice(&a, &customer, &awkward_lines()).await;
    let original = a.billing_invoice(&original_id).await.unwrap().unwrap();
    assert_eq!(original.invoice.status, InvoiceStatus::Issued);
    assert!(!original.invoice.is_credit_note);
    assert!(
        original.totals.gross_cents > 0 && original.totals.vat_by_rate.len() == 3,
        "the fixture must be a real, awkward document: {:?}",
        original.totals
    );

    let credit_id = a.create_billing_credit_note(&original_id).await.unwrap();
    let credit = a.billing_invoice(&credit_id).await.unwrap().unwrap();

    // It is a draft that names its original, on the same customer, currency and
    // terms, carrying the customer's own reference forward.
    assert_eq!(credit.invoice.status, InvoiceStatus::Draft);
    assert!(credit.invoice.is_credit_note);
    assert_eq!(
        credit.invoice.credits_invoice_id.as_ref(),
        Some(&original_id)
    );
    assert_eq!(credit.invoice.customer_id, original.invoice.customer_id);
    assert_eq!(credit.invoice.currency, original.invoice.currency);
    assert_eq!(
        credit.invoice.payment_terms_days,
        original.invoice.payment_terms_days
    );
    assert_eq!(credit.invoice.reference, original.invoice.reference);
    // The original's note travelled with the document ("payable within 14
    // days"); repeating it on a credit note would say the opposite of the
    // truth, so a credit note starts with none.
    assert_eq!(credit.invoice.note, "");
    assert!(credit.invoice.number.is_none(), "a draft carries no number");

    // Every line mirrored, in the original's order, with only the quantity
    // negated — and lines of its own, not the original's rows.
    assert_eq!(credit.lines.len(), original.lines.len());
    for (mirrored, source) in credit.lines.iter().zip(original.lines.iter()) {
        assert_eq!(mirrored.line_order, source.line_order);
        assert_eq!(mirrored.description, source.description);
        assert_eq!(mirrored.unit, source.unit);
        assert_eq!(mirrored.qty_milli, -source.qty_milli);
        assert_eq!(mirrored.unit_price_cents, source.unit_price_cents);
        assert_eq!(mirrored.vat_rate_bp, source.vat_rate_bp);
        assert_ne!(mirrored.id, source.id, "a mirrored line is its own row");
    }

    // The ledger closes — before it is issued and after.
    let closed = ledger_sum(&original.totals, &credit.totals);
    assert_eq!(closed.net_cents, 0, "{:?}", credit.totals);
    assert_eq!(closed.vat_cents, 0, "{:?}", credit.totals);
    assert_eq!(closed.gross_cents, 0, "{:?}", credit.totals);
    for row in &closed.vat_by_rate {
        assert_eq!(
            (row.net_cents, row.vat_cents),
            (0, 0),
            "rate {} leaves a residue",
            row.rate_bp
        );
    }
    assert_eq!(
        closed.vat_by_rate.len(),
        3,
        "every rate of the original must appear in the credit"
    );

    // Issuing it draws from the SAME series: the original took 1, the credit
    // takes 2, and there is exactly one counter row for the tenant.
    let issued_credit = a.issue_billing_invoice(&credit_id).await.unwrap();
    let year = issued_credit.invoice.issue_date.unwrap().year();
    assert_eq!(
        original.invoice.number.as_deref(),
        Some(format!("INV-{year}-00001").as_str())
    );
    assert_eq!(
        issued_credit.invoice.number.as_deref(),
        Some(format!("INV-{year}-00002").as_str())
    );
    assert_eq!(counter(&pool, &tenant, year).await, Some(3));
    assert!(
        issued_credit.invoice.is_credit_note
            && issued_credit.invoice.credits_invoice_id == Some(original_id.clone()),
        "issuing must not lose the link"
    );
    // Issued, it is frozen like any other document.
    assert!(!assert_conflict(a.delete_billing_invoice(&credit_id).await).is_empty());
    // And the ledger still closes with the frozen pair.
    assert_eq!(
        ledger_sum(&original.totals, &issued_credit.totals).gross_cents,
        0
    );

    // The read side: the original knows what credits it.
    let credits = a.billing_credit_notes(&original_id).await.unwrap();
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].invoice.id, credit_id);
    assert_eq!(credits[0].totals, issued_credit.totals);
    // …and the credit note is not itself credited by anything.
    assert!(a.billing_credit_notes(&credit_id).await.unwrap().is_empty());
}

/// Only a document the customer actually holds can be credited, and a refused
/// attempt writes nothing at all — no document, and no number drawn.
#[tokio::test]
async fn a_draft_a_void_document_and_a_credit_note_are_all_refused() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "refusal").await;

    // A draft: never a document, so there is nothing to credit.
    let draft_id = a
        .create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
        .await
        .unwrap();
    a.set_billing_invoice_lines(&draft_id, &[line("Consulting", 1_000, 10_000, 2100)])
        .await
        .unwrap();
    let refusal = assert_conflict(a.create_billing_credit_note(&draft_id).await);
    assert!(refusal.contains("draft"), "{refusal}");
    assert!(
        refusal.contains("delete"),
        "the refusal should point at the way out: {refusal}"
    );
    assert_eq!(
        counter(&pool, &tenant, 2026).await,
        None,
        "a refused credit must not touch the series"
    );
    assert!(
        a.billing_credit_notes(&draft_id).await.unwrap().is_empty(),
        "nothing was written"
    );

    // A void document: already cancelled in full.
    let voided_id = issued_invoice(&a, &customer, &[line("Consulting", 1_000, 10_000, 2100)]).await;
    a.void_billing_invoice(&voided_id).await.unwrap();
    let refusal = assert_conflict(a.create_billing_credit_note(&voided_id).await);
    assert!(refusal.contains("void"), "{refusal}");
    assert!(a.billing_credit_notes(&voided_id).await.unwrap().is_empty());

    // A credit note: crediting one is an invoice, not a credit. The answer is
    // the same whether it is still a draft or already issued — the refusal is
    // about what the document IS, so it must not change under it.
    let original_id =
        issued_invoice(&a, &customer, &[line("Consulting", 2_000, 10_000, 2100)]).await;
    let credit_id = a.create_billing_credit_note(&original_id).await.unwrap();
    let refusal = assert_conflict(a.create_billing_credit_note(&credit_id).await);
    assert!(refusal.contains("credit note"), "{refusal}");
    assert!(
        !refusal.contains("draft"),
        "a credit note is refused for what it is, not for being a draft: {refusal}"
    );
    a.issue_billing_invoice(&credit_id).await.unwrap();
    let refusal = assert_conflict(a.create_billing_credit_note(&credit_id).await);
    assert!(refusal.contains("credit note"), "{refusal}");
    assert!(a.billing_credit_notes(&credit_id).await.unwrap().is_empty());

    // An id that never existed is the not-found denial, not a state refusal.
    assert_not_found(
        a.create_billing_credit_note(&BillingInvoiceId::new("no-such-invoice"))
            .await,
    );
}

/// A **paid** invoice is creditable — that is the whole reason credit notes
/// exist rather than voiding: money changed hands, so the correction has to
/// leave both documents standing.
#[tokio::test]
async fn a_paid_invoice_is_corrected_by_crediting_it_not_by_voiding_it() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant, customer) = tenant_with_customer(&store, "paid").await;

    let original_id = issued_invoice(&a, &customer, &awkward_lines()).await;
    // Payments arrive in B1.19; the state they will write is planted here, so
    // the guard is tested against the STORED state rather than against what
    // today's Rust API happens to be able to produce.
    let done =
        sqlx::query("UPDATE billing_invoices SET status = 'paid' WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_str())
            .bind(original_id.as_str())
            .execute(&pool)
            .await
            .unwrap();
    assert_eq!(done.rows_affected(), 1);

    // Voiding a paid document is refused…
    let refusal = assert_conflict(a.void_billing_invoice(&original_id).await);
    assert!(refusal.contains("paid"), "{refusal}");
    // …crediting it is not.
    let credit_id = a.create_billing_credit_note(&original_id).await.unwrap();
    let credit = a.billing_invoice(&credit_id).await.unwrap().unwrap();
    let original = a.billing_invoice(&original_id).await.unwrap().unwrap();
    assert_eq!(
        original.invoice.status,
        InvoiceStatus::Paid,
        "crediting does not reopen the original"
    );
    assert_eq!(ledger_sum(&original.totals, &credit.totals).gross_cents, 0);

    // An ARCHIVED customer can still be credited: correcting a document
    // already in their hands is not new business. (Raising a fresh invoice for
    // them stays refused.)
    a.set_billing_customer_archived(&customer, true)
        .await
        .unwrap();
    let second_credit = a.create_billing_credit_note(&original_id).await.unwrap();
    assert_eq!(
        a.billing_invoice(&second_credit)
            .await
            .unwrap()
            .unwrap()
            .invoice
            .customer_id,
        customer
    );
    assert!(matches!(
        a.create_billing_invoice(&NewInvoice::for_customer(customer.clone()))
            .await,
        Err(StoreError::Validation(_))
    ));
    assert_eq!(
        a.billing_credit_notes(&original_id).await.unwrap().len(),
        2,
        "partial corrections are several credit notes, all listed"
    );
}

/// A credit-note draft is editable — that is how a **partial** credit is
/// made — but not off the document it credits: the customer and the currency
/// are what tie the correction to the original.
#[tokio::test]
async fn a_credit_note_draft_is_editable_but_stays_on_its_original() {
    let store = common::test_store().await;
    let (a, _tenant, customer) = tenant_with_customer(&store, "partial").await;
    let other_customer = a
        .create_billing_customer(&NewCustomer {
            name: "Somebody Else BV".to_owned(),
            country: "NL".to_owned(),
            currency: "EUR".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let original_id = issued_invoice(&a, &customer, &awkward_lines()).await;
    let credit_id = a.create_billing_credit_note(&original_id).await.unwrap();

    // Moving it to another customer, or another currency, is refused typed —
    // either would make the correction reverse nothing.
    let refusal = assert_validation(
        a.update_billing_invoice(
            &credit_id,
            &NewInvoice::for_customer(other_customer.clone()),
        )
        .await,
    );
    assert!(refusal.contains("credit note"), "{refusal}");
    let refusal = assert_validation(
        a.update_billing_invoice(
            &credit_id,
            &NewInvoice {
                currency: Some("USD".to_owned()),
                ..NewInvoice::for_customer(customer.clone())
            },
        )
        .await,
    );
    assert!(refusal.contains("currency"), "{refusal}");
    let unchanged = a.billing_invoice(&credit_id).await.unwrap().unwrap();
    assert_eq!(unchanged.invoice.customer_id, customer);
    assert_eq!(unchanged.invoice.currency, "EUR");

    // Everything else about it is an ordinary draft: reference, note, terms…
    a.update_billing_invoice(
        &credit_id,
        &NewInvoice {
            payment_terms_days: Some(0),
            reference: "PO-4471".to_owned(),
            note: "Corrects the training line only.".to_owned(),
            ..NewInvoice::for_customer(customer.clone())
        },
    )
    .await
    .unwrap();

    // …and its lines, which is how a PARTIAL credit is made: keep one line of
    // the mirror, drop the rest.
    let mirror = a.billing_invoice(&credit_id).await.unwrap().unwrap();
    let training = mirror
        .lines
        .iter()
        .find(|line| line.description == "Training")
        .expect("the mirror carries every line of the original");
    a.set_billing_invoice_lines(
        &credit_id,
        &[line(
            "Training",
            training.qty_milli,
            training.unit_price_cents,
            training.vat_rate_bp,
        )],
    )
    .await
    .unwrap();

    let partial = a.billing_invoice(&credit_id).await.unwrap().unwrap();
    assert!(partial.invoice.is_credit_note, "still a credit note");
    assert_eq!(
        partial.invoice.credits_invoice_id.as_ref(),
        Some(&original_id),
        "editing must never break the link"
    );
    assert_eq!(partial.invoice.note, "Corrects the training line only.");
    assert_eq!(partial.lines.len(), 1);
    // A partial credit is worth less than the whole, and still negative.
    let original = a.billing_invoice(&original_id).await.unwrap().unwrap();
    assert!(partial.totals.gross_cents < 0, "{:?}", partial.totals);
    assert!(
        partial.totals.gross_cents > -original.totals.gross_cents,
        "a one-line credit cannot be worth the whole document"
    );
    // Training: 2.5 h × €45.67 = €114.175 → 11 418 net, 21 % → 2 398 VAT.
    assert_eq!(partial.totals.net_cents, -11_418);
    assert_eq!(partial.totals.vat_cents, -2_398);
    assert_eq!(partial.totals.gross_cents, -13_816);
}

/// The mandatory wrong-tenant proof: another tenant can neither credit a
/// document, nor learn that it exists, nor move the series it belongs to.
#[tokio::test]
async fn another_tenant_can_neither_credit_nor_discover_a_document() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "owner").await;
    let (b, _tenant_b, customer_b) = tenant_with_customer(&store, "intruder").await;

    let a_issued = issued_invoice(&a, &customer_a, &awkward_lines()).await;
    let a_draft = a
        .create_billing_invoice(&NewInvoice::for_customer(customer_a.clone()))
        .await
        .unwrap();
    let a_credit = a.create_billing_credit_note(&a_issued).await.unwrap();
    let year = a
        .billing_invoice(&a_issued)
        .await
        .unwrap()
        .unwrap()
        .invoice
        .issue_date
        .unwrap()
        .year();
    let counter_before = counter(&pool, &tenant_a, year).await;

    // Crediting A's documents — issued, draft, or A's own credit note — is the
    // plain not-found denial. Never a Conflict: that would confirm both that
    // the id exists and what state it is in.
    for id in [&a_issued, &a_draft, &a_credit] {
        assert_not_found(b.create_billing_credit_note(id).await);
    }
    // An id that never existed anywhere gets the identical answer, so trying is
    // not an existence oracle.
    assert_not_found(
        b.create_billing_credit_note(&BillingInvoiceId::new("no-such-invoice"))
            .await,
    );

    // The read side denies just as completely: no listing, and no count.
    for id in [&a_issued, &a_credit] {
        assert!(b.billing_credit_notes(id).await.unwrap().is_empty());
    }

    // A's world is untouched: the credit note it already had, no extra ones,
    // and a counter that B's attempts never moved.
    let credits = a.billing_credit_notes(&a_issued).await.unwrap();
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].invoice.id, a_credit);
    assert_eq!(counter(&pool, &tenant_a, year).await, counter_before);
    let stray: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing_invoices WHERE tenant_id <> $1 AND credits_invoice_id = $2",
    )
    .bind(tenant_a.as_str())
    .bind(a_issued.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stray, 0, "no other tenant holds a document crediting A's");

    // And the denial is about ownership, not about the operation: B credits its
    // own document of the same shape without trouble.
    let b_issued = issued_invoice(&b, &customer_b, &awkward_lines()).await;
    let b_credit = b.create_billing_credit_note(&b_issued).await.unwrap();
    assert_eq!(
        b.billing_credit_notes(&b_issued).await.unwrap()[0]
            .invoice
            .id,
        b_credit
    );
    // A cannot see B's credit note either — the denial runs both ways.
    assert!(a.billing_credit_notes(&b_issued).await.unwrap().is_empty());
    assert!(a.billing_invoice(&b_credit).await.unwrap().is_none());
}

/// The database refuses the two shapes a bug could otherwise write: a document
/// that credits itself, and one that credits another tenant's invoice.
#[tokio::test]
async fn the_table_itself_refuses_an_impossible_credit_link() {
    let store = common::test_store().await;
    let pool = raw_pool().await;
    let (a, tenant_a, customer_a) = tenant_with_customer(&store, "sql").await;
    let (b, tenant_b, customer_b) = tenant_with_customer(&store, "sqlother").await;
    let a_issued =
        issued_invoice(&a, &customer_a, &[line("Consulting", 1_000, 10_000, 2100)]).await;
    let b_issued =
        issued_invoice(&b, &customer_b, &[line("Consulting", 1_000, 10_000, 2100)]).await;

    // A one-row cycle: every walk of the credit chain would either loop or have
    // to defend itself against a state the business can never be in.
    let itself = sqlx::query(
        "UPDATE billing_invoices SET is_credit_note = true, credits_invoice_id = id \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_a.as_str())
    .bind(a_issued.as_str())
    .execute(&pool)
    .await;
    assert!(itself.is_err(), "a document must not credit itself");

    // A cross-tenant link, refused by the composite foreign key rather than by
    // a WHERE clause that could one day be wrong.
    let across = sqlx::query(
        "UPDATE billing_invoices SET is_credit_note = true, credits_invoice_id = $3 \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_b.as_str())
    .bind(b_issued.as_str())
    .bind(a_issued.as_str())
    .execute(&pool)
    .await;
    assert!(across.is_err(), "a credit note stays inside its tenant");

    // The flag and the reference stay in step: neither alone is a state.
    let flag_only = sqlx::query(
        "UPDATE billing_invoices SET is_credit_note = true WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_a.as_str())
    .bind(a_issued.as_str())
    .execute(&pool)
    .await;
    assert!(flag_only.is_err(), "a credit note must name its original");

    // Deleting the tenant still cascades cleanly with a credit chain in place.
    let credit = a.create_billing_credit_note(&a_issued).await.unwrap();
    a.issue_billing_invoice(&credit).await.unwrap();
    store.delete_tenant(&tenant_a).await.unwrap();
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing_invoices WHERE tenant_id = $1")
            .bind(tenant_a.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0, "the self-reference must not block the cascade");
}
