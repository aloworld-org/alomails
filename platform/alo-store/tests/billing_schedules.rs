//! Recurring invoices (alo Billing, wave B2.11) against a real database.
//!
//! The queue item's done-when is *a time-based test with an injected clock*,
//! and the clock is the point: every entry point of
//! [`alo_store::billing_schedules`] takes `today`, so this file runs a year of
//! an arrangement's life in a second and asks what a database can settle that a
//! unit test cannot —
//!
//! - that a due run raises **drafts**, never a numbered document;
//! - that it catches up (three months missed is three drafts) and then stops;
//! - that running twice on the same day raises nothing the second time, which
//!   is the whole reason the occurrence is stamped on the document;
//! - that a paused arrangement bills nothing and resumes where it left off;
//! - that an end date is the last date it bills on;
//! - that the drafts copy the template exactly, to the cent;
//! - and that none of it crosses a tenant boundary, in either direction.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::billing_schedules::{SCHEDULE_MAX_PER_RUN, ScheduleRun};
use alo_store::{
    AccountStore, BillingCustomerId, BillingScheduleId, Cadence, InvoiceStatus, NewCustomer,
    NewLine, NewSchedule, ScheduleEdit, Store, StoreError, TenantId,
};
use time::{Date, Month};

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).unwrap()
}

