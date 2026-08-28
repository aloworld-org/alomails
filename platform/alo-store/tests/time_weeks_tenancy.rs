//! Tenancy proof and behaviour of the week — submit, decide, reopen, and the
//! **lock** all three put on the hours inside it (alo Projects, wave B3.05).
//!
//! Three denials, not one, because this is the module where a tenant boundary,
//! a personal-data boundary and a workflow boundary all meet:
//!
//! - *Wrong tenant*: tenant A's handle can never read, submit, decide or reopen
//!   tenant B's week, and A's inbox never shows one of B's.
//! - *Wrong user*: the personal door reaches only the caller's own week; a
//!   colleague's week inside the same tenant is not addressable at all, and
//!   locking one person's week never freezes another's hours.
//! - *Wrong state*: the lock itself — an hour in a submitted or approved week
//!   refuses to be written, corrected, moved into, moved out of, deleted, or
//!   accepted from a proposal, and each refusal names the week.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::billing_line::NewLine;
use alo_store::time_weeks::{WeekDecision, WeekStatus};
use alo_store::{
    AccountStore, NewCustomer, NewInvoice, NewProjectClient, NewTimeEntry, ProjectId, Store,
    StoreError, TenantId, TimeEntryEdit, TimeWeekId, UserId,
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

/// Asserts a result is a conflict naming the rule it broke.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Conflict({rule:?}), got: {other:?}"),
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

/// A day in August 2026. The 3rd is a Monday and the 9th the Sunday closing its
/// week — the whole suite lives in that week and the one after it.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// The Monday this suite submits.
fn monday() -> Date {
    day(3)
}

/// The Monday of the following week.
fn next_monday() -> Date {
    day(10)
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("weeks-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@weeks.test"))
        .await
        .unwrap();
    let account = store.for_account(tenant.clone(), user);
    common::seed_default_chart(&account).await;
    (account, tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@weeks.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// A team board that is client work at €95 an hour.
async fn engagement(account: &AccountStore, name: &str) -> ProjectId {
    let customer = account
        .create_billing_customer(&NewCustomer {
            name: format!("{name} customer"),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap();
    let project = account.create_task_project(name, None).await.unwrap();
    account
        .set_project_client(
            &project,
            &NewProjectClient {
                rate_cents: Some(9_500),
                ..NewProjectClient::for_customer(customer)
            },
        )
        .await
        .unwrap();
    project
}

#[tokio::test]
async fn a_week_is_submitted_decided_and_reopened() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "arc").await;
    let admin = store.for_tenant(tenant.clone());
    let project = engagement(&a, "Portal rebuild").await;
    a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 90))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry {
        billable: false,
        ..NewTimeEntry::worked(project.clone(), day(5), 30)
    })
    .await
    .unwrap();

    // ---- open is the absence of a row -------------------------------------
    assert!(
        a.timesheet_week(monday()).await.unwrap().is_none(),
        "a week nobody has submitted has no row, and that is what open means"
    );
    assert!(a.timesheet_weeks(day(3), day(10)).await.unwrap().is_empty());

    // ---- submit -----------------------------------------------------------
    let submitted = a.submit_week(monday()).await.unwrap();
    assert_eq!(submitted.status, WeekStatus::Submitted);
    assert!(submitted.status.is_locked());
    assert_eq!(submitted.week_start, monday());
    assert_eq!(submitted.week_end(), day(9), "both ends inclusive");
    assert!(submitted.submitted_at.is_some());
    assert!(submitted.decided_by.is_none() && submitted.decided_at.is_none());
    assert_conflict(a.submit_week(monday()).await, "is submitted and cannot be");

    // ---- the inbox --------------------------------------------------------
    let inbox = admin.pending_weeks().await.unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].week.id, submitted.id);
    assert_eq!(inbox[0].user_email, "arc@weeks.test");
    assert_eq!(inbox[0].minutes, 120, "every real minute in the week");
    assert_eq!(inbox[0].billable_minutes, 90);
    assert_eq!(inbox[0].projects.len(), 1);
    assert_eq!(inbox[0].projects[0].project_id, project.as_str());
    assert_eq!(inbox[0].projects[0].minutes, 120);
    assert_eq!(inbox[0].projects[0].billable_minutes, 90);

    // ---- withdraw, and submit again ---------------------------------------
    let withdrawn = a.withdraw_week(monday()).await.unwrap();
    assert_eq!(withdrawn.status, WeekStatus::Open);
    assert!(!withdrawn.status.is_locked());
    assert!(
        withdrawn.submitted_at.is_none(),
        "a week that is open is awaiting nothing"
    );
    assert!(
        admin.pending_weeks().await.unwrap().is_empty(),
        "a withdrawn week leaves the approver's inbox"
    );
    assert_conflict(a.withdraw_week(monday()).await, "is open and cannot be");
    let resubmitted = a.submit_week(monday()).await.unwrap();
    assert_eq!(
        resubmitted.id, submitted.id,
        "one row per person per week, whatever it has been through"
    );

    // ---- reject, fix, resubmit --------------------------------------------
    let rejected = admin
        .decide_week(
            &submitted.id,
            WeekDecision::Reject,
            a.user(),
            "  Thursday looks doubled  ",
        )
        .await
        .unwrap();
    assert_eq!(rejected.status, WeekStatus::Rejected);
    assert!(
        !rejected.status.is_locked(),
        "the point of a rejection is that the week can be fixed"
    );
    assert_eq!(rejected.decided_by.as_ref(), Some(a.user()));
    assert!(rejected.decided_at.is_some());
    assert_eq!(rejected.decision_note, "Thursday looks doubled", "trimmed");
    assert!(
        rejected.submitted_at.is_some(),
        "a decided week keeps the instant it was handed in"
    );
    assert_conflict(
        admin
            .decide_week(&submitted.id, WeekDecision::Approve, a.user(), "")
            .await,
        "is rejected and cannot be decided",
    );
    // The hours are the person's again.
    a.log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();
    let again = a.submit_week(monday()).await.unwrap();
    assert_eq!(again.status, WeekStatus::Submitted);
    assert!(
        again.decided_by.is_none() && again.decision_note.is_empty(),
        "a decision that no longer stands is not still displayed on the record"
    );

    // ---- approve ----------------------------------------------------------
    let approved = admin
        .decide_week(&submitted.id, WeekDecision::Approve, a.user(), "")
        .await
        .unwrap();
    assert_eq!(approved.status, WeekStatus::Approved);
    assert!(approved.status.is_locked(), "approved hours do not move");
    assert_eq!(approved.decided_by.as_ref(), Some(a.user()));
    assert!(
        admin.pending_weeks().await.unwrap().is_empty(),
        "a decided week is out of the inbox"
    );
    assert_conflict(
        a.withdraw_week(monday()).await,
        "is approved and cannot be withdrawn",
    );

    // ---- reopen -----------------------------------------------------------
    let reopened = admin.reopen_week(&submitted.id).await.unwrap();
    assert_eq!(reopened.status, WeekStatus::Open);
    assert!(reopened.decided_by.is_none() && reopened.submitted_at.is_none());
    assert_conflict(
        admin.reopen_week(&submitted.id).await,
        "has no decision to take back",
    );
    a.log_time(&NewTimeEntry::worked(project, day(6), 15))
        .await
        .unwrap();

    // ---- the caller's own list of weeks -----------------------------------
    a.submit_week(next_monday()).await.unwrap();
    let weeks = a.timesheet_weeks(day(1), day(31)).await.unwrap();
    assert_eq!(weeks.len(), 2, "only weeks that have a row");
    assert_eq!(weeks[0].week_start, monday(), "oldest first");
    assert_eq!(weeks[1].week_start, next_monday());
    assert_invalid(
        a.timesheet_weeks(day(10), day(3)).await,
        "must not be before its start",
    );
}

