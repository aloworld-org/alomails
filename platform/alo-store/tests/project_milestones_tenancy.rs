//! Tenancy proof for alo Projects milestones (Law 1: isolation is tested, not
//! assumed), plus the arc the queue item requires and the rules the plan is
//! made of.
//!
//! A plan is tenant-wide business data on a team board — a co-tenant reads the
//! same milestones — but an outsider tenant gets the clean `NotFound`/empty on
//! **every** path: create, list, read, update, reach, delete, place a task,
//! unplace it and read the placements. Inside a tenant the second door still
//! shuts: a colleague's *personal* board is not planable and its plan is not
//! readable, which is the same rule [`alo_store::tasks`] enforces for the board
//! itself.
//!
//! The plan-specific claims proved here: the read order is the plan's order,
//! a milestone is reached only when a human says so, a task has exactly one
//! place in the plan and only within its own project, deleting a milestone
//! unplaces work without deleting it, and deleting the board takes the plan
//! with it.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, MILESTONES_MAX, MilestoneEdit, NewMilestone, NewTask, ProjectId,
    ProjectMilestoneId, Store, StoreError, TaskId, TenantId, UserId,
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

/// A day in the wave's own quarter.
fn day(month: Month, day: u8) -> Date {
    Date::from_calendar_date(2026, month, day).unwrap()
}

/// A milestone to plan.
fn milestone(name: &str, on: Date) -> NewMilestone {
    NewMilestone {
        name: name.to_owned(),
        due_on: on,
    }
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("plan-{tag}")).await.unwrap();
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@projects.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user), tenant)
}

/// A second user of an existing tenant, on their own account door.
async fn second_user(store: &Store, tenant: &TenantId, tag: &str) -> (AccountStore, UserId) {
    let user = store
        .for_tenant(tenant.clone())
        .create_user(&format!("{tag}@projects.test"))
        .await
        .unwrap();
    (store.for_account(tenant.clone(), user.clone()), user)
}

/// A task on a board, by title.
async fn task(acc: &AccountStore, project: &ProjectId, title: &str) -> TaskId {
    acc.create_task(
        project,
        &NewTask {
            title: title.to_owned(),
            ..NewTask::default()
        },
    )
    .await
    .unwrap()
}

