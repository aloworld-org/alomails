//! Tenancy proof for the project-grain hours aggregate (B3.07, Law 1:
//! isolation is tested, not assumed) — and the second, narrower claim this
//! aggregate makes, which is the one worth a suite of its own.
//!
//! `docs/design/projects.md` allows exactly one read that crosses people:
//! "project aggregates are visible to anyone who can see the project … without
//! a per-person breakdown". So there are three denials to prove, not one:
//!
//! - *Wrong tenant*: tenant A's handle can never see tenant B's project in the
//!   aggregate, and asking for it by id is the clean `NotFound`.
//! - *Wrong user's private board*: a colleague's `personal` board contributes
//!   nothing — not to the list, not by id — because a private board's hours
//!   are nobody else's business.
//! - *No breakdown*: a colleague's hours on a **shared** board are counted, and
//!   the answer says who worked them nowhere at all.
//!
//! Plus what the budget bar reads it for: billable and billed subsets, the last
//! day anybody worked, proposals excluded, and a project nobody has worked
//! answering zero rather than vanishing.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, NewTimeEntry, ProjectId, Store, StoreError, TenantId, TimeEntryId, UserId,
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

/// A day in the middle of a working week.
fn day(d: u8) -> Date {
    Date::from_calendar_date(2026, Month::August, d).expect("a real August day")
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("hours-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@hours.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@hours.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// The aggregate row for one project, or `None` when it is not in the list.
async fn listed(acc: &AccountStore, project: &ProjectId) -> Option<alo_store::ProjectHours> {
    acc.project_hours()
        .await
        .unwrap()
        .into_iter()
        .find(|h| &h.project_id == project)
}

/// Direct pool access. Marking an entry as carried onto a document is the
/// handoff's job (B3.06) and takes an approved week and a customer to get
/// there; this suite is about the *aggregate*, so the state it needs is planted
/// rather than re-earned.
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

#[tokio::test]
async fn the_aggregate_counts_the_project_and_names_nobody() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "sum").await;
    let (b, _) = second_user(&store, &t1, "sum-colleague").await;

    let project = a.create_task_project("Portal rebuild", None).await.unwrap();

    // A project nobody has worked is absent from the list — the list answers
    // "what has been worked", not "every board, restated" — but answers by id
    // with an honest zero, because the project certainly exists.
    assert!(listed(&a, &project).await.is_none());
    let empty = a.project_hours_for(&project).await.unwrap();
    assert_eq!(empty.minutes, 0);
    assert_eq!(empty.last_worked_on, None);

    // Two people, one shared board. Both sets of hours count.
    a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 90))
        .await
        .unwrap();
    let colleague_entry = b
        .log_time(&NewTimeEntry::worked(project.clone(), day(5), 60))
        .await
        .unwrap();
    // One of them was not chargeable, so the two subsets differ.
    a.log_time(&NewTimeEntry {
        billable: false,
        ..NewTimeEntry::worked(project.clone(), day(4), 45)
    })
    .await
    .unwrap();

    let hours = a.project_hours_for(&project).await.unwrap();
    assert_eq!(hours.minutes, 195, "everybody's minutes, on one project");
    assert_eq!(hours.billable_minutes, 150);
    assert_eq!(hours.billed_minutes, 0, "nothing is on a document yet");
    assert_eq!(
        hours.last_worked_on,
        Some(day(5)),
        "the most recent day anybody worked, including a colleague's"
    );

    // The colleague reads exactly the same figures — a project aggregate is
    // shared, which is the whole reason it may cross people.
    assert_eq!(b.project_hours_for(&project).await.unwrap(), hours);
    assert_eq!(listed(&b, &project).await.unwrap(), hours);

    // And there is nowhere in the answer to ask who worked when: the type has
    // no per-person field, which is the guarantee — not a filter somebody has
    // to remember. What can still be asked is one's own hours, through the
    // door that carries a user.
    assert_eq!(
        b.time_entries(day(1), day(7), Some(&project))
            .await
            .unwrap()
            .len(),
        1,
        "the personal door still shows only the reader's own entry"
    );

    // ---- billed subset ---------------------------------------------------
    let planted = pool().await;
    sqlx::query(
        "UPDATE time_entries SET invoice_id = 'inv-planted', billed_at = now() \
                 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(t1.as_str())
    .bind(colleague_entry.id.as_str())
    .execute(&planted)
    .await
    .unwrap();
    let billed = a.project_hours_for(&project).await.unwrap();
    assert_eq!(billed.billed_minutes, 60);
    assert_eq!(billed.billable_minutes, 150, "billing does not unbill");
    assert_eq!(billed.minutes, 195);

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_suggestion_is_not_an_hour_and_is_not_counted() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "proposed").await;
    let project = a.create_task_project("Migration", None).await.unwrap();

    a.log_time(&NewTimeEntry::worked(project.clone(), day(3), 120))
        .await
        .unwrap();
    let proposal = a
        .log_time(&NewTimeEntry {
            proposed: true,
            ..NewTimeEntry::worked(project.clone(), day(6), 300)
        })
        .await
        .unwrap();
    assert!(proposal.is_proposed());

    // A budget bar that filled up with the agent's suggestions would report on
    // work nobody has done (ADR 0023).
    let before = a.project_hours_for(&project).await.unwrap();
    assert_eq!(before.minutes, 120);
    assert_eq!(before.last_worked_on, Some(day(3)));

    // Accepting it is what turns it into an hour, and the aggregate moves then.
    a.accept_time_entry(&proposal.id).await.unwrap();
    let after = a.project_hours_for(&project).await.unwrap();
    assert_eq!(after.minutes, 420);
    assert_eq!(after.last_worked_on, Some(day(6)));

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn a_colleagues_private_board_is_not_in_the_aggregate() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "private").await;
    let (b, _) = second_user(&store, &t1, "private-colleague").await;

    let mine = a.ensure_personal_project().await.unwrap();
    let theirs = b.ensure_personal_project().await.unwrap();
    a.log_time(&NewTimeEntry::worked(mine.clone(), day(3), 30))
        .await
        .unwrap();
    b.log_time(&NewTimeEntry::worked(theirs.clone(), day(3), 240))
        .await
        .unwrap();

    // My own private board is mine to see.
    assert_eq!(a.project_hours_for(&mine).await.unwrap().minutes, 30);
    assert!(listed(&a, &mine).await.is_some());

    // A colleague's is not — the same denial an id that never existed gets,
    // because "that is a personal project" would already confirm a row this
    // reader may not see.
    assert_not_found(a.project_hours_for(&theirs).await);
    assert!(listed(&a, &theirs).await.is_none());
    assert_not_found(b.project_hours_for(&mine).await);
    assert!(listed(&b, &mine).await.is_none());

    store.delete_tenant(&t1).await.unwrap();
}

