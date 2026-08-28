//! The billable handoff: approved hours become a draft invoice, and come back
//! when that document goes away (alo Projects, wave B3.06).
//!
//! Four things are proved here, and the first is law 1:
//!
//! - *Wrong tenant*: tenant A can never read tenant B's unbilled hours, bill
//!   them onto a document of A's, or release them by deleting A's own invoice.
//!   Every denial is clean — a `NotFound`, never data and never a `500`.
//! - *The rules*: an hour reaches a document only if it is active, billable, in
//!   an approved week, unbilled, worked for the invoiced customer and priced in
//!   the invoice's currency, and every refusal names how many hours broke the
//!   rule.
//! - *The arc*: unbilled → invoice draft with one line per (project, rate) whose
//!   money matches the view's, hours stamped, and the view now empty.
//! - *The release*: deleting the draft or voiding the issued document returns
//!   the hours to unbilled; a credit note deliberately does not.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::time_weeks::WeekDecision;
use alo_store::{
    AccountStore, BillingCustomerId, NewCustomer, NewProjectClient, NewTimeEntry, ProjectId, Store,
    StoreError, TenantId, TimeBilling, TimeEntryId, UserId,
};
use time::{Date, Month};

/// Asserts a result is the clean not-found denial — never data, never an
/// internal (`Db`) error.
fn assert_not_found<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    match result {
        Err(StoreError::NotFound) => {}
        Err(other) => panic!("expected NotFound, got: {other:?}"),
        Ok(value) => panic!("expected NotFound, but got data: {value:?}"),
    }
}

/// Asserts a result is a validation refusal naming the rule.
fn assert_invalid<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Validation(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Validation({rule:?}), got: {other:?}"),
    }
}

/// Asserts a result is a conflict naming the rule it broke.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Conflict({rule:?}), got: {other:?}"),
    }
}

/// A day in August 2026. The 3rd is a Monday.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// The Monday the whole suite works in.
fn monday() -> Date {
    day(3)
}