/// Direct pool access, for the assertions that must read rows rather than the
/// tenant-predicated API (a cascade is a claim about the table, not the view).
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_plan_is_written_read_moved_reached_and_removed() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "arc").await;
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();

    // A project with no plan says so by being empty, not by refusing.
    assert!(a.milestones(&project).await.unwrap().is_empty());

    // ---- plan ------------------------------------------------------------
    let beta = a
        .create_milestone(
            &project,
            &milestone("Beta with the pilot", day(Month::October, 15)),
        )
        .await
        .unwrap();
    let design = a
        .create_milestone(
            &project,
            &milestone("  Design signed off  ", day(Month::September, 30)),
        )
        .await
        .unwrap();
    assert_eq!(design.name, "Design signed off", "the name is trimmed");
    assert_eq!(design.project_id, project);
    assert!(design.done_at.is_none(), "a new milestone is ahead of us");
    assert_eq!(design.task_count, 0);
    assert_eq!(design.position, 1, "the next position within the project");

    // The read order is the plan's order — by date, not by when it was typed.
    let listed = a.milestones(&project).await.unwrap();
    assert_eq!(
        listed.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec![design.id.as_str(), beta.id.as_str()]
    );

    // A same-day milestone keeps the order it was planned in.
    let also_beta = a
        .create_milestone(
            &project,
            &milestone("Pilot review", day(Month::October, 15)),
        )
        .await
        .unwrap();
    let listed = a.milestones(&project).await.unwrap();
    assert_eq!(
        listed.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec![design.id.as_str(), beta.id.as_str(), also_beta.id.as_str()]
    );

    // ---- move ------------------------------------------------------------
    let moved = a
        .update_milestone(
            &design.id,
            &MilestoneEdit {
                name: "Design signed off by the client".to_owned(),
                due_on: day(Month::October, 7),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        moved.due_on,
        day(Month::October, 7),
        "a plan can be re-planned"
    );
    assert_eq!(moved.name, "Design signed off by the client");
    assert_eq!(
        moved.created_at, design.created_at,
        "moving a date is not re-planning from scratch"
    );

    // ---- reach -----------------------------------------------------------
    let reached = a.set_milestone_done(&design.id, true).await.unwrap();
    let stamp = reached
        .done_at
        .expect("a reached milestone carries its day");
    assert!(reached.is_done());
    let again = a.set_milestone_done(&design.id, true).await.unwrap();
    assert_eq!(
        again.done_at,
        Some(stamp),
        "a second click is not a second event"
    );
    let reopened = a.set_milestone_done(&design.id, false).await.unwrap();
    assert!(!reopened.is_done(), "and a plan can be un-reached");

    // Late is a date behind us that nobody has closed.
    assert!(reopened.is_late(day(Month::October, 8)));
    assert!(!reopened.is_late(day(Month::October, 7)));

    // ---- remove ----------------------------------------------------------
    a.delete_milestone(&also_beta.id).await.unwrap();
    assert!(a.milestone(&also_beta.id).await.unwrap().is_none());
    assert_not_found(a.delete_milestone(&also_beta.id).await);
    assert_eq!(a.milestones(&project).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_task_has_exactly_one_place_in_the_plan_and_only_in_its_own() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "place").await;
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();
    let other = a.create_task_project("Website", None).await.unwrap();
    let design = a
        .create_milestone(&project, &milestone("Design", day(Month::September, 30)))
        .await
        .unwrap();
    let beta = a
        .create_milestone(&project, &milestone("Beta", day(Month::October, 15)))
        .await
        .unwrap();
    let elsewhere = a
        .create_milestone(&other, &milestone("Launch", day(Month::November, 1)))
        .await
        .unwrap();

    let wireframes = task(&a, &project, "Wireframes").await;
    let copy = task(&a, &project, "Copy").await;
    let unrelated = task(&a, &other, "Pick a domain").await;

    // A plan does not reach across boards.
    assert_invalid(
        a.set_task_milestone(&wireframes, &elsewhere.id).await,
        "its own project",
    );
    assert_invalid(
        a.set_task_milestone(&unrelated, &design.id).await,
        "its own project",
    );

    a.set_task_milestone(&wireframes, &design.id).await.unwrap();
    a.set_task_milestone(&copy, &design.id).await.unwrap();
    assert_eq!(
        a.milestone(&design.id).await.unwrap().unwrap().task_count,
        2
    );

    // Placing a placed task moves it: one milestone per task, never two rows.
    a.set_task_milestone(&wireframes, &beta.id).await.unwrap();
    let placements = a.task_placements(&project).await.unwrap();
    assert_eq!(placements.len(), 2);
    let for_wireframes: Vec<_> = placements
        .iter()
        .filter(|p| p.task_id == wireframes)
        .collect();
    assert_eq!(for_wireframes.len(), 1, "one place, not two");
    assert_eq!(for_wireframes[0].milestone_id, beta.id);

    // The counts a timeline draws: closed work is counted, and it is still not
    // the same statement as "reached".
    a.move_task(&copy, "done", 0.0).await.unwrap();
    let design_now = a.milestone(&design.id).await.unwrap().unwrap();
    assert_eq!(design_now.task_count, 1);
    assert_eq!(design_now.task_done_count, 1);
    assert!(
        !design_now.is_done(),
        "every task closed is not the milestone reached"
    );

    // Unplacing leaves the task on the board.
    a.clear_task_milestone(&copy).await.unwrap();
    assert_not_found(a.clear_task_milestone(&copy).await);
    assert!(
        a.task(&copy).await.unwrap().is_some(),
        "work is never deleted"
    );
    assert_eq!(
        a.milestone(&design.id).await.unwrap().unwrap().task_count,
        0
    );

    // Deleting a milestone unplaces its tasks and deletes none of them.
    a.delete_milestone(&beta.id).await.unwrap();
    assert!(a.task(&wireframes).await.unwrap().is_some());
    assert!(a.task_placements(&project).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_rules_a_plan_is_written_under() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "rules").await;
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();

    // A name is required and bounded — a milestone is a heading, not a note.
    assert_invalid(
        a.create_milestone(&project, &milestone("   ", day(Month::October, 1)))
            .await,
        "milestone name",
    );
    let long = "x".repeat(121);
    assert_invalid(
        a.create_milestone(&project, &milestone(&long, day(Month::October, 1)))
            .await,
        "milestone name",
    );
    let planned = a
        .create_milestone(&project, &milestone("Kickoff", day(Month::September, 1)))
        .await
        .unwrap();
    assert_invalid(
        a.update_milestone(
            &planned.id,
            &MilestoneEdit {
                name: String::new(),
                due_on: day(Month::September, 1),
            },
        )
        .await,
        "milestone name",
    );

    // An archived board is not planned on — and its existing plan still reads,
    // because "what was planned" stays answerable.
    let archived = a.create_task_project("Old engagement", None).await.unwrap();
    let before = a
        .create_milestone(&archived, &milestone("Handover", day(Month::July, 1)))
        .await
        .unwrap();
    sqlx::query("UPDATE task_projects SET archived = true WHERE id = $1")
        .bind(archived.as_str())
        .execute(&pool().await)
        .await
        .unwrap();
    assert_invalid(
        a.create_milestone(&archived, &milestone("More", day(Month::July, 2)))
            .await,
        "archived",
    );
    assert!(a.milestone(&before.id).await.unwrap().is_some());
}

