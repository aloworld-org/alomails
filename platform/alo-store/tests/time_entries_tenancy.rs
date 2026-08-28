//! Tenancy proof for alo Projects time entries (Law 1: isolation is tested, not
//! assumed) — and this module's own addition to alo's vocabulary: **a person's
//! hours are personal data even inside their own tenant**, so there are two
//! denials to prove, not one.
//!
//! - *Wrong tenant*: tenant A's handle can never read, correct, delete, accept
//!   or reject tenant B's hours, nor log an hour on their board.
//! - *Wrong user*: user B's account door can never reach user A's entries —
//!   inside the same tenant, on the same team board, a clean absence rather
//!   than a refusal that would confirm somebody worked that day.
//!
//! Plus the arc the queue item requires (log, read, list, correct, delete), the
//! rules deciding which board an hour may be logged against, the rate snapshot,
//! the proposal verbs, the frozen billed entry, and the cascades.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, BillingCustomerId, NewCustomer, NewProjectClient, NewTask, NewTimeEntry,
    ProjectId, Store, StoreError, TaskId, TenantId, TimeEntryEdit, TimeEntryId, UserId,
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

/// Asserts a result is a conflict — a well-formed request that disagrees with
/// the state of the world.
fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>, rule: &str) {
    match result {
        Err(StoreError::Conflict(msg)) => {
            assert!(msg.contains(rule), "expected {rule:?} in {msg:?}");
        }
        other => panic!("expected Conflict({rule:?}), got: {other:?}"),
    }
}

/// A day in the middle of a working week.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("time-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@time.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@time.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// A team board that is client work at `rate_cents` an hour, and the customer
/// it is worked for.
async fn engagement(
    account: &AccountStore,
    name: &str,
    rate_cents: Option<i64>,
) -> (ProjectId, BillingCustomerId) {
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
                rate_cents,
                ..NewProjectClient::for_customer(customer.clone())
            },
        )
        .await
        .unwrap();
    (project, customer)
}

/// Direct pool access, for the assertions that must read or plant rows rather
/// than go through the tenant-predicated API (a cascade is a claim about the
/// table, not the view; and nothing bills an hour until B3.06).
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

