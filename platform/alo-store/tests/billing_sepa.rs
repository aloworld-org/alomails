//! Paying suppliers (alo Billing, wave B2.12): approved bills becoming one SEPA
//! credit-transfer instruction, the refusals that keep a payment from being made
//! twice or made at all, and the tenancy proof (Law 1: isolation is tested, not
//! assumed).
//!
//! The three things this suite is here to prove, in the order they matter:
//!
//! - **A bill is paid once.** The export marks what it covers, the second run
//!   over the same bill is refused, and a deliberate repeat is a different call
//!   that says so.
//! - **The plan and the record agree.** Planning writes nothing; recording
//!   re-checks under the row lock, so a bill approved-then-paid by a colleague
//!   between the two cannot slip through.
//! - **Tenant B cannot pay — or see — tenant A's bills.** Including the case
//!   that matters most here: B naming A's bill id in a payment run gets the same
//!   answer as an id that never existed, and A's bill is untouched afterwards.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_bills::{BillTotals, NewBill, Supplier};
use alo_store::billing_settings::NewBillingSettings;
use alo_store::{
    AccountStore, BillStatus, BillingBillId, EInvoiceSyntax, NewLine, PaymentFile, Store,
    StoreError, TenantId,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Date, Month, OffsetDateTime};

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

fn day(year: i32, month: u8, day: u8) -> Date {
    Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day).unwrap()
}

/// Today according to the database, which is what the store judges an execution
/// date against — never the test process's own clock, which may be a day away.
async fn today() -> Date {
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap();
    sqlx::query_scalar("SELECT CURRENT_DATE")
        .fetch_one(&pool)
        .await
        .unwrap()
}

/// A tenant with one user and a stated bank account — a tenant that can
/// instruct a payment at all.
async fn tenant(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("sepa-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@sepa.test"))
        .await
        .unwrap();
    let acc = store.for_account(tenant.clone(), user);
    acc.save_billing_settings(&NewBillingSettings {
        legal_name: "Alo Werkplaats B.V.".to_owned(),
        address_line1: "Keizersgracht 1".to_owned(),
        postal_code: "1015 CJ".to_owned(),
        city: "Amsterdam".to_owned(),
        country: "nl".to_owned(),
        email: "billing@alo.test".to_owned(),
        iban: Some("nl91 abna 0417 1643 00".to_owned()),
        bic: Some("abnanl2a".to_owned()),
        base_currency: "eur".to_owned(),
        ..NewBillingSettings::default()
    })
    .await
    .unwrap();
    (acc, tenant)
}

/// A supplier's invoice as a bill: €1331.97 payable, to a German account.
fn bill(number: &str) -> NewBill {
    NewBill {
        // Every bill in existence arrives as a file: `billing_bills` stores the
        // syntax as NOT NULL, so a hand-entered one is not writable today
        // (noted in STATE.md — it is B1.24's gap, not this item's).
        source_syntax: Some(EInvoiceSyntax::Cii),
        source_sha256: "a".repeat(64),
        credit_note: false,
        supplier: Supplier {
            name: "Müller & Söhne GmbH".to_owned(),
            vat_id: format!("DE{}", 811_907_980),
            country: "DE".to_owned(),
            iban: "DE89 3704 0044 0532 0130 00".to_owned(),
            ..Supplier::default()
        },
        number: number.to_owned(),
        issue_date: Some(day(2026, 7, 1)),
        due_date: Some(day(2026, 8, 1)),
        currency: "EUR".to_owned(),
        totals: BillTotals {
            line_total_cents: 110_080,
            tax_exclusive_cents: 110_080,
            tax_total_cents: 23_117,
            tax_inclusive_cents: 133_197,
            payable_cents: 133_197,
            ..BillTotals::default()
        },
        lines: vec![NewLine {
            description: "Beratung".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 8_000,
            unit_price_cents: 12_500,
            vat_rate_bp: 2100,
        }],
        ..NewBill::default()
    }
}

