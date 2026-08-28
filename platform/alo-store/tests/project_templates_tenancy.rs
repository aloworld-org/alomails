//! Tenancy proof for alo Projects templates (Law 1: isolation is tested, not
//! assumed), plus the arc the queue item requires and the rules a copy is made
//! of.
//!
//! A template is tenant-wide business data on a team board — a colleague reads
//! the same templates and may start a project from one — but an outsider tenant
//! gets the clean `NotFound`/empty on **every** path: list, read, mark, unmark
//! and instantiate, with nothing changed behind the refusal. Inside a tenant
//! the second door still shuts: a personal board cannot be marked at all, so
//! the tenant-wide list can never name a colleague's private work.
//!
//! The copy-specific claims proved here: the shape is copied and progress is
//! not, finished cards stay behind, the plan lands on the start date and every
//! other date moves with it, a template with no milestones shifts nothing, the
//! engagement's currency and rate travel but its customer never does, and the
//! copy is a new board — editing it leaves the template alone.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::{
    AccountStore, NewCustomer, NewMilestone, NewProjectClient, NewTask, ProjectId, Store,
    StoreError, TemplateInstance, TenantId, UserId,
};
use time::{Date, Month, OffsetDateTime, Time, UtcOffset};

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

/// Noon on a day, which is how a task's due date is stored.
fn noon(month: Month, at: u8) -> OffsetDateTime {
    day(month, at)
        .with_time(Time::from_hms(12, 0, 0).unwrap())
        .assume_offset(UtcOffset::UTC)
}

/// A tenant with one user, returning the account door plus the tenant id.
async fn tenant_with_user(store: &Store, tag: &str) -> (AccountStore, TenantId) {
    let tenant = store.create_tenant(&format!("tpl-{tag}")).await.unwrap();
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

/// Direct pool access, for the two assertions that must read or write rows
/// rather than the tenant-predicated API (archiving a board has no store call,
/// and a cascade is a claim about the table, not the view).
async fn pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&common::database_url())
        .await
        .unwrap()
}