/// Today, as the database sees it — the date the store validates a new
/// arrangement's start against. Everything *after* creation is driven by an
/// injected date instead, which is what lets this file bill a year in a second.
fn today() -> Date {
    time::OffsetDateTime::now_utc().date()
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

fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// A tenant with one user and one customer on 14-day terms in euro.
async fn tenant_with_customer(
    store: &Store,
    tag: &str,
) -> (AccountStore, TenantId, BillingCustomerId) {
    let tenant = store.create_tenant(&format!("sch-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@schedules.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
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

/// Two lines across two VAT rates, one with a fractional quantity: a template
/// whose totals only come out right if the copy is exact and the arithmetic
/// rounds once per rate.
fn template() -> Vec<NewLine> {
    vec![
        NewLine {
            description: "Hosting".to_owned(),
            unit: "month".to_owned(),
            qty_milli: 1_000,
            unit_price_cents: 9_900,
            vat_rate_bp: 2100,
        },
        NewLine {
            description: "Support".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 2_500,
            unit_price_cents: 8_000,
            vat_rate_bp: 900,
        },
    ]
}

/// A monthly arrangement anchored to `start`, set up now.
async fn monthly(
    account: &AccountStore,
    customer: &BillingCustomerId,
    name: &str,
    start: Date,
    end: Option<Date>,
) -> BillingScheduleId {
    account
        .create_billing_schedule(
            &NewSchedule {
                customer_id: customer.clone(),
                name: name.to_owned(),
                cadence: Cadence::Monthly,
                start_date: start,
                end_date: end,
                currency: None,
                payment_terms_days: None,
                reference: "PO-2026".to_owned(),
                note: "Thank you for your business".to_owned(),
            },
            &template(),
        )
        .await
        .unwrap()
}

fn raised(run: &[ScheduleRun]) -> usize {
    run.iter().map(|r| r.raised.len()).sum()
}

#[tokio::test]
async fn a_due_run_raises_drafts_catches_up_and_never_bills_a_period_twice() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_customer(&store, "catchup").await;

    // Anchored to the 31st, six months back: the month-end clamp and the
    // catch-up both matter, and the start is inside the backdating bound.
    let start = day(2026, Month::January, 31);
    let schedule = monthly(&account, &customer, "Hosting — monthly", start, None).await;

    // Nothing is due the day before the first occurrence.
    let runs = account
        .run_due_billing_schedules(day(2026, Month::January, 30))
        .await
        .unwrap();
    assert!(runs.is_empty(), "not due yet: {runs:?}");
    assert!(account.billing_invoices(None).await.unwrap().is_empty());

    // Mid-March: January and February have come due, March has not (the 31st
    // is still ahead), so two drafts and the next date is 31 March.
    let runs = account
        .run_due_billing_schedules(day(2026, Month::March, 15))
        .await
        .unwrap();
    assert_eq!(raised(&runs), 2, "January and February were billable");
    assert_eq!(runs[0].next_run_date, day(2026, Month::March, 31));

    let stored = account.billing_schedule(&schedule).await.unwrap().unwrap();
    assert_eq!(stored.schedule.next_run_date, day(2026, Month::March, 31));
    assert_eq!(
        stored.schedule.last_run_date,
        Some(day(2026, Month::March, 15))
    );
    assert_eq!(stored.raised_count, 2);
    assert!(!stored.schedule.is_due(day(2026, Month::March, 15)));

    // The occurrences are the anchored dates, not the day the run happened —
    // and the second one is clamped to the end of February.
    let mut occurrences: Vec<Date> = account
        .billing_invoices_from_schedule(&schedule)
        .await
        .unwrap()
        .iter()
        .map(|s| s.invoice.schedule_due_date.unwrap())
        .collect();
    occurrences.sort_unstable();
    assert_eq!(
        occurrences,
        vec![
            day(2026, Month::January, 31),
            day(2026, Month::February, 28)
        ]
    );

    // Running again the same day raises nothing: the arrangement has moved on,
    // and the second run is a no-op rather than a second January invoice.
    let again = account
        .run_due_billing_schedules(day(2026, Month::March, 15))
        .await
        .unwrap();
    assert!(
        again.is_empty(),
        "the same day cannot bill twice: {again:?}"
    );
    assert_eq!(account.billing_invoices(None).await.unwrap().len(), 2);

    // Every document raised is a DRAFT: unnumbered, undated, deletable. A
    // schedule never issues, which is the whole safety property of the feature.
    for summary in account.billing_invoices(None).await.unwrap() {
        assert_eq!(summary.invoice.status, InvoiceStatus::Draft);
        assert!(summary.invoice.number.is_none(), "a run assigned a number");
        assert!(summary.invoice.issue_date.is_none());
        assert!(summary.invoice.due_date.is_none());
        assert_eq!(summary.invoice.schedule_id.as_ref(), Some(&schedule));
    }

    // The copy is exact, to the cent, and in the template's order.
    let document = account
        .billing_invoice(&occurrence(&account, &schedule, day(2026, Month::January, 31)).await)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(document.lines.len(), 2);
    assert_eq!(document.lines[0].description, "Hosting");
    assert_eq!(document.lines[1].description, "Support");
    assert_eq!(document.lines[1].qty_milli, 2_500);
    assert_eq!(document.totals.gross_cents, stored.totals.gross_cents);
    assert_eq!(document.totals.vat_by_rate.len(), 2);
    // The header is the arrangement's snapshot, not the customer's record read
    // again: terms, reference and note all came across.
    assert_eq!(document.invoice.payment_terms_days, 14);
    assert_eq!(document.invoice.reference, "PO-2026");
    assert_eq!(document.invoice.note, "Thank you for your business");
    assert_eq!(document.invoice.currency, "EUR");
}

/// The draft this arrangement raised for one occurrence.
async fn occurrence(
    account: &AccountStore,
    schedule: &BillingScheduleId,
    due: Date,
) -> alo_store::BillingInvoiceId {
    account
        .billing_invoices_from_schedule(schedule)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.invoice.schedule_due_date == Some(due))
        .unwrap_or_else(|| panic!("no draft for {due}"))
        .invoice
        .id
}

#[tokio::test]
async fn a_paused_arrangement_bills_nothing_and_resumes_where_it_left_off() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_customer(&store, "paused").await;
    let start = day(2026, Month::February, 1);
    let schedule = monthly(&account, &customer, "Retainer", start, None).await;

    account
        .set_billing_schedule_active(&schedule, false)
        .await
        .unwrap();
    let runs = account
        .run_due_billing_schedules(day(2026, Month::April, 10))
        .await
        .unwrap();
    assert!(runs.is_empty(), "a paused arrangement bills nothing");
    assert!(account.billing_invoices(None).await.unwrap().is_empty());

    // Running the paused one by name is a no-op too, not a refusal: a run is
    // something that happens on a rhythm, and "nothing to do" is not an error.
    let run = account
        .run_billing_schedule(&schedule, day(2026, Month::April, 10))
        .await
        .unwrap();
    assert!(run.raised.is_empty());
    assert_eq!(run.next_run_date, start, "its dates were not moved");

    // Resumed, it bills the months it was under contract for — February, March
    // and April — because they were owed, not skipped.
    account
        .set_billing_schedule_active(&schedule, true)
        .await
        .unwrap();
    let run = account
        .run_billing_schedule(&schedule, day(2026, Month::April, 10))
        .await
        .unwrap();
    assert_eq!(run.raised.len(), 3);
    assert_eq!(run.next_run_date, day(2026, Month::May, 1));
}

#[tokio::test]
async fn an_end_date_is_the_last_date_it_bills_on() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_customer(&store, "ends").await;
    let start = day(2026, Month::January, 15);
    let schedule = monthly(
        &account,
        &customer,
        "Three months",
        start,
        Some(day(2026, Month::March, 15)),
    )
    .await;

    // Far past the end: three occurrences (January, February, March), then it
    // stops for good rather than billing April.
    let run = account
        .run_billing_schedule(&schedule, day(2026, Month::December, 31))
        .await
        .unwrap();
    assert_eq!(run.raised.len(), 3);
    assert_eq!(run.next_run_date, day(2026, Month::April, 15));

    let stored = account.billing_schedule(&schedule).await.unwrap().unwrap();
    assert!(stored.schedule.is_ended(), "it has run out of dates");
    assert!(
        stored.schedule.active,
        "ended is not paused — a reader must be able to tell them apart"
    );
    assert!(!stored.schedule.is_due(day(2027, Month::June, 1)));

    let run = account
        .run_billing_schedule(&schedule, day(2027, Month::June, 1))
        .await
        .unwrap();
    assert!(run.raised.is_empty(), "an ended arrangement bills nothing");

    // An end date before the start is refused rather than stored as an
    // arrangement that can never bill.
    let message = assert_validation(
        account
            .update_billing_schedule(
                &schedule,
                &ScheduleEdit {
                    name: "Three months".to_owned(),
                    cadence: Cadence::Monthly,
                    end_date: Some(day(2025, Month::December, 1)),
                    reference: String::new(),
                    note: String::new(),
                },
                None,
            )
            .await,
    );
    assert!(message.contains("ends before"), "{message}");
}