#[tokio::test]
async fn an_hour_is_logged_read_corrected_and_removed() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "arc").await;
    let (project, _) = engagement(&a, "Portal rebuild", Some(9_500)).await;
    let task = a
        .create_task(
            &project,
            &NewTask {
                title: "Wire the login".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();

    // ---- log -------------------------------------------------------------
    let logged = a
        .log_time(&NewTimeEntry {
            task_id: Some(task.clone()),
            note: "  Login screen and its tests  ".to_owned(),
            ..NewTimeEntry::worked(project.clone(), day(3), 90)
        })
        .await
        .unwrap();
    assert_eq!(logged.project_id, project);
    assert_eq!(logged.task_id, Some(task.clone()));
    assert_eq!(logged.work_date, day(3));
    assert_eq!(logged.minutes, 90);
    assert!(logged.billable, "a client project's hours are chargeable");
    assert_eq!(
        logged.rate_cents,
        Some(9_500),
        "the engagement's rate is snapshotted onto the hour"
    );
    assert_eq!(logged.currency.as_deref(), Some("EUR"));
    assert_eq!(logged.note, "Login screen and its tests", "trimmed");
    assert!(!logged.is_proposed() && !logged.is_billed() && logged.is_rated());
    assert!(logged.started_at.is_none(), "a manual entry has no timer");

    // ---- read + list ------------------------------------------------------
    let read = a.time_entry(&logged.id).await.unwrap().unwrap();
    assert_eq!(read.minutes, 90);
    assert_eq!(read.rate_cents, Some(9_500));
    let week = a.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!(week[0].id, logged.id);
    assert!(
        a.time_entries(day(10), day(16), None)
            .await
            .unwrap()
            .is_empty(),
        "another week is another question"
    );
    assert_eq!(
        a.time_entries(day(3), day(9), Some(&project))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        a.time_entries(day(3), day(9), Some(&ProjectId::generate()))
            .await
            .unwrap()
            .is_empty()
    );
    // The day itself is included at both ends — a week that drops its Monday
    // is a week an employee will dispute.
    assert_eq!(a.time_entries(day(3), day(3), None).await.unwrap().len(), 1);
    assert_invalid(
        a.time_entries(day(9), day(3), None).await,
        "must not be before its start",
    );

    // ---- correct ----------------------------------------------------------
    let corrected = a
        .edit_time_entry(
            &logged.id,
            &TimeEntryEdit {
                work_date: day(4),
                task_id: None,
                minutes: 120,
                billable: false,
                note: "Login screen, unbillable rework".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(corrected.work_date, day(4));
    assert_eq!(corrected.minutes, 120);
    assert!(!corrected.billable);
    assert_eq!(corrected.task_id, None, "the task link can be dropped");
    assert_eq!(
        corrected.rate_cents,
        Some(9_500),
        "correcting the work never reprices it"
    );
    assert_eq!(corrected.created_at, logged.created_at);
    assert!(corrected.updated_at >= logged.updated_at);

    // ---- remove -----------------------------------------------------------
    a.delete_time_entry(&logged.id).await.unwrap();
    assert!(a.time_entry(&logged.id).await.unwrap().is_none());
    assert_not_found(a.delete_time_entry(&logged.id).await);
    assert!(
        a.time_entries(day(1), day(28), None)
            .await
            .unwrap()
            .is_empty()
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn another_tenant_can_never_reach_our_hours() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "iso-a").await;
    let (b, t2) = tenant_with_user(&store, "iso-b").await;

    let (our_project, _) = engagement(&a, "Portal rebuild", Some(9_500)).await;
    let (their_project, _) = engagement(&b, "Their work", Some(4_000)).await;
    let ours = a
        .log_time(&NewTimeEntry::worked(our_project.clone(), day(3), 60))
        .await
        .unwrap();
    let our_proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(our_project.clone(), day(3), 30)
        })
        .await
        .unwrap();

    // ---- B reaching A's hours --------------------------------------------
    assert!(
        b.time_entry(&ours.id).await.unwrap().is_none(),
        "our hour is invisible, not merely unreadable"
    );
    assert!(
        b.time_entries(day(1), day(28), None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        b.time_entries(day(1), day(28), Some(&our_project))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(b.time_entry_proposals().await.unwrap().is_empty());
    assert_not_found(
        b.edit_time_entry(
            &ours.id,
            &TimeEntryEdit {
                work_date: day(3),
                task_id: None,
                minutes: 1,
                billable: false,
                note: String::new(),
            },
        )
        .await,
    );
    assert_not_found(b.delete_time_entry(&ours.id).await);
    assert_not_found(b.accept_time_entry(&our_proposal.id).await);
    assert_not_found(b.reject_time_entry(&our_proposal.id).await);
    // Nor may they log an hour against our board.
    assert_not_found(
        b.log_time(&NewTimeEntry::worked(our_project.clone(), day(3), 60))
            .await,
    );

    // ---- A reaching B's ---------------------------------------------------
    assert_not_found(
        a.log_time(&NewTimeEntry::worked(their_project.clone(), day(3), 60))
            .await,
    );
    // An id that never existed answers exactly like a foreign one.
    assert_not_found(a.delete_time_entry(&TimeEntryId::generate()).await);
    assert_not_found(a.accept_time_entry(&TimeEntryId::generate()).await);
    assert!(
        a.time_entry(&TimeEntryId::generate())
            .await
            .unwrap()
            .is_none()
    );

    // Nothing above moved our hours, and B has none.
    assert_eq!(
        a.time_entries(day(1), day(28), None).await.unwrap().len(),
        2
    );
    assert_eq!(
        b.time_entries(day(1), day(28), None).await.unwrap().len(),
        0
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn a_colleagues_hours_are_invisible_inside_the_same_tenant() {
    let store = common::test_store().await;
    let (mine, t1) = tenant_with_user(&store, "own-a").await;
    let (theirs, their_user) = second_user(&store, &t1, "own-b").await;

    // One shared team board — the point is that sharing the board never shares
    // the hours worked on it.
    let (project, _) = engagement(&mine, "Portal rebuild", Some(9_500)).await;
    let my_hour = mine
        .log_time(&NewTimeEntry::worked(project.clone(), day(3), 60))
        .await
        .unwrap();
    let their_hour = theirs
        .log_time(&NewTimeEntry {
            note: "Their afternoon".to_owned(),
            ..NewTimeEntry::worked(project.clone(), day(3), 120)
        })
        .await
        .unwrap();
    assert_eq!(
        their_hour.user_id, their_user,
        "an entry is stamped with the door that wrote it, never with request input"
    );
    assert_eq!(
        their_hour.rate_cents,
        Some(9_500),
        "a colleague's hour is priced by the same engagement"
    );

    // Each door sees exactly one week: their own.
    let my_week = mine.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(my_week.len(), 1);
    assert_eq!(my_week[0].id, my_hour.id);
    let their_week = theirs.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(their_week.len(), 1);
    assert_eq!(their_week[0].id, their_hour.id);
    // Even filtered to the shared project, which is the read most likely to
    // leak: the project is shared, the hours are not.
    assert_eq!(
        mine.time_entries(day(3), day(9), Some(&project))
            .await
            .unwrap()
            .len(),
        1
    );

    // A colleague's entry is absent, not forbidden: a refusal would confirm
    // that somebody worked that day.
    assert!(mine.time_entry(&their_hour.id).await.unwrap().is_none());
    assert!(theirs.time_entry(&my_hour.id).await.unwrap().is_none());
    assert_not_found(
        mine.edit_time_entry(
            &their_hour.id,
            &TimeEntryEdit {
                work_date: day(3),
                task_id: None,
                minutes: 5,
                billable: false,
                note: String::new(),
            },
        )
        .await,
    );
    assert_not_found(mine.delete_time_entry(&their_hour.id).await);
    assert_not_found(mine.accept_time_entry(&their_hour.id).await);
    assert_not_found(mine.reject_time_entry(&their_hour.id).await);

    // …and their hour is still there, unchanged, after all of that.
    let intact = theirs.time_entry(&their_hour.id).await.unwrap().unwrap();
    assert_eq!(intact.minutes, 120);
    assert_eq!(intact.note, "Their afternoon");

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn an_hour_is_logged_against_a_board_the_worker_can_open() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "board-a").await;
    let (colleague, _) = second_user(&store, &t1, "board-b").await;

    let (team, _) = engagement(&a, "Portal rebuild", None).await;
    let my_own = a.ensure_personal_project().await.unwrap();
    let their_own = colleague.ensure_personal_project().await.unwrap();
    let shelved = a.create_task_project("Old work", None).await.unwrap();

    // A team board and my own personal one both take hours — internal work is
    // work, and it is counted even though nobody is billed for it.
    a.log_time(&NewTimeEntry::worked(team.clone(), day(3), 60))
        .await
        .unwrap();
    let personal = a
        .log_time(&NewTimeEntry::worked(my_own.clone(), day(3), 45))
        .await
        .unwrap();
    assert_eq!(
        personal.rate_cents, None,
        "a board with no client facts prices nothing"
    );
    assert_eq!(personal.currency, None);

    // A colleague's personal board is not mine to log against, and reads as
    // absent rather than as a rule I broke.
    assert_not_found(
        a.log_time(&NewTimeEntry::worked(their_own.clone(), day(3), 60))
            .await,
    );
    // An archived board takes no new hours: the Tasks module's own visibility
    // rule, applied to the hours worked on it.
    let pool = pool().await;
    sqlx::query("UPDATE task_projects SET archived = true WHERE tenant_id = $1 AND id = $2")
        .bind(t1.as_str())
        .bind(shelved.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert_not_found(
        a.log_time(&NewTimeEntry::worked(shelved.clone(), day(3), 60))
            .await,
    );
    assert_not_found(
        a.log_time(&NewTimeEntry::worked(ProjectId::generate(), day(3), 60))
            .await,
    );

    // Archiving a board afterwards does not empty somebody's timesheet: the
    // hours were worked, and remembering them is not the same as writing them.
    sqlx::query("UPDATE task_projects SET archived = true WHERE tenant_id = $1 AND id = $2")
        .bind(t1.as_str())
        .bind(team.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(a.time_entries(day(3), day(9), None).await.unwrap().len(), 2);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_task_link_belongs_to_the_entrys_own_project() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "task-a").await;
    let (b, t2) = tenant_with_user(&store, "task-b").await;

    let (portal, _) = engagement(&a, "Portal rebuild", Some(9_500)).await;
    let other = a.create_task_project("Something else", None).await.unwrap();
    let on_portal = a
        .create_task(
            &portal,
            &NewTask {
                title: "Wire the login".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();
    let elsewhere = a
        .create_task(
            &other,
            &NewTask {
                title: "Unrelated".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();
    let (their_project, _) = engagement(&b, "Their work", None).await;
    let theirs = b
        .create_task(
            &their_project,
            &NewTask {
                title: "Their task".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();

    let entry = a
        .log_time(&NewTimeEntry {
            task_id: Some(on_portal.clone()),
            ..NewTimeEntry::worked(portal.clone(), day(3), 60)
        })
        .await
        .unwrap();
    assert_eq!(entry.task_id, Some(on_portal));

    // An hour attributed to a task on another board would be counted against
    // one engagement and described by another.
    assert_invalid(
        a.log_time(&NewTimeEntry {
            task_id: Some(elsewhere.clone()),
            ..NewTimeEntry::worked(portal.clone(), day(3), 60)
        })
        .await,
        "same project",
    );
    assert_invalid(
        a.edit_time_entry(
            &entry.id,
            &TimeEntryEdit {
                work_date: day(3),
                task_id: Some(elsewhere),
                minutes: 60,
                billable: true,
                note: String::new(),
            },
        )
        .await,
        "same project",
    );
    // Another tenant's task is absent, not a rule broken.
    assert_not_found(
        a.log_time(&NewTimeEntry {
            task_id: Some(theirs),
            ..NewTimeEntry::worked(portal.clone(), day(3), 60)
        })
        .await,
    );
    assert_not_found(
        a.log_time(&NewTimeEntry {
            task_id: Some(TaskId::generate()),
            ..NewTimeEntry::worked(portal, day(3), 60)
        })
        .await,
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn a_rate_is_a_snapshot_taken_when_the_work_was_written_down() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "rate").await;
    let (project, customer) = engagement(&a, "Portal rebuild", Some(9_500)).await;

    let before = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(3), 60))
        .await
        .unwrap();
    assert_eq!(before.rate_cents, Some(9_500));

    // Repricing the engagement never rewrites an hour already logged — the
    // rule a billing line lives by, one module up.
    a.set_project_client(
        &project,
        &NewProjectClient {
            rate_cents: Some(12_000),
            currency: Some("CHF".to_owned()),
            ..NewProjectClient::for_customer(customer.clone())
        },
    )
    .await
    .unwrap();
    let unmoved = a.time_entry(&before.id).await.unwrap().unwrap();
    assert_eq!(unmoved.rate_cents, Some(9_500));
    assert_eq!(unmoved.currency.as_deref(), Some("EUR"));
    // …and the next hour takes the new price.
    let after = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(4), 60))
        .await
        .unwrap();
    assert_eq!(after.rate_cents, Some(12_000));
    assert_eq!(after.currency.as_deref(), Some("CHF"));

    // An explicit rate wins over the engagement's, and carries its currency.
    let explicit = a
        .log_time(&NewTimeEntry {
            rate_cents: Some(20_000),
            currency: Some("usd".to_owned()),
            ..NewTimeEntry::worked(project.clone(), day(5), 60)
        })
        .await
        .unwrap();
    assert_eq!(explicit.rate_cents, Some(20_000));
    assert_eq!(explicit.currency.as_deref(), Some("USD"));

    // Detaching the client facts leaves every hour already logged priced: what
    // was deleted is the claim they are billable to somebody, not the price
    // that was agreed when they were worked.
    a.clear_project_client(&project).await.unwrap();
    assert_eq!(
        a.time_entry(&before.id).await.unwrap().unwrap().rate_cents,
        Some(9_500)
    );
    let internal = a
        .log_time(&NewTimeEntry::worked(project.clone(), day(6), 60))
        .await
        .unwrap();
    assert_eq!(internal.rate_cents, None, "and the next hour is unrated");
    // A rate on a board with no client facts must say what currency it is in.
    assert_invalid(
        a.log_time(&NewTimeEntry {
            rate_cents: Some(20_000),
            ..NewTimeEntry::worked(project, day(7), 60)
        })
        .await,
        "needs a currency",
    );

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_duration_or_note_outside_its_bound_is_refused_before_the_column_sees_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "bounds").await;
    let (project, _) = engagement(&a, "Portal rebuild", Some(9_500)).await;

    for bad in [0_i64, -1, alo_store::MINUTES_MAX + 1] {
        assert_invalid(
            a.log_time(&NewTimeEntry::worked(project.clone(), day(3), bad))
                .await,
            "minutes must be between",
        );
    }
    let long_note = "n".repeat(alo_store::TIME_NOTE_MAX + 1);
    assert_invalid(
        a.log_time(&NewTimeEntry {
            note: long_note.clone(),
            ..NewTimeEntry::worked(project.clone(), day(3), 60)
        })
        .await,
        "note must be at most",
    );
    assert!(
        a.time_entries(day(1), day(28), None)
            .await
            .unwrap()
            .is_empty(),
        "not one of them wrote a row"
    );

    // The bounds themselves are inclusive: a minute of work is work, and a
    // full day is a day.
    for ok in [alo_store::MINUTES_MIN, alo_store::MINUTES_MAX] {
        let written = a
            .log_time(&NewTimeEntry {
                note: "n".repeat(alo_store::TIME_NOTE_MAX),
                ..NewTimeEntry::worked(project.clone(), day(3), ok)
            })
            .await
            .unwrap();
        assert_eq!(written.minutes, ok);
        // …and a correction is held to exactly the same rules.
        assert_invalid(
            a.edit_time_entry(
                &written.id,
                &TimeEntryEdit {
                    work_date: day(3),
                    task_id: None,
                    minutes: 0,
                    billable: true,
                    note: String::new(),
                },
            )
            .await,
            "minutes must be between",
        );
        assert_invalid(
            a.edit_time_entry(
                &written.id,
                &TimeEntryEdit {
                    work_date: day(3),
                    task_id: None,
                    minutes: 60,
                    billable: true,
                    note: long_note.clone(),
                },
            )
            .await,
            "note must be at most",
        );
    }

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_proposal_is_not_an_hour_until_somebody_accepts_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "propose").await;
    let (project, customer) = engagement(&a, "Portal rebuild", Some(9_500)).await;

    let proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            source_kind: Some("event".to_owned()),
            source_id: Some("evt-1".to_owned()),
            note: "Kickoff call".to_owned(),
            ..NewTimeEntry::worked(project.clone(), day(3), 60)
        })
        .await
        .unwrap();
    assert!(proposal.is_proposed());
    assert_eq!(
        proposal.rate_cents, None,
        "a machine's guess about somebody's Tuesday is priced by nobody"
    );
    assert_eq!(proposal.currency, None);
    assert_eq!(proposal.source_kind.as_deref(), Some("event"));
    assert_eq!(proposal.source_id.as_deref(), Some("evt-1"));

    let pending = a.time_entry_proposals().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, proposal.id);
    // The list shows it — the screen that offers a suggestion is the screen
    // that shows the week — saying plainly which it is.
    let week = a.time_entries(day(3), day(9), None).await.unwrap();
    assert_eq!(week.len(), 1);
    assert!(week[0].is_proposed());

    // The engagement is repriced before the human gets to it: acceptance
    // resolves the rate as it stands the moment the work is agreed.
    a.set_project_client(
        &project,
        &NewProjectClient {
            rate_cents: Some(11_000),
            ..NewProjectClient::for_customer(customer)
        },
    )
    .await
    .unwrap();
    let accepted = a.accept_time_entry(&proposal.id).await.unwrap();
    assert!(!accepted.is_proposed());
    assert_eq!(accepted.rate_cents, Some(11_000));
    assert_eq!(accepted.currency.as_deref(), Some("EUR"));
    assert_eq!(accepted.minutes, 60, "the suggested work itself is intact");
    assert!(a.time_entry_proposals().await.unwrap().is_empty());
    // A second accept cannot reprice a real hour.
    assert_not_found(a.accept_time_entry(&proposal.id).await);
    assert_not_found(a.reject_time_entry(&proposal.id).await);

    // A rejected suggestion is not a record of anything.
    let unwanted = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(project.clone(), day(4), 30)
        })
        .await
        .unwrap();
    a.reject_time_entry(&unwanted.id).await.unwrap();
    assert!(a.time_entry(&unwanted.id).await.unwrap().is_none());
    assert_not_found(a.reject_time_entry(&unwanted.id).await);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn an_hour_already_on_a_document_is_frozen() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "billed").await;
    let (project, _) = engagement(&a, "Portal rebuild", Some(9_500)).await;
    let entry = a
        .log_time(&NewTimeEntry::worked(project, day(3), 60))
        .await
        .unwrap();

    // Nothing writes `invoice_id` until the handoff (B3.06), so the fixture
    // plants what that transaction will: an hour that is on paper a customer
    // has read.
    let pool = pool().await;
    sqlx::query(
        "UPDATE time_entries SET invoice_id = 'inv-1', billed_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t1.as_str())
    .bind(entry.id.as_str())
    .execute(&pool)
    .await
    .unwrap();

    let billed = a.time_entry(&entry.id).await.unwrap().unwrap();
    assert!(billed.is_billed());
    assert!(billed.billed_at.is_some());
    assert_conflict(
        a.edit_time_entry(
            &entry.id,
            &TimeEntryEdit {
                work_date: day(4),
                task_id: None,
                minutes: 15,
                billable: false,
                note: String::new(),
            },
        )
        .await,
        "void or credit",
    );
    assert_conflict(a.delete_time_entry(&entry.id).await, "void or credit");
    // And nothing moved.
    let intact = a.time_entry(&entry.id).await.unwrap().unwrap();
    assert_eq!(intact.minutes, 60);
    assert_eq!(intact.work_date, day(3));

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn deleting_the_project_or_the_tenant_takes_the_hours_with_it() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "purge").await;
    let (doomed, _) = engagement(&a, "Short job", Some(9_500)).await;
    let (kept, _) = engagement(&a, "Long job", Some(9_500)).await;
    for project in [&doomed, &kept] {
        a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 60))
            .await
            .unwrap();
    }

    // The board owns the work done on it: hours on a project that no longer
    // exists are hours against nothing.
    let pool = pool().await;
    sqlx::query("DELETE FROM task_projects WHERE tenant_id = $1 AND id = $2")
        .bind(t1.as_str())
        .bind(doomed.as_str())
        .execute(&pool)
        .await
        .unwrap();
    let left = a.time_entries(day(1), day(28), None).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].project_id, kept);

    // Read the rows directly: the claim is that a tenant deletion cascaded
    // them away, not that they are hidden behind the tenant predicate.
    store.delete_tenant(&t1).await.unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM time_entries WHERE tenant_id = $1")
            .bind(t1.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the tenant's hours are purged with it");
}