/// Archives a board — the one fact this wave needs that no store call writes.
async fn archive(tenant: &TenantId, project: &ProjectId) {
    sqlx::query("UPDATE task_projects SET archived = true WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_str())
        .bind(project.as_str())
        .execute(&pool().await)
        .await
        .unwrap();
}

/// The template board every copy test starts from: three cards (one of them
/// finished), a checklist, a label, two milestones and two placements.
struct Fixture {
    project: ProjectId,
    kickoff_title: String,
}

async fn template_board(acc: &AccountStore, name: &str) -> Fixture {
    let project = acc
        .create_task_project(name, Some("#4b83c4"))
        .await
        .unwrap();
    let kickoff = acc
        .create_task(
            &project,
            &NewTask {
                title: "Kickoff workshop".to_owned(),
                description: Some("Agenda in the shared drive".to_owned()),
                priority: Some("high".to_owned()),
                due_at: Some(noon(Month::September, 3)),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();
    let wireframes = acc
        .create_task(
            &project,
            &NewTask {
                title: "Wireframes".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();
    let finished = acc
        .create_task(
            &project,
            &NewTask {
                title: "Last year's retro".to_owned(),
                ..NewTask::default()
            },
        )
        .await
        .unwrap();
    acc.move_task(&wireframes, "doing", 1.0).await.unwrap();
    acc.move_task(&finished, "done", 1.0).await.unwrap();

    let step = acc.add_subtask(&kickoff, "Book the room").await.unwrap();
    acc.set_subtask_done(&kickoff, &step, true).await.unwrap();
    let label = acc.create_task_label("Design", None).await.unwrap();
    acc.add_task_label(&kickoff, &label).await.unwrap();

    let start = acc
        .create_milestone(
            &project,
            &NewMilestone {
                name: "Kickoff".to_owned(),
                due_on: day(Month::September, 1),
            },
        )
        .await
        .unwrap();
    let launch = acc
        .create_milestone(
            &project,
            &NewMilestone {
                name: "Launch".to_owned(),
                due_on: day(Month::September, 15),
            },
        )
        .await
        .unwrap();
    acc.set_task_milestone(&kickoff, &start.id).await.unwrap();
    acc.set_task_milestone(&finished, &launch.id).await.unwrap();

    Fixture {
        project,
        kickoff_title: "Kickoff workshop".to_owned(),
    }
}

#[tokio::test]
async fn a_copy_carries_the_shape_and_lands_on_the_start_date() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "arc").await;
    let fixture = template_board(&a, "Website relaunch").await;

    // ---- mark ------------------------------------------------------------
    assert!(a.templates().await.unwrap().is_empty(), "nothing yet");
    let template = a.mark_template(&fixture.project).await.unwrap();
    assert_eq!(template.name, "Website relaunch");
    assert_eq!(template.color.as_deref(), Some("#4b83c4"));
    assert!(!template.archived);
    assert_eq!(
        template.task_count, 2,
        "what a copy would carry: the finished card is not part of the shape"
    );
    assert_eq!(template.milestone_count, 2);
    assert_eq!(a.templates().await.unwrap().len(), 1);

    // Marking twice leaves one mark, and keeps the first one's date.
    let again = a.mark_template(&fixture.project).await.unwrap();
    assert_eq!(again.created_at, template.created_at);
    assert_eq!(a.templates().await.unwrap().len(), 1);

    // ---- copy ------------------------------------------------------------
    let copy = a
        .instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "  Hansen relaunch  ".to_owned(),
                starts_on: Some(day(Month::October, 1)),
                customer_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(copy.task_count, 2);
    assert_eq!(copy.milestone_count, 2);
    assert_ne!(copy.project_id, fixture.project, "a copy is a new board");

    let board = a
        .task_projects()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == copy.project_id)
        .expect("the copy is on the board list");
    assert_eq!(board.name, "Hansen relaunch", "the name is trimmed");
    assert_eq!(board.kind, "team", "a copy is shared work");
    assert_eq!(board.color.as_deref(), Some("#4b83c4"));

    // ---- the tasks -------------------------------------------------------
    let tasks = a.tasks_in_project(&copy.project_id).await.unwrap();
    assert_eq!(tasks.len(), 2, "the finished card stayed behind");
    let kickoff = tasks
        .iter()
        .find(|t| t.title == fixture.kickoff_title)
        .expect("the open work came along");
    assert_eq!(
        kickoff.description.as_deref(),
        Some("Agenda in the shared drive")
    );
    assert_eq!(kickoff.priority, "high");
    assert_eq!(kickoff.status, "todo");
    assert!(kickoff.assignee.is_none(), "a copy assigns nobody");
    assert!(kickoff.completed_at.is_none());
    assert_eq!(
        kickoff.due_at,
        Some(noon(Month::October, 3)),
        "every date moved by the plan's own delta: 1 Sep → 1 Oct is 30 days"
    );
    assert!(
        tasks.iter().any(|t| t.status == "doing"),
        "the board column is part of the shape"
    );

    // The checklist came, unticked; the label is the same tenant label.
    let subtasks = a.subtasks(&kickoff.id).await.unwrap();
    assert_eq!(subtasks.len(), 1);
    assert_eq!(subtasks[0].title, "Book the room");
    assert!(
        !subtasks[0].done,
        "a copy carries the steps, never the ticks"
    );
    let labels = a.labels_for_task(&kickoff.id).await.unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].name, "Design");

    // Nothing personal followed the shape across.
    assert!(a.task_comments(&kickoff.id).await.unwrap().is_empty());
    assert!(a.task_attachments(&kickoff.id).await.unwrap().is_empty());
    assert!(a.dependencies(&kickoff.id).await.unwrap().is_empty());

    // ---- the plan --------------------------------------------------------
    let plan = a.milestones(&copy.project_id).await.unwrap();
    assert_eq!(
        plan.iter()
            .map(|m| (m.name.clone(), m.due_on))
            .collect::<Vec<_>>(),
        vec![
            ("Kickoff".to_owned(), day(Month::October, 1)),
            ("Launch".to_owned(), day(Month::October, 15)),
        ],
        "the first milestone lands on the start date and the rest keep their spacing"
    );
    assert!(plan.iter().all(|m| !m.is_done()), "a copy reaches nothing");
    let placements = a.task_placements(&copy.project_id).await.unwrap();
    assert_eq!(
        placements.len(),
        1,
        "the finished card's place went with it"
    );
    assert_eq!(placements[0].task_id, kickoff.id);
    assert_eq!(placements[0].milestone_id, plan[0].id);

    // ---- the template is untouched --------------------------------------
    assert_eq!(
        a.tasks_in_project(&fixture.project).await.unwrap().len(),
        3,
        "the board the copy came from still has all its cards"
    );
    assert_eq!(
        a.milestones(&fixture.project).await.unwrap()[0].due_on,
        day(Month::September, 1),
        "and its own dates"
    );
}