#[tokio::test]
async fn one_run_is_bounded_and_the_rest_follows_on_the_next() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_customer(&store, "bounded").await;
    // A weekly arrangement a year back has fifty-two occurrences waiting; one
    // run raises the cap and no more, and the next run continues from there.
    let start = today() - time::Duration::days(360);
    let schedule = account
        .create_billing_schedule(
            &NewSchedule {
                customer_id: customer.clone(),
                name: "Weekly".to_owned(),
                cadence: Cadence::Weekly,
                start_date: start,
                end_date: None,
                currency: None,
                payment_terms_days: None,
                reference: String::new(),
                note: String::new(),
            },
            &template(),
        )
        .await
        .unwrap();

    let first = account
        .run_billing_schedule(&schedule, today())
        .await
        .unwrap();
    assert_eq!(first.raised.len(), SCHEDULE_MAX_PER_RUN);
    let second = account
        .run_billing_schedule(&schedule, today())
        .await
        .unwrap();
    assert_eq!(second.raised.len(), SCHEDULE_MAX_PER_RUN);
    assert!(
        second.next_run_date > first.next_run_date,
        "the second run continued where the first stopped"
    );
    // Both runs' documents are stored, and each occurrence appears once.
    //
    // Deliberately not an assertion that the arrangement has *exactly* two
    // batches: this arrangement is a year overdue and therefore due, and the
    // cross-tenant sweep the suite exercises next door
    // (`the_background_sweep_runs_every_tenant_through_its_own_door`) runs
    // concurrently over every tenant, so it may legitimately raise a third
    // batch for it in between. What the bound is about is that *one run* never
    // raises more than the cap — which the two assertions above state exactly.
    let stored = account
        .billing_invoices_from_schedule(&schedule)
        .await
        .unwrap();
    for id in first.raised.iter().chain(second.raised.iter()) {
        assert!(
            stored.iter().any(|stored| stored.invoice.id == *id),
            "a draft a run reported was not stored"
        );
    }
    assert!(stored.len() >= SCHEDULE_MAX_PER_RUN * 2);
    let occurrences: std::collections::HashSet<_> = stored
        .iter()
        .map(|stored| stored.invoice.schedule_due_date)
        .collect();
    assert_eq!(
        occurrences.len(),
        stored.len(),
        "no period was billed twice, whatever raised it"
    );
}