/// A tenant with one user, returning the account door, the tenant and the user.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId, UserId) {
    let tenant = store.create_tenant(&format!("bill-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@hours.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user.clone());
    common::seed_default_chart(&account).await;
    (account, tenant, user)
}

/// A customer of this tenant.
async fn customer(account: &AccountStore, name: &str) -> BillingCustomerId {
    account
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap()
}

/// A team board that is client work for `customer` at `rate` cents an hour.
async fn engagement(
    account: &AccountStore,
    name: &str,
    customer: &BillingCustomerId,
    rate: Option<i64>,
) -> ProjectId {
    let project = account.create_task_project(name, None).await.unwrap();
    account
        .set_project_client(
            &project,
            &NewProjectClient {
                rate_cents: rate,
                ..NewProjectClient::for_customer(customer.clone())
            },
        )
        .await
        .unwrap();
    project
}

/// Hands the caller's week in and has an admin approve it.
async fn approve_week(store: &Store, account: &AccountStore, tenant: &TenantId, user: &UserId) {
    let week = account.submit_week(monday()).await.unwrap();
    store
        .for_tenant(tenant.clone())
        .decide_week(&week.id, WeekDecision::Approve, user, "")
        .await
        .unwrap();
}

/// What a handoff carries: everything but the entries, which each test chooses.
fn handoff(customer: &BillingCustomerId, entry_ids: Vec<TimeEntryId>) -> TimeBilling {
    TimeBilling {
        customer_id: customer.clone(),
        vat_rate_bp: 1_900,
        currency: None,
        unit: "hour".to_owned(),
        entry_ids,
    }
}

/// The whole arc on one tenant: an approved week of hours becomes a draft
/// invoice whose lines and money are the unbilled view's, and the view empties.
#[tokio::test]
async fn approved_hours_become_a_draft_invoice_and_leave_the_unbilled_view() {
    let store = common::test_store().await;
    let (a, tenant, user) = tenant_with_user(&store, "arc").await;
    let acme = customer(&a, "Acme GmbH").await;
    let portal = engagement(&a, "Portal rebuild", &acme, Some(9_500)).await;

    // 90 + 30 minutes at the engagement's rate, and an hour at an agreed
    // premium — two lines, because the rate is part of the grouping key.
    for minutes in [90, 30] {
        a.log_time(&NewTimeEntry::worked(portal.clone(), day(3), minutes))
            .await
            .unwrap();
    }
    a.log_time(&NewTimeEntry {
        rate_cents: Some(11_000),
        ..NewTimeEntry::worked(portal.clone(), day(4), 60)
    })
    .await
    .unwrap();
    // Not chargeable: counted in the week, never on the document.
    a.log_time(&NewTimeEntry {
        billable: false,
        ..NewTimeEntry::worked(portal.clone(), day(5), 45)
    })
    .await
    .unwrap();

    // ---- nothing is billable until the week is approved --------------------
    assert!(
        a.unbilled_time(&acme, None).await.unwrap().is_empty(),
        "an hour nobody has signed off is not a client's to be charged for"
    );
    approve_week(&store, &a, &tenant, &user).await;

    // ---- the unbilled view -------------------------------------------------
    let groups = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(groups.len(), 2, "one group per (project, rate)");
    assert_eq!(groups[0].project_name, "Portal rebuild");
    assert_eq!(groups[0].minutes, 120);
    assert_eq!(groups[0].rate_cents, Some(9_500));
    assert_eq!(groups[0].currency.as_deref(), Some("EUR"));
    assert_eq!(groups[0].net_cents, Some(19_000), "two hours at €95");
    assert_eq!(groups[0].entry_ids.len(), 2);
    assert_eq!(groups[1].minutes, 60);
    assert_eq!(groups[1].net_cents, Some(11_000));

    // ---- the handoff -------------------------------------------------------
    let selected: Vec<TimeEntryId> = groups
        .iter()
        .flat_map(|group| group.entry_ids.clone())
        .collect();
    let draft = a
        .bill_time_entries(&handoff(&acme, selected.clone()))
        .await
        .unwrap();
    assert_eq!(draft.entries, 3);
    assert_eq!(draft.lines, 2);
    assert_eq!(draft.minutes, 180);

    let document = a.billing_invoice(&draft.invoice_id).await.unwrap().unwrap();
    assert!(
        document.invoice.number.is_none(),
        "the handoff raises a draft and consumes no number"
    );
    assert_eq!(document.lines.len(), 2);
    assert_eq!(document.lines[0].description, "Portal rebuild");
    assert_eq!(document.lines[0].unit, "hour");
    assert_eq!(document.lines[0].qty_milli, 2_000);
    assert_eq!(document.lines[0].unit_price_cents, 9_500);
    assert_eq!(document.lines[0].vat_rate_bp, 1_900);
    assert_eq!(document.lines[1].qty_milli, 1_000);
    assert_eq!(document.lines[1].unit_price_cents, 11_000);
    assert_eq!(
        document.totals.net_cents,
        groups.iter().filter_map(|g| g.net_cents).sum::<i64>(),
        "the view and the document are the same arithmetic"
    );

    // ---- the hours know where they went ------------------------------------
    let entries = a.time_entries(day(3), day(9), None).await.unwrap();
    let billed: Vec<_> = entries.iter().filter(|e| e.is_billed()).collect();
    assert_eq!(billed.len(), 3);
    assert!(
        billed
            .iter()
            .all(|e| e.invoice_id == Some(draft.invoice_id.clone()) && e.billed_at.is_some())
    );
    assert!(
        a.unbilled_time(&acme, None).await.unwrap().is_empty(),
        "hours on a document are no longer waiting to be billed"
    );

    // ---- billing them twice is refused, and names how many ------------------
    assert_conflict(
        a.bill_time_entries(&handoff(&acme, selected.clone())).await,
        "3 of the selected hours are already on a document",
    );

    // ---- deleting the draft gives them back --------------------------------
    a.delete_billing_invoice(&draft.invoice_id).await.unwrap();
    let released = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(released.len(), 2, "the hours are unbilled again");
    assert_eq!(
        released.iter().map(|g| g.minutes).sum::<i64>(),
        180,
        "all of them, not some of them"
    );
    assert!(
        a.time_entries(day(3), day(9), None)
            .await
            .unwrap()
            .iter()
            .all(|e| !e.is_billed()),
        "a released hour carries neither the document nor the day it went there"
    );
}

/// The document a customer holds: voiding it releases the hours, crediting it
/// deliberately does not.
#[tokio::test]
async fn voiding_releases_the_hours_and_crediting_does_not() {
    let store = common::test_store().await;
    let (a, tenant, user) = tenant_with_user(&store, "void").await;
    let acme = customer(&a, "Acme GmbH").await;
    let portal = engagement(&a, "Portal rebuild", &acme, Some(9_500)).await;
    a.log_time(&NewTimeEntry::worked(portal.clone(), day(3), 60))
        .await
        .unwrap();
    approve_week(&store, &a, &tenant, &user).await;
    let ids: Vec<TimeEntryId> = a
        .unbilled_time(&acme, None)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|group| group.entry_ids)
        .collect();

    // ---- a credit note leaves the hours where they are ----------------------
    let first = a
        .bill_time_entries(&handoff(&acme, ids.clone()))
        .await
        .unwrap();
    a.issue_billing_invoice(&first.invoice_id).await.unwrap();
    let credit = a
        .create_billing_credit_note(&first.invoice_id)
        .await
        .unwrap();
    a.issue_billing_invoice(&credit).await.unwrap();
    assert!(
        a.unbilled_time(&acme, None).await.unwrap().is_empty(),
        "crediting corrects a document; the hours stay billed against the original, or they \
         would be charged twice"
    );

    // ---- voiding the original does release them -----------------------------
    // (The credit note is itself void-able and carries no hours; the original is
    // the one that does.)
    a.void_billing_invoice(&first.invoice_id).await.unwrap();
    let released = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].minutes, 60);
}