#[tokio::test]
async fn another_tenants_project_is_never_in_the_answer() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "wrong-a").await;
    let (b, t2) = tenant_with_user(&store, "wrong-b").await;

    let theirs = a
        .create_task_project("Their engagement", None)
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(theirs.clone(), day(3), 480))
        .await
        .unwrap();
    let ours = b.create_task_project("Our engagement", None).await.unwrap();
    b.log_time(&NewTimeEntry::worked(ours.clone(), day(3), 15))
        .await
        .unwrap();

    // By id: a clean denial, never data and never a 500.
    assert_not_found(b.project_hours_for(&theirs).await);
    assert_not_found(a.project_hours_for(&ours).await);
    // In the list: the other tenant's project simply is not there, and the
    // reader's own figures are untouched by a neighbour who worked eight hours.
    let listing = b.project_hours().await.unwrap();
    assert!(listing.iter().all(|h| h.project_id != theirs));
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].minutes, 15);

    // An id that never existed anywhere is the same answer as a neighbour's.
    assert_not_found(
        b.project_hours_for(&ProjectId::new("no-such-project"))
            .await,
    );

    store.delete_tenant(&t1).await.unwrap();
    store.delete_tenant(&t2).await.unwrap();
}

#[tokio::test]
async fn the_list_is_ordered_by_the_most_recent_work() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "order").await;

    let stale = a
        .create_task_project("Finished in June", None)
        .await
        .unwrap();
    let live = a.create_task_project("Running now", None).await.unwrap();
    a.log_time(&NewTimeEntry::worked(stale.clone(), day(1), 60))
        .await
        .unwrap();
    a.log_time(&NewTimeEntry::worked(live.clone(), day(7), 60))
        .await
        .unwrap();

    // What somebody is working on this week sorts above what they finished last
    // month: the engagement list is read top-down.
    let listing = a.project_hours().await.unwrap();
    assert_eq!(listing.len(), 2);
    assert_eq!(listing[0].project_id, live);
    assert_eq!(listing[1].project_id, stale);

    // Deleting the entry takes its hours with it; the project stays a project.
    let entries = a.time_entries(day(1), day(7), Some(&live)).await.unwrap();
    let id: &TimeEntryId = &entries[0].id;
    a.delete_time_entry(id).await.unwrap();
    assert_eq!(a.project_hours_for(&live).await.unwrap().minutes, 0);
    assert!(listed(&a, &live).await.is_none());

    store.delete_tenant(&t1).await.unwrap();
}