#[tokio::test]
async fn a_plan_a_human_reads_is_bounded_and_refuses_rather_than_truncates() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "cap").await;
    let project = a.create_task_project("Programme", None).await.unwrap();

    // Fill the plan to its cap in one statement — two hundred round trips is a
    // slow way to prove an arithmetic rule.
    sqlx::query(
        "INSERT INTO project_milestones (tenant_id, id, project_id, name, due_on, position, \
             created_by) \
         SELECT $1, 'seed-' || $2 || '-' || g, $2, 'Phase ' || g, \
             DATE '2026-09-01' + g::int, g, $3 \
         FROM generate_series(1, $4) AS g",
    )
    .bind(a.tenant().as_str())
    .bind(project.as_str())
    .bind(a.user().as_str())
    .bind(MILESTONES_MAX)
    .execute(&pool().await)
    .await
    .unwrap();
    assert_eq!(
        a.milestones(&project).await.unwrap().len(),
        usize::try_from(MILESTONES_MAX).unwrap()
    );
    assert_invalid(
        a.create_milestone(
            &project,
            &milestone("One too many", day(Month::December, 1)),
        )
        .await,
        "at most 200 milestones",
    );
}

#[tokio::test]
async fn another_tenant_reaches_nothing_of_this_plan() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "own").await;
    let (b, _) = tenant_with_user(&store, "outsider").await;

    let project = a.create_task_project("Portal rebuild", None).await.unwrap();
    let design = a
        .create_milestone(&project, &milestone("Design", day(Month::September, 30)))
        .await
        .unwrap();
    let wireframes = task(&a, &project, "Wireframes").await;
    a.set_task_milestone(&wireframes, &design.id).await.unwrap();

    // Read paths answer with nothing at all — existence is never disclosed.
    assert!(b.milestones(&project).await.unwrap().is_empty());
    assert!(b.milestone(&design.id).await.unwrap().is_none());
    assert!(b.task_placements(&project).await.unwrap().is_empty());

    // Write paths deny cleanly.
    assert_not_found(
        b.create_milestone(&project, &milestone("Theirs", day(Month::October, 1)))
            .await,
    );
    assert_not_found(
        b.update_milestone(
            &design.id,
            &MilestoneEdit {
                name: "Renamed by an outsider".to_owned(),
                due_on: day(Month::October, 1),
            },
        )
        .await,
    );
    assert_not_found(b.set_milestone_done(&design.id, true).await);
    assert_not_found(b.delete_milestone(&design.id).await);
    assert_not_found(b.set_task_milestone(&wireframes, &design.id).await);
    assert_not_found(b.clear_task_milestone(&wireframes).await);

    // And nothing they attempted changed anything.
    let still = a.milestone(&design.id).await.unwrap().unwrap();
    assert_eq!(still.name, "Design");
    assert!(still.done_at.is_none());
    assert_eq!(still.task_count, 1);

    // A co-tenant reads the same plan: a team board's plan is the tenant's.
    let (colleague, _) = second_user(&store, &t1, "colleague").await;
    assert_eq!(colleague.milestones(&project).await.unwrap().len(), 1);
    colleague
        .set_milestone_done(&design.id, true)
        .await
        .unwrap();
    assert!(a.milestone(&design.id).await.unwrap().unwrap().is_done());
}