/// Every rule an hour must satisfy, and the count each refusal names.
#[tokio::test]
async fn an_hour_reaches_a_document_only_when_every_rule_holds() {
    let store = common::test_store().await;
    let (a, tenant, user) = tenant_with_user(&store, "rules").await;
    let acme = customer(&a, "Acme GmbH").await;
    let other = customer(&a, "Beta BV").await;
    let priced = engagement(&a, "Portal rebuild", &acme, Some(9_500)).await;
    let unpriced = engagement(&a, "Discovery", &acme, None).await;
    let elsewhere = engagement(&a, "Beta migration", &other, Some(8_000)).await;
    let internal = a.create_task_project("Internal", None).await.unwrap();

    let good = a
        .log_time(&NewTimeEntry::worked(priced.clone(), day(3), 60))
        .await
        .unwrap();
    let unbillable = a
        .log_time(&NewTimeEntry {
            billable: false,
            ..NewTimeEntry::worked(priced.clone(), day(3), 30)
        })
        .await
        .unwrap();
    let unrated = a
        .log_time(&NewTimeEntry::worked(unpriced.clone(), day(4), 60))
        .await
        .unwrap();
    let foreign_customer = a
        .log_time(&NewTimeEntry::worked(elsewhere.clone(), day(4), 60))
        .await
        .unwrap();
    let internal_hour = a
        .log_time(&NewTimeEntry::worked(internal.clone(), day(4), 60))
        .await
        .unwrap();
    let usd = a
        .log_time(&NewTimeEntry {
            rate_cents: Some(10_000),
            currency: Some("usd".to_owned()),
            ..NewTimeEntry::worked(priced.clone(), day(5), 60)
        })
        .await
        .unwrap();
    let proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(priced.clone(), day(5), 30)
        })
        .await
        .unwrap();

    // ---- a week nobody approved --------------------------------------------
    assert_conflict(
        a.bill_time_entries(&handoff(&acme, vec![good.id.clone()]))
            .await,
        "1 of the selected hours are in a week that has not been approved",
    );
    approve_week(&store, &a, &tenant, &user).await;

    // ---- the selection itself ----------------------------------------------
    assert_invalid(
        a.bill_time_entries(&handoff(&acme, Vec::new())).await,
        "select at least one hour",
    );
    assert_not_found(
        a.bill_time_entries(&handoff(
            &acme,
            vec![good.id.clone(), TimeEntryId::new("no-such-hour".to_owned())],
        ))
        .await,
    );

    // ---- one rule at a time, each naming its count -------------------------
    assert_invalid(
        a.bill_time_entries(&handoff(&acme, vec![good.id.clone(), proposal.id.clone()]))
            .await,
        "1 of the selected hours are still proposals",
    );
    assert_invalid(
        a.bill_time_entries(&handoff(
            &acme,
            vec![good.id.clone(), unbillable.id.clone()],
        ))
        .await,
        "1 of the selected hours are not billable",
    );
    assert_invalid(
        a.bill_time_entries(&handoff(
            &acme,
            vec![
                good.id.clone(),
                foreign_customer.id.clone(),
                internal_hour.id.clone(),
            ],
        ))
        .await,
        "2 of the selected hours are worked for another customer",
    );
    assert_invalid(
        a.bill_time_entries(&handoff(&acme, vec![good.id.clone(), unrated.id.clone()]))
            .await,
        "1 of the selected hours carry no rate",
    );
    assert_invalid(
        a.bill_time_entries(&handoff(&acme, vec![good.id.clone(), usd.id.clone()]))
            .await,
        "1 of the selected hours are priced in another currency",
    );

    // ---- nothing was written by any of those refusals -----------------------
    assert!(
        a.billing_invoices(None).await.unwrap().is_empty(),
        "a refused handoff leaves no half-raised document behind"
    );
    assert!(
        a.time_entries(day(3), day(9), None)
            .await
            .unwrap()
            .iter()
            .all(|e| !e.is_billed()),
        "and no hour marked as billed"
    );

    // ---- the unbilled view shows what is eligible, and names what is not -----
    let groups = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(
        groups.len(),
        3,
        "the priced hour, the unrated one, and the one in another currency"
    );
    let unrated_group = groups
        .iter()
        .find(|g| g.project_name == "Discovery")
        .expect("an unpriced engagement is shown");
    assert_eq!(unrated_group.rate_cents, None);
    assert_eq!(
        unrated_group.net_cents, None,
        "an hour nobody priced is never priced at zero"
    );
    // Another customer's work is never in this customer's view.
    assert!(groups.iter().all(|g| g.project_name != "Beta migration"));
    // And the cut-off is a period boundary, not a suggestion.
    let to_monday = a.unbilled_time(&acme, Some(day(3))).await.unwrap();
    assert_eq!(to_monday.len(), 1);
    assert_eq!(to_monday[0].minutes, 60);

    // ---- an unknown customer is a clean denial ------------------------------
    assert_not_found(
        a.unbilled_time(&BillingCustomerId::new("no-such-customer".to_owned()), None)
            .await,
    );
}