/// Imports and approves a bill, which is the state a payment run starts from.
async fn approved(acc: &AccountStore, number: &str) -> BillingBillId {
    let id = acc.create_billing_bill(&bill(number)).await.unwrap();
    acc.decide_billing_bill(&id, BillStatus::Approved)
        .await
        .unwrap();
    id
}

/// Plans and records in one go, the way the route does.
async fn export(
    acc: &AccountStore,
    ids: &[BillingBillId],
    on: Date,
    repeat: bool,
) -> Result<PaymentFile, StoreError> {
    let file = acc.plan_sepa_payment_file(ids, on, repeat).await?;
    acc.record_sepa_payment_file(&file, repeat).await?;
    Ok(file)
}

#[tokio::test]
async fn a_payment_run_covers_the_approved_bills_and_marks_them_once() {
    let store = common::test_store().await;
    let (acc, _t) = tenant(&store, "run").await;
    let on = today().await;

    let first = approved(&acc, "R-2026-77").await;
    let second = approved(&acc, "R-2026-78").await;
    // A bill nobody has decided about is not part of a run.
    let undecided = acc.create_billing_bill(&bill("R-2026-79")).await.unwrap();

    // ---- what is waiting to be paid --------------------------------------
    let payable = acc.payable_billing_bills().await.unwrap();
    assert_eq!(payable.len(), 2, "only the approved two");
    assert!(payable.iter().all(|bill| bill.exported_at.is_none()));

    // ---- the run ---------------------------------------------------------
    let file = export(&acc, &[first.clone(), second.clone()], on, false)
        .await
        .unwrap();
    assert_eq!(file.count(), 2);
    assert_eq!(file.control_sum_cents(), 266_394, "two bills of 1331.97");
    assert_eq!(file.debtor_iban, "NL91ABNA0417164300");
    assert_eq!(file.debtor_bic, "ABNANL2A");
    assert_eq!(file.execution_date, on);
    assert!(file.message_id.starts_with("ALO"));
    // The supplier's data reaches the file in its own script; folding it into
    // what a bank can spell is the writer's job, not the store's.
    assert_eq!(file.transfers[0].creditor_name, "Müller & Söhne GmbH");
    assert_eq!(file.transfers[0].creditor_iban, "DE89370400440532013000");

    // ---- the mark --------------------------------------------------------
    let marked = acc.billing_bill(&first).await.unwrap().unwrap().bill;
    assert_eq!(marked.export_message_id.as_deref(), Some(&*file.message_id));
    assert!(marked.exported_at.is_some());
    assert!(marked.exported_by.is_some());
    // …and it is not a payment: the bill is still simply approved.
    assert_eq!(marked.status, BillStatus::Approved);

    // The exported two are out of the payable list; the undecided one was
    // never in it.
    let payable = acc.payable_billing_bills().await.unwrap();
    assert!(payable.is_empty(), "{payable:?}");
    let waiting = acc.billing_bill(&undecided).await.unwrap().unwrap().bill;
    assert_eq!(waiting.status, BillStatus::Received);
    assert!(waiting.export_message_id.is_none());
}

#[tokio::test]
async fn a_bill_is_paid_once_unless_the_repeat_is_deliberate() {
    let store = common::test_store().await;
    let (acc, _t) = tenant(&store, "twice").await;
    let on = today().await;
    let id = approved(&acc, "R-2026-77").await;

    let first = export(&acc, std::slice::from_ref(&id), on, false)
        .await
        .unwrap();
    let refusal = assert_conflict(export(&acc, std::slice::from_ref(&id), on, false).await);
    assert!(refusal.contains("already in a payment file"), "{refusal}");
    assert!(refusal.contains(id.as_str()), "names the row: {refusal}");

    // The refusal changed nothing: the bill still carries the first run.
    let stored = acc.billing_bill(&id).await.unwrap().unwrap().bill;
    assert_eq!(
        stored.export_message_id.as_deref(),
        Some(&*first.message_id)
    );

    // A deliberate repeat — the bank never executed that file — is a different
    // run, and the bill now carries the new one.
    let again = export(&acc, std::slice::from_ref(&id), on, true)
        .await
        .unwrap();
    assert_ne!(again.message_id, first.message_id);
    let stored = acc.billing_bill(&id).await.unwrap().unwrap().bill;
    assert_eq!(
        stored.export_message_id.as_deref(),
        Some(&*again.message_id)
    );
}