#[tokio::test]
async fn only_a_shared_team_board_can_be_a_template() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "kind").await;
    let (colleague, _) = second_user(&store, &tenant, "kind2").await;

    // The caller's own personal board gets the honest reason…
    let mine = a.ensure_personal_project().await.unwrap();
    assert_invalid(a.mark_template(&mine).await, "personal");

    // …a colleague's reads as absent, because naming the rule would confirm a
    // row they may not see.
    let theirs = colleague.ensure_personal_project().await.unwrap();
    assert_not_found(a.mark_template(&theirs).await);
    assert!(a.template(&theirs).await.unwrap().is_none());
    assert!(
        a.templates().await.unwrap().is_empty(),
        "and no private board can ever reach the tenant-wide list"
    );

    // An archived board is refused with its reason, but a template archived
    // *after* it was marked stays a template: keeping a shape out of the board
    // list is exactly why somebody archives it.
    let project = a.create_task_project("Old shape", None).await.unwrap();
    let marked = a.mark_template(&project).await.unwrap();
    assert!(!marked.archived);
    archive(&tenant, &project).await;
    let listed = a.templates().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].archived, "and the list says so");
    let copy = a
        .instantiate_template(
            &project,
            &TemplateInstance {
                name: "From the archive".to_owned(),
                starts_on: None,
                customer_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(copy.task_count, 0);

    let fresh = a.create_task_project("Fresh", None).await.unwrap();
    archive(&tenant, &fresh).await;
    assert_invalid(a.mark_template(&fresh).await, "archived");
}

#[tokio::test]
async fn another_tenant_can_neither_see_nor_copy_a_template() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "own").await;
    let (b, _) = tenant_with_user(&store, "other").await;
    let fixture = template_board(&a, "Website relaunch").await;
    a.mark_template(&fixture.project).await.unwrap();

    // Every path, one at a time.
    assert!(b.templates().await.unwrap().is_empty());
    assert!(b.template(&fixture.project).await.unwrap().is_none());
    assert_not_found(b.mark_template(&fixture.project).await);
    assert_not_found(b.unmark_template(&fixture.project).await);
    assert_not_found(
        b.instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Stolen shape".to_owned(),
                starts_on: Some(day(Month::October, 1)),
                customer_id: None,
            },
        )
        .await,
    );

    // Nothing changed behind the refusals.
    assert_eq!(a.templates().await.unwrap().len(), 1);
    assert_eq!(a.tasks_in_project(&fixture.project).await.unwrap().len(), 3);
    assert!(
        b.task_projects()
            .await
            .unwrap()
            .iter()
            .all(|p| p.id != fixture.project),
        "and no board of ours ever appeared on theirs"
    );
}

#[tokio::test]
async fn a_colleague_reads_the_same_templates_and_may_copy_one() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "share").await;
    let (colleague, _) = second_user(&store, &tenant, "share2").await;
    let fixture = template_board(&a, "Website relaunch").await;
    a.mark_template(&fixture.project).await.unwrap();

    assert_eq!(colleague.templates().await.unwrap().len(), 1);
    let copy = colleague
        .instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Second client".to_owned(),
                starts_on: Some(day(Month::November, 2)),
                customer_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(copy.task_count, 2);
    assert_eq!(
        colleague.milestones(&copy.project_id).await.unwrap()[0].due_on,
        day(Month::November, 2)
    );
    assert_eq!(
        a.tasks_in_project(&copy.project_id).await.unwrap().len(),
        2,
        "the copy is a team board, so the whole tenant sees it"
    );
}

#[tokio::test]
async fn the_engagement_shape_travels_but_the_client_never_does() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "client").await;
    let fixture = template_board(&a, "Retainer shape").await;
    let old_customer = a
        .create_billing_customer(&NewCustomer {
            name: "Acme GmbH".to_owned(),
            country: "de".to_owned(),
            currency: "eur".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap();
    let new_customer = a
        .create_billing_customer(&NewCustomer {
            name: "Hansen BV".to_owned(),
            country: "nl".to_owned(),
            currency: "usd".to_owned(),
            ..NewCustomer::default()
        })
        .await
        .unwrap();
    a.set_project_client(
        &fixture.project,
        &NewProjectClient {
            rate_cents: Some(12_000),
            budget_minutes: Some(6_000),
            budget_cents: Some(1_200_000),
            ..NewProjectClient::for_customer(old_customer.clone())
        },
    )
    .await
    .unwrap();
    a.mark_template(&fixture.project).await.unwrap();

    // No customer stated: the copy is internal work, said by absence.
    let internal = a
        .instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Internal rebuild".to_owned(),
                starts_on: Some(day(Month::October, 1)),
                customer_id: None,
            },
        )
        .await
        .unwrap();
    assert!(
        a.project_client(&internal.project_id)
            .await
            .unwrap()
            .is_none(),
        "a template is an engagement shape, not a client"
    );

    // A customer stated: the shape's money travels with it, the client does not.
    let billed = a
        .instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Hansen retainer".to_owned(),
                starts_on: Some(day(Month::October, 1)),
                customer_id: Some(new_customer.clone()),
            },
        )
        .await
        .unwrap();
    let facts = a
        .project_client(&billed.project_id)
        .await
        .unwrap()
        .expect("client facts");
    assert_eq!(facts.customer_id, new_customer, "the caller's customer");
    assert_ne!(facts.customer_id, old_customer);
    assert_eq!(
        facts.currency, "EUR",
        "a rate is a number in a currency; the two travel together"
    );
    assert_eq!(facts.rate_cents, Some(12_000));
    assert_eq!(facts.budget_minutes, Some(6_000));
    assert_eq!(facts.budget_cents, Some(1_200_000));
    assert_eq!(facts.starts_on, Some(day(Month::October, 1)));

    // An archived customer is refused, and refused *before* anything is written.
    a.set_billing_customer_archived(&new_customer, true)
        .await
        .unwrap();
    let boards = a.task_projects().await.unwrap().len();
    assert_invalid(
        a.instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Too late".to_owned(),
                starts_on: None,
                customer_id: Some(new_customer.clone()),
            },
        )
        .await,
        "archived",
    );
    assert_eq!(
        a.task_projects().await.unwrap().len(),
        boards,
        "a refused copy leaves no board behind"
    );
}