/// Law 1: tenant A cannot read, bill or release tenant B's hours, and B's hours
/// never appear on A's document.
#[tokio::test]
async fn one_tenants_hours_are_invisible_and_unbillable_to_another() {
    let store = common::test_store().await;
    let (a, tenant_a, user_a) = tenant_with_user(&store, "left").await;
    let (b, tenant_b, user_b) = tenant_with_user(&store, "right").await;

    let acme_a = customer(&a, "Acme GmbH").await;
    let acme_b = customer(&b, "Acme GmbH").await;
    let project_a = engagement(&a, "Portal rebuild", &acme_a, Some(9_500)).await;
    let project_b = engagement(&b, "Portal rebuild", &acme_b, Some(9_500)).await;
    let hour_a = a
        .log_time(&NewTimeEntry::worked(project_a.clone(), day(3), 60))
        .await
        .unwrap();
    let hour_b = b
        .log_time(&NewTimeEntry::worked(project_b.clone(), day(3), 120))
        .await
        .unwrap();
    approve_week(&store, &a, &tenant_a, &user_a).await;
    approve_week(&store, &b, &tenant_b, &user_b).await;

    // ---- reads -------------------------------------------------------------
    assert_not_found(a.unbilled_time(&acme_b, None).await);
    let mine = a.unbilled_time(&acme_a, None).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].minutes, 60, "never the other tenant's 120");

    // ---- billing another tenant's hour, and billing to their customer ------
    assert_not_found(
        a.bill_time_entries(&handoff(&acme_a, vec![hour_b.id.clone()]))
            .await,
    );
    assert_not_found(
        a.bill_time_entries(&handoff(&acme_b, vec![hour_a.id.clone()]))
            .await,
    );
    assert_not_found(
        a.bill_time_entries(&handoff(&acme_b, vec![hour_b.id.clone()]))
            .await,
    );
    assert!(
        b.time_entries(day(3), day(9), None)
            .await
            .unwrap()
            .iter()
            .all(|e| !e.is_billed()),
        "no refusal of A's ever touched B's hours"
    );

    // ---- and a released document releases only its own tenant's hours ------
    let draft_a = a
        .bill_time_entries(&handoff(&acme_a, vec![hour_a.id.clone()]))
        .await
        .unwrap();
    let draft_b = b
        .bill_time_entries(&handoff(&acme_b, vec![hour_b.id.clone()]))
        .await
        .unwrap();
    a.delete_billing_invoice(&draft_a.invoice_id).await.unwrap();
    assert!(
        b.unbilled_time(&acme_b, None).await.unwrap().is_empty(),
        "B's hours stayed on B's document"
    );
    assert_not_found(a.delete_billing_invoice(&draft_b.invoice_id).await);
    let still_billed = b.time_entry(&hour_b.id).await.unwrap().unwrap();
    assert_eq!(still_billed.invoice_id, Some(draft_b.invoice_id));
}