#[tokio::test]
async fn an_arrangement_is_refused_what_it_cannot_bill() {
    let store = common::test_store().await;
    let (account, _tenant, customer) = tenant_with_customer(&store, "refusals").await;

    // A template with no lines would be a standing instruction to raise
    // documents worth nothing.
    let message = assert_validation(
        account
            .create_billing_schedule(
                &NewSchedule {
                    customer_id: customer.clone(),
                    name: "Empty".to_owned(),
                    cadence: Cadence::Monthly,
                    start_date: today(),
                    end_date: None,
                    currency: None,
                    payment_terms_days: None,
                    reference: String::new(),
                    note: String::new(),
                },
                &[],
            )
            .await,
    );
    assert!(message.contains("at least one line"), "{message}");

    // A start date years back would mean years of drafts nobody asked for.
    let message = assert_validation(
        account
            .create_billing_schedule(
                &NewSchedule {
                    customer_id: customer.clone(),
                    name: "Backdated".to_owned(),
                    cadence: Cadence::Monthly,
                    start_date: today() - time::Duration::days(400),
                    end_date: None,
                    currency: None,
                    payment_terms_days: None,
                    reference: String::new(),
                    note: String::new(),
                },
                &template(),
            )
            .await,
    );
    assert!(message.contains("in the past"), "{message}");

    // An arrangement that has raised documents is paused, never deleted: the
    // documents point back at it.
    let schedule = monthly(&account, &customer, "Live", today(), None).await;
    account
        .delete_billing_schedule(&schedule)
        .await
        .unwrap_or_else(|e| panic!("an arrangement that has raised nothing is deletable: {e:?}"));

    let schedule = monthly(&account, &customer, "Live again", today(), None).await;
    account
        .run_billing_schedule(&schedule, today())
        .await
        .unwrap();
    let message = assert_conflict(account.delete_billing_schedule(&schedule).await);
    assert!(message.contains("pause it"), "{message}");
    // And it is still there, with its documents.
    assert_eq!(
        account
            .billing_invoices_from_schedule(&schedule)
            .await
            .unwrap()
            .len(),
        1
    );

    // Emptying the template of a live arrangement is refused for the same
    // reason it could not be created empty.
    let message = assert_validation(
        account
            .update_billing_schedule(
                &schedule,
                &ScheduleEdit {
                    name: "Live again".to_owned(),
                    cadence: Cadence::Monthly,
                    end_date: None,
                    reference: String::new(),
                    note: String::new(),
                },
                Some(&[]),
            )
            .await,
    );
    assert!(message.contains("at least one line"), "{message}");
}