#[tokio::test]
async fn a_bill_that_cannot_be_paid_by_credit_transfer_is_refused_by_name() {
    let store = common::test_store().await;
    let (acc, _t) = tenant(&store, "refuse").await;
    let on = today().await;

    // Undecided, and rejected: neither is a liability to pay.
    let undecided = acc.create_billing_bill(&bill("R-1")).await.unwrap();
    let message = assert_conflict(
        acc.plan_sepa_payment_file(std::slice::from_ref(&undecided), on, false)
            .await,
    );
    assert!(message.contains("has not been approved"), "{message}");
    acc.decide_billing_bill(&undecided, BillStatus::Rejected)
        .await
        .unwrap();
    let message = assert_conflict(acc.plan_sepa_payment_file(&[undecided], on, false).await);
    assert!(message.contains("was rejected"), "{message}");

    // A foreign-currency bill is a different payment product.
    let dollars = acc
        .create_billing_bill(&NewBill {
            currency: "USD".to_owned(),
            ..bill("R-2")
        })
        .await
        .unwrap();
    acc.decide_billing_bill(&dollars, BillStatus::Approved)
        .await
        .unwrap();
    let message = assert_validation(acc.plan_sepa_payment_file(&[dollars], on, false).await);
    assert!(message.contains("not in euro"), "{message}");

    // A credit note is money coming back.
    let credit = acc
        .create_billing_bill(&NewBill {
            credit_note: true,
            totals: BillTotals {
                line_total_cents: -110_080,
                tax_exclusive_cents: -110_080,
                tax_total_cents: -23_117,
                tax_inclusive_cents: -133_197,
                payable_cents: -133_197,
                ..BillTotals::default()
            },
            ..bill("R-3")
        })
        .await
        .unwrap();
    acc.decide_billing_bill(&credit, BillStatus::Approved)
        .await
        .unwrap();
    let message = assert_validation(acc.plan_sepa_payment_file(&[credit], on, false).await);
    assert!(message.contains("nothing to pay"), "{message}");

    // A supplier who never stated an account.
    let accountless = acc
        .create_billing_bill(&NewBill {
            supplier: Supplier {
                iban: String::new(),
                ..bill("R-4").supplier
            },
            ..bill("R-4")
        })
        .await
        .unwrap();
    acc.decide_billing_bill(&accountless, BillStatus::Approved)
        .await
        .unwrap();
    let message = assert_validation(acc.plan_sepa_payment_file(&[accountless], on, false).await);
    assert!(message.contains("no IBAN"), "{message}");

    // An empty run, and a date a bank cannot be given.
    let message = assert_validation(acc.plan_sepa_payment_file(&[], on, false).await);
    assert!(message.contains("at least one bill"), "{message}");
    let id = approved(&acc, "R-5").await;
    let message = assert_validation(
        acc.plan_sepa_payment_file(std::slice::from_ref(&id), on.previous_day().unwrap(), false)
            .await,
    );
    assert!(message.contains("before today"), "{message}");

    // Nothing above wrote anything: every bill is still where it was.
    assert!(
        acc.billing_bill(&id)
            .await
            .unwrap()
            .unwrap()
            .bill
            .exported_at
            .is_none()
    );
}