#[tokio::test]
async fn a_colleagues_private_board_is_not_planned_on_and_its_plan_is_not_read() {
    let store = common::test_store().await;
    let (a, t1) = tenant_with_user(&store, "private").await;
    let (colleague, _) = second_user(&store, &t1, "other").await;

    // Each user's own personal board, which only they can see.
    let mine = a.ensure_personal_project().await.unwrap();
    let theirs = colleague.ensure_personal_project().await.unwrap();

    // A private board carries a plan for its owner…
    let personal = a
        .create_milestone(&mine, &milestone("Tax return", day(Month::October, 31)))
        .await
        .unwrap();
    assert_eq!(a.milestones(&mine).await.unwrap().len(), 1);

    // …and nobody else, not even inside the tenant.
    assert!(colleague.milestone(&personal.id).await.unwrap().is_none());
    assert!(colleague.milestones(&mine).await.unwrap().is_empty());
    assert_not_found(colleague.delete_milestone(&personal.id).await);
    assert_not_found(
        colleague
            .create_milestone(&mine, &milestone("Theirs", day(Month::November, 1)))
            .await,
    );
    assert_not_found(
        a.create_milestone(&theirs, &milestone("Mine", day(Month::November, 1)))
            .await,
    );
}

#[tokio::test]
async fn deleting_the_board_or_the_tenant_takes_the_plan_with_it() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "cascade").await;
    let project = a.create_task_project("Portal rebuild", None).await.unwrap();
    let design = a
        .create_milestone(&project, &milestone("Design", day(Month::September, 30)))
        .await
        .unwrap();
    let wireframes = task(&a, &project, "Wireframes").await;
    a.set_task_milestone(&wireframes, &design.id).await.unwrap();

    // Deleting the task unplaces it (the link is the task's, not the work's).
    a.delete_task(&wireframes).await.unwrap();
    assert!(a.task_placements(&project).await.unwrap().is_empty());

    // Deleting the board takes the plan.
    let pool = pool().await;
    sqlx::query("DELETE FROM task_projects WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_str())
        .bind(project.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert!(rows_for(&pool, &tenant).await.is_empty());

    // And so does deleting the tenant.
    let (a2, tenant2) = tenant_with_user(&store, "cascade2").await;
    let project2 = a2
        .create_task_project("Portal rebuild", None)
        .await
        .unwrap();
    a2.create_milestone(&project2, &milestone("Design", day(Month::September, 30)))
        .await
        .unwrap();
    assert_eq!(rows_for(&pool, &tenant2).await.len(), 1);
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant2.as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert!(rows_for(&pool, &tenant2).await.is_empty());
}

/// The raw milestone ids of one tenant, read past the tenant-predicated API.
async fn rows_for(pool: &sqlx::PgPool, tenant: &TenantId) -> Vec<String> {
    sqlx::query_scalar::<_, String>("SELECT id FROM project_milestones WHERE tenant_id = $1")
        .bind(tenant.as_str())
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A milestone id from another tenant is just a string here — the store never
/// trusts it, and this test names the fact so the next reader does not have to
/// infer it from the absence of a test.
#[tokio::test]
async fn an_id_from_elsewhere_is_never_a_key() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "forged").await;
    let forged = ProjectMilestoneId::new("this-is-not-a-key");
    assert!(a.milestone(&forged).await.unwrap().is_none());
    assert_not_found(a.delete_milestone(&forged).await);
    assert_not_found(a.set_milestone_done(&forged, true).await);
}