#[tokio::test]
async fn a_week_is_addressed_by_its_monday_and_never_rounded_to_one() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "monday").await;
    // Silently submitting the week containing a Wednesday would be submitting a
    // different week than the one asked for — the worst bug this module could
    // ship. Refused, naming the Monday that was probably meant.
    for wrong in [4, 5, 6, 7, 8, 9] {
        assert_invalid(a.submit_week(day(wrong)).await, "addressed by its Monday");
        assert_invalid(a.withdraw_week(day(wrong)).await, "2026-08-03");
        assert_invalid(
            a.timesheet_week(day(wrong)).await,
            "addressed by its Monday",
        );
    }
}

#[tokio::test]
async fn the_lock_refuses_every_write_and_both_ends_of_a_move() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "lock").await;
    let admin = store.for_tenant(tenant.clone());
    let project = engagement(&a, "Ledger migration").await;

    let inside = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();
    let outside = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(11), 60))
        .await
        .unwrap();
    let proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(project.clone(), day(5), 45)
        })
        .await
        .unwrap();
    let doomed = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(project.clone(), day(6), 15)
        })
        .await
        .unwrap();

    let week = a.submit_week(monday()).await.unwrap();

    let edit = |work_date: Date| TimeEntryEdit {
        work_date,
        task_id: None,
        minutes: 120,
        billable: true,
        note: String::new(),
    };

    for status in ["submitted", "approved"] {
        // ---- writing into the locked week ---------------------------------
        assert_conflict(
            a.log_time(&NewTimeEntry::worked(project.clone(), day(7), 30))
                .await,
            &format!("the week of 2026-08-03 is {status}"),
        );
        assert_conflict(
            a.log_time(&NewTimeEntry {
                proposed: true,
                ..NewTimeEntry::worked(project.clone(), day(7), 30)
            })
            .await,
            "hours are locked",
        );
        // ---- correcting inside it -----------------------------------------
        assert_conflict(
            a.edit_time_entry(&inside.id, &edit(day(4))).await,
            &format!("the week of 2026-08-03 is {status}"),
        );
        // ---- moving OUT of it ---------------------------------------------
        assert_conflict(
            a.edit_time_entry(&inside.id, &edit(day(11))).await,
            "2026-08-03",
        );
        // ---- moving INTO it -----------------------------------------------
        assert_conflict(
            a.edit_time_entry(&outside.id, &edit(day(4))).await,
            "2026-08-03",
        );
        // ---- deleting from it ---------------------------------------------
        assert_conflict(a.delete_time_entry(&inside.id).await, "2026-08-03");
        // ---- accepting a suggestion into it -------------------------------
        assert_conflict(a.accept_time_entry(&proposal.id).await, "2026-08-03");

        if status == "submitted" {
            admin
                .decide_week(&week.id, WeekDecision::Approve, a.user(), "")
                .await
                .unwrap();
        }
    }

    // ---- the one thing that stays legal -----------------------------------
    // A proposal is in no total, so discarding one changes nothing the approver
    // saw — and since creating one here is refused, a proposal found in a locked
    // week is a draft the lock arrived after. Refusing this too would strand it.
    a.reject_time_entry(&doomed.id).await.unwrap();
    assert!(a.time_entry(&doomed.id).await.unwrap().is_none());

    // ---- the neighbouring week was never touched --------------------------
    a.edit_time_entry(&outside.id, &edit(day(11)))
        .await
        .unwrap();
    let moved = a.time_entry(&outside.id).await.unwrap().unwrap();
    assert_eq!(moved.minutes, 120);

    // ---- and the lock lifts -----------------------------------------------
    admin.reopen_week(&week.id).await.unwrap();
    a.edit_time_entry(&inside.id, &edit(day(4))).await.unwrap();
    a.accept_time_entry(&proposal.id).await.unwrap();
    a.delete_time_entry(&inside.id).await.unwrap();
    a.log_time(&NewTimeEntry::worked(project, day(7), 30))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_week_whose_hours_are_on_a_document_refuses_to_reopen() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "billed").await;
    let admin = store.for_tenant(tenant.clone());

    let customer = a
        .create_billing_customer(&NewCustomer {
            name: "Acme GmbH".to_owned(),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap();
    let project = a.create_task_project("Acme retainer", None).await.unwrap();
    a.set_project_client(
        &project,
        &NewProjectClient {
            rate_cents: Some(9_500),
            ..NewProjectClient::for_customer(customer.clone())
        },
    )
    .await
    .unwrap();
    let entry = a
        .log_time(&NewTimeEntry::worked(project, day(4), 120))
        .await
        .unwrap();

    // A real, issued document — the hours have left the module and are on paper
    // a customer has read.
    let invoice = a
        .create_billing_invoice(&NewInvoice::for_customer(customer))
        .await
        .unwrap();
    a.set_billing_invoice_lines(
        &invoice,
        &[NewLine {
            description: "Consulting".to_owned(),
            unit: "hour".to_owned(),
            qty_milli: 2_000,
            unit_price_cents: 9_500,
            vat_rate_bp: 1_900,
        }],
    )
    .await
    .unwrap();
    let issued = a.issue_billing_invoice(&invoice).await.unwrap();
    let number = issued
        .invoice
        .number
        .clone()
        .expect("issuing draws a number");

    let week = a.submit_week(monday()).await.unwrap();
    admin
        .decide_week(&week.id, WeekDecision::Approve, a.user(), "")
        .await
        .unwrap();
    // B3.06 is what will set this; until then the handoff is planted directly,
    // because the refusal it produces is this item's rule and must be proven
    // now rather than assumed later.
    sqlx::query(
        "UPDATE time_entries SET invoice_id = $3, billed_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_str())
    .bind(entry.id.as_str())
    .bind(invoice.as_str())
    .execute(&pool().await)
    .await
    .unwrap();

    let refusal = admin.reopen_week(&week.id).await;
    assert_conflict(refusal, &number);
    assert_conflict(admin.reopen_week(&week.id).await, "1 of this week's hours");
    assert_eq!(
        a.timesheet_week(monday()).await.unwrap().unwrap().status,
        WeekStatus::Approved,
        "the refusal changed nothing"
    );
}

#[tokio::test]
async fn another_tenants_week_is_never_reachable() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "one").await;
    let (b, t2) = tenant_with_user(&store, "two").await;
    let admin_a = store.for_tenant(t1.clone());
    let admin_b = store.for_tenant(t2.clone());

    let project_b = engagement(&b, "B's engagement").await;
    b.log_time(&NewTimeEntry::worked(project_b, day(4), 60))
        .await
        .unwrap();
    let week_b = b.submit_week(monday()).await.unwrap();

    // ---- the inbox is one tenant's --------------------------------------
    assert!(
        admin_a.pending_weeks().await.unwrap().is_empty(),
        "A's approver never sees B's week"
    );
    assert_eq!(admin_b.pending_weeks().await.unwrap().len(), 1);

    // ---- B's week is not addressable from A ------------------------------
    assert!(
        admin_a.week_by_id(&week_b.id).await.unwrap().is_none(),
        "another tenant's id reads exactly like one that was never issued"
    );
    assert_not_found(
        admin_a
            .decide_week(&week_b.id, WeekDecision::Approve, a.user(), "")
            .await,
    );
    assert_not_found(admin_a.reopen_week(&week_b.id).await);
    assert_not_found(admin_a.reopen_week(&TimeWeekId::generate()).await);
    assert_eq!(
        admin_b
            .week_by_id(&week_b.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        WeekStatus::Submitted,
        "and B's own week is untouched by any of it"
    );

    // ---- A's own Monday is A's own --------------------------------------
    assert!(
        a.timesheet_week(monday()).await.unwrap().is_none(),
        "the same Monday in another tenant is a different week entirely"
    );
    let week_a = a.submit_week(monday()).await.unwrap();
    assert_ne!(week_a.id, week_b.id);
    assert_eq!(admin_b.pending_weeks().await.unwrap().len(), 1, "still one");
}

#[tokio::test]
async fn a_colleagues_week_is_not_the_callers_to_submit_or_to_be_locked_by() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "mine").await;
    let (b, b_id) = second_user(&store, &tenant, "yours").await;
    let admin = store.for_tenant(tenant.clone());
    let project = engagement(&a, "Shared board").await;

    let a_hour = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();
    let b_hour = b
        .log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();

    // A submits their own week on a board they share with B.
    let week_a = a.submit_week(monday()).await.unwrap();
    assert_conflict(a.delete_time_entry(&a_hour.id).await, "2026-08-03");

    // B's hours in the same week, on the same board, are untouched: a week is a
    // person's, not a project's.
    b.edit_time_entry(
        &b_hour.id,
        &TimeEntryEdit {
            work_date: day(5),
            task_id: None,
            minutes: 30,
            billable: true,
            note: String::new(),
        },
    )
    .await
    .unwrap();
    b.log_time(&NewTimeEntry::worked(project, day(6), 45))
        .await
        .unwrap();
    assert!(
        b.timesheet_week(monday()).await.unwrap().is_none(),
        "B has submitted nothing; A's submit did not create a week for them"
    );

    // The personal door has no argument for somebody else's week, so the only
    // thing to prove is that B's own door answers about B.
    let week_b = b.submit_week(monday()).await.unwrap();
    assert_ne!(week_a.id, week_b.id);
    assert_eq!(week_b.user_id, b_id);
    let inbox = admin.pending_weeks().await.unwrap();
    assert_eq!(inbox.len(), 2, "the approver sees both");
    assert_eq!(
        inbox[0].week.id, week_a.id,
        "oldest submission first — that is the queue"
    );
    assert_eq!(inbox[0].minutes, 60);
    assert_eq!(inbox[1].minutes, 75, "B's own two entries, and only theirs");
}

#[tokio::test]
async fn a_week_and_its_hours_are_erased_with_the_tenant() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "cascade").await;
    let project = engagement(&a, "Doomed").await;
    a.log_time(&NewTimeEntry::worked(project, day(4), 60))
        .await
        .unwrap();
    a.submit_week(monday()).await.unwrap();

    store.delete_tenant(&tenant).await.unwrap();
    let remaining: (i64,) = sqlx::query_as("SELECT count(*) FROM time_weeks WHERE tenant_id = $1")
        .bind(tenant.as_str())
        .fetch_one(&pool().await)
        .await
        .unwrap();
    assert_eq!(remaining.0, 0, "a deleted tenant leaves no week behind");
}

/// Direct pool access, for the assertions that must read or plant rows rather
/// than go through the tenant-predicated API (a cascade is a claim about the
/// table, and nothing bills an hour until B3.06).
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}