#[tokio::test]
async fn a_tenant_without_a_stated_account_is_told_which_field_is_missing() {
    let store = common::test_store().await;
    let tenant = store.create_tenant("sepa-blank").await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user("blank@sepa.test")
        .await
        .unwrap();
    let acc = store.for_account(tenant, user);
    let on = today().await;
    let id = approved(&acc, "R-2026-77").await;

    let message = assert_validation(
        acc.plan_sepa_payment_file(std::slice::from_ref(&id), on, false)
            .await,
    );
    assert!(message.contains("your own"), "{message}");

    // Stating the identity but not the account is still not enough.
    acc.save_billing_settings(&NewBillingSettings {
        legal_name: "Alo Werkplaats B.V.".to_owned(),
        country: "nl".to_owned(),
        base_currency: "eur".to_owned(),
        ..NewBillingSettings::default()
    })
    .await
    .unwrap();
    let message = assert_validation(acc.plan_sepa_payment_file(&[id], on, false).await);
    assert!(message.contains("your own IBAN"), "{message}");
}

#[tokio::test]
async fn one_tenants_bills_are_never_in_another_tenants_payment_run() {
    let store = common::test_store().await;
    let (a, _ta) = tenant(&store, "a").await;
    let (b, _tb) = tenant(&store, "b").await;
    let on = today().await;

    let mine = approved(&a, "R-2026-77").await;
    let theirs = approved(&b, "R-2026-90").await;

    // B cannot plan a run over A's bill, and the answer is the same as for an
    // id that never existed — no existence oracle across tenants.
    assert_not_found(
        b.plan_sepa_payment_file(std::slice::from_ref(&mine), on, false)
            .await,
    );
    assert_not_found(
        b.plan_sepa_payment_file(&[BillingBillId::new("never-existed".to_owned())], on, false)
            .await,
    );
    // Nor mixed in with one of its own, which is the shape a guessed id
    // actually arrives in.
    assert_not_found(
        b.plan_sepa_payment_file(&[theirs.clone(), mine.clone()], on, false)
            .await,
    );

    // Nor can B *record* a run naming A's bill, even holding a file that says
    // so: the record re-reads every id through B's own door.
    let mut forged = b
        .plan_sepa_payment_file(std::slice::from_ref(&theirs), on, false)
        .await
        .unwrap();
    forged.transfers[0].bill_id = mine.clone();
    assert_not_found(b.record_sepa_payment_file(&forged, false).await);

    // B's payable list is its own, and A's bill is untouched by any of it.
    let payable = b.payable_billing_bills().await.unwrap();
    assert_eq!(payable.len(), 1);
    assert_eq!(payable[0].id, theirs);
    let untouched = a.billing_bill(&mine).await.unwrap().unwrap().bill;
    assert!(untouched.exported_at.is_none(), "A's bill was not marked");
    assert_eq!(untouched.status, BillStatus::Approved);

    // And A's own run still works, unaffected by any of B's attempts.
    let file = export(&a, std::slice::from_ref(&mine), on, false)
        .await
        .unwrap();
    assert_eq!(file.count(), 1);
    assert_eq!(
        a.billing_bill(&mine)
            .await
            .unwrap()
            .unwrap()
            .bill
            .export_message_id,
        Some(file.message_id)
    );
}

#[tokio::test]
async fn recording_re_checks_what_planning_read() {
    let store = common::test_store().await;
    let (acc, _t) = tenant(&store, "race").await;
    let on = today().await;
    let id = approved(&acc, "R-2026-77").await;

    // Two bookkeepers plan the same bill at the same moment…
    let first = acc
        .plan_sepa_payment_file(std::slice::from_ref(&id), on, false)
        .await
        .unwrap();
    let second = acc
        .plan_sepa_payment_file(std::slice::from_ref(&id), on, false)
        .await
        .unwrap();
    assert_ne!(first.message_id, second.message_id);

    // …and exactly one instruction is recorded, because the record re-checks.
    acc.record_sepa_payment_file(&first, false).await.unwrap();
    let refusal = assert_conflict(acc.record_sepa_payment_file(&second, false).await);
    assert!(refusal.contains("already in a payment file"), "{refusal}");

    let stored = acc.billing_bill(&id).await.unwrap().unwrap().bill;
    assert_eq!(stored.export_message_id, Some(first.message_id));
    assert!(stored.exported_at.unwrap() > OffsetDateTime::UNIX_EPOCH);
}