/// A colleague's hours are billable — an invoice carries the team's work — but
/// only through a handle that holds the tenant's customer, and the aggregate
/// never names who worked.
#[tokio::test]
async fn the_document_carries_the_teams_hours_without_naming_who_worked() {
    let store = common::test_store().await;
    let (a, tenant, user_a) = tenant_with_user(&store, "team").await;
    let colleague = store
        .for_tenant(tenant.clone())
        .create_user("colleague@hours.test")
        .await
        .unwrap();
    let b = store.for_account(tenant.clone(), colleague.clone());

    let acme = customer(&a, "Acme GmbH").await;
    let portal = engagement(&a, "Portal rebuild", &acme, Some(9_500)).await;
    a.log_time(&NewTimeEntry::worked(portal.clone(), day(3), 60))
        .await
        .unwrap();
    b.log_time(&NewTimeEntry::worked(portal.clone(), day(3), 30))
        .await
        .unwrap();
    approve_week(&store, &a, &tenant, &user_a).await;

    // Only the approved week's hours are eligible: the colleague has not handed
    // theirs in, so the fold is 60 minutes, not 90.
    let groups = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].minutes, 60);

    approve_week(&store, &b, &tenant, &colleague).await;
    let groups = a.unbilled_time(&acme, None).await.unwrap();
    assert_eq!(
        groups[0].minutes, 90,
        "an invoice carries the team's hours, not the caller's"
    );
    assert_eq!(groups[0].entry_ids.len(), 2);

    let draft = a
        .bill_time_entries(&handoff(&acme, groups[0].entry_ids.clone()))
        .await
        .unwrap();
    let document = a.billing_invoice(&draft.invoice_id).await.unwrap().unwrap();
    assert_eq!(document.lines.len(), 1, "one project, one rate, one line");
    assert_eq!(document.lines[0].qty_milli, 1_500, "an hour and a half");
    assert!(
        !document.lines[0].description.contains('@'),
        "a document a customer reads names the work, never the people"
    );
    // The colleague's hour is billed, and still theirs to read.
    let theirs = b.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(theirs.len(), 1);
    assert!(theirs[0].is_billed());
}