#[tokio::test]
async fn one_tenants_arrangement_is_invisible_and_untouchable_to_another() {
    let store = common::test_store().await;
    let (alpha, _alpha_tenant, alpha_customer) = tenant_with_customer(&store, "alpha").await;
    let (beta, _beta_tenant, beta_customer) = tenant_with_customer(&store, "beta").await;

    let alpha_schedule = monthly(&alpha, &alpha_customer, "Alpha hosting", today(), None).await;

    // Read: absent, exactly as a made-up id is — never an existence oracle.
    assert!(
        beta.billing_schedule(&alpha_schedule)
            .await
            .unwrap()
            .is_none()
    );
    assert!(beta.billing_schedules().await.unwrap().is_empty());
    assert!(
        beta.billing_invoices_from_schedule(&alpha_schedule)
            .await
            .unwrap()
            .is_empty()
    );

    // Write: every door refuses with the same `NotFound` an absent id gets.
    assert_not_found(
        beta.update_billing_schedule(
            &alpha_schedule,
            &ScheduleEdit {
                name: "Taken over".to_owned(),
                cadence: Cadence::Yearly,
                end_date: None,
                reference: String::new(),
                note: String::new(),
            },
            Some(&template()),
        )
        .await,
    );
    assert_not_found(
        beta.set_billing_schedule_active(&alpha_schedule, false)
            .await,
    );
    assert_not_found(beta.delete_billing_schedule(&alpha_schedule).await);
    assert_not_found(beta.run_billing_schedule(&alpha_schedule, today()).await);

    // A customer of alpha's cannot be billed by beta's arrangement either.
    assert_not_found(
        beta.create_billing_schedule(
            &NewSchedule {
                customer_id: alpha_customer.clone(),
                name: "Somebody else's customer".to_owned(),
                cadence: Cadence::Monthly,
                start_date: today(),
                end_date: None,
                currency: None,
                payment_terms_days: None,
                reference: String::new(),
                note: String::new(),
            },
            &template(),
        )
        .await,
    );

    // Beta's own run bills only beta's arrangements, and alpha's is untouched.
    let beta_schedule = monthly(&beta, &beta_customer, "Beta hosting", today(), None).await;
    let runs = beta.run_due_billing_schedules(today()).await.unwrap();
    assert_eq!(raised(&runs), 1);
    assert_eq!(runs[0].schedule_id, beta_schedule);
    assert!(
        alpha.billing_invoices(None).await.unwrap().is_empty(),
        "beta's run raised a document for alpha"
    );

    let stored = alpha
        .billing_schedule(&alpha_schedule)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.schedule.active, "beta paused alpha's arrangement");
    assert_eq!(stored.schedule.name, "Alpha hosting");
    assert_eq!(stored.raised_count, 0);
}

#[tokio::test]
async fn the_background_sweep_runs_every_tenant_through_its_own_door() {
    let store = common::test_store().await;
    let (alpha, _alpha_tenant, alpha_customer) = tenant_with_customer(&store, "sweep-a").await;
    let (beta, _beta_tenant, beta_customer) = tenant_with_customer(&store, "sweep-b").await;

    let alpha_schedule = monthly(&alpha, &alpha_customer, "Alpha", today(), None).await;
    monthly(&beta, &beta_customer, "Beta", today(), None).await;
    // Paused arrangements are skipped by the sweep like by any other run.
    let paused = monthly(&alpha, &alpha_customer, "Alpha paused", today(), None).await;
    alpha
        .set_billing_schedule_active(&paused, false)
        .await
        .unwrap();

    // The sweep is cross-tenant; other tests may have left due arrangements
    // behind, so the assertion is about *these* tenants, not the total.
    store.sweep_billing_schedules(today()).await.unwrap();

    assert_eq!(alpha.billing_invoices(None).await.unwrap().len(), 1);
    assert_eq!(beta.billing_invoices(None).await.unwrap().len(), 1);
    assert_eq!(
        alpha
            .billing_invoices_from_schedule(&alpha_schedule)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        alpha
            .billing_invoices_from_schedule(&paused)
            .await
            .unwrap()
            .is_empty()
    );

    // The draft belongs to the colleague whose standing instruction raised it,
    // not to whoever the sweep happened to be running as.
    let raised = alpha
        .billing_invoices_from_schedule(&alpha_schedule)
        .await
        .unwrap();
    let owner = alpha
        .billing_schedule(&alpha_schedule)
        .await
        .unwrap()
        .unwrap()
        .schedule
        .created_by;
    assert_eq!(raised[0].invoice.created_by, owner);

    // And a second sweep on the same day adds nothing.
    store.sweep_billing_schedules(today()).await.unwrap();
    assert_eq!(alpha.billing_invoices(None).await.unwrap().len(), 1);
}