#[tokio::test]
async fn a_template_with_no_milestones_shifts_nothing() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "anchor").await;
    let project = a.create_task_project("Undated shape", None).await.unwrap();
    a.create_task(
        &project,
        &NewTask {
            title: "Standing task".to_owned(),
            due_at: Some(noon(Month::September, 3)),
            ..NewTask::default()
        },
    )
    .await
    .unwrap();
    a.mark_template(&project).await.unwrap();

    let copy = a
        .instantiate_template(
            &project,
            &TemplateInstance {
                name: "Copy".to_owned(),
                starts_on: Some(day(Month::December, 1)),
                customer_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(copy.milestone_count, 0);
    let tasks = a.tasks_in_project(&copy.project_id).await.unwrap();
    assert_eq!(
        tasks[0].due_at,
        Some(noon(Month::September, 3)),
        "there is nothing to anchor to, so nothing is silently re-dated"
    );
}

#[tokio::test]
async fn a_copy_needs_a_name_and_is_its_own_board_afterwards() {
    let store = common::test_store().await;
    let (a, _) = tenant_with_user(&store, "name").await;
    let fixture = template_board(&a, "Website relaunch").await;
    a.mark_template(&fixture.project).await.unwrap();

    for blank in ["", "   "] {
        assert_invalid(
            a.instantiate_template(
                &fixture.project,
                &TemplateInstance {
                    name: blank.to_owned(),
                    starts_on: None,
                    customer_id: None,
                },
            )
            .await,
            "project name",
        );
    }

    let copy = a
        .instantiate_template(
            &fixture.project,
            &TemplateInstance {
                name: "Independent".to_owned(),
                starts_on: None,
                customer_id: None,
            },
        )
        .await
        .unwrap();
    let task = a.tasks_in_project(&copy.project_id).await.unwrap()[0]
        .id
        .clone();
    a.delete_task(&task).await.unwrap();
    assert_eq!(
        a.tasks_in_project(&fixture.project).await.unwrap().len(),
        3,
        "the template is a different board and does not feel the edit"
    );
}

#[tokio::test]
async fn the_mark_is_removable_and_dies_with_its_board() {
    let store = common::test_store().await;
    let (a, tenant) = tenant_with_user(&store, "mark").await;
    let project = a.create_task_project("Shape", None).await.unwrap();
    a.mark_template(&project).await.unwrap();

    a.unmark_template(&project).await.unwrap();
    assert!(a.templates().await.unwrap().is_empty());
    assert_not_found(a.unmark_template(&project).await);
    assert_not_found(
        a.instantiate_template(
            &project,
            &TemplateInstance {
                name: "No longer reusable".to_owned(),
                starts_on: None,
                customer_id: None,
            },
        )
        .await,
    );
    // Unmarking never touched the board itself.
    assert!(
        a.task_projects()
            .await
            .unwrap()
            .iter()
            .any(|p| p.id == project)
    );

    // And the tenant cascade takes the mark with everything else.
    a.mark_template(&project).await.unwrap();
    store.delete_tenant(&tenant).await.unwrap();
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project_templates WHERE tenant_id = $1")
            .bind(tenant.as_str())
            .fetch_one(&pool().await)
            .await
            .unwrap();
    assert_eq!(rows, 0, "nothing of a deleted tenant survives (law 1)");
}
