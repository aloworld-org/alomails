//! Tenancy and lifecycle proofs for onboarding and offboarding checklists (alo
//! HR, B6.05 — Law 1: isolation is tested, not assumed).
//!
//! A checklist is the one HR surface that *writes into another module*: running
//! a template creates a real task board with real tasks assigned to real people.
//! That crossing is where a tenant boundary would be lost if it were going to
//! be, so five things are proven here:
//!
//! - **wrong tenant** — tenant A's template cannot be read, listed, edited,
//!   deleted or run from tenant B, and B cannot run their own template for A's
//!   employee or assign a step to A's user; every denial is the clean
//!   `NotFound`, and nothing B attempts leaves a board behind in either tenant;
//! - **the lifecycle** — create with steps, read every field back in order,
//!   edit (which replaces the steps as a block), the name rules, and delete;
//! - **the run** — a template becomes a board of dated, assigned, source-linked
//!   tasks, with each owner role resolved to the person the rules name;
//! - **an instance is a copy** — editing or deleting the template afterwards
//!   changes nothing on a checklist somebody is working through;
//! - **progress is folded from the tasks**, so ticking a card is what moves it
//!   and there is no second number to drift.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::common;

use alo_store::hr_checklists::{TEMPLATE_STEPS_MAX, resolve_owner};
use alo_store::{
    AccountStore, ChecklistKind, ChecklistOwners, ChecklistTemplate, HrChecklistTemplateId,
    HrEmployeeId, NewChecklistRun, NewChecklistStep, NewChecklistTemplate, NewEmployee, StepOwner,
    Store, StoreError, TenantStore, UserId,
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

fn conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Conflict(message)) => message,
        other => panic!("expected Conflict, got {other:?}"),
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, StoreError>) -> String {
    match result {
        Err(StoreError::Validation(message)) => message,
        other => panic!("expected Validation, got {other:?}"),
    }
}

fn day(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("a real date")
}

fn step(title: &str, owner: StepOwner, day_offset: i32) -> NewChecklistStep {
    NewChecklistStep {
        title: title.to_owned(),
        owner,
        day_offset,
        ..Default::default()
    }
}

/// The shape a company runs when somebody arrives: one step per role, one of
/// them before the first day, so the fixture exercises every resolution rule
/// and a negative offset at once.
fn arrival() -> NewChecklistTemplate {
    NewChecklistTemplate {
        name: "Nieuwe collega".to_owned(),
        kind: ChecklistKind::Onboarding,
        steps: vec![
            NewChecklistStep {
                detail: "Standard developer machine, delivered to the office.".to_owned(),
                ..step("Order the laptop", StepOwner::It, -5)
            },
            step("Countersign the contract", StepOwner::Hr, -1),
            step("First-day walkthrough", StepOwner::Manager, 0),
            step("Read the handbook", StepOwner::Employee, 2),
        ],
    }
}

/// A tenant with an HR user who acts, a manager and a newcomer — each an
/// employee record *and* a login, so a resolved assignee can be checked against
/// a real user id rather than a placeholder.
struct Company {
    ts: TenantStore,
    hr: AccountStore,
    hr_user: UserId,
    manager_user: UserId,
    newcomer_user: UserId,
    newcomer: HrEmployeeId,
}

async fn company(store: &Store, tag: &str) -> Company {
    let tenant = store
        .create_tenant(&format!("hr-checklists-{tag}"))
        .await
        .unwrap();
    let ts = store.for_tenant(tenant.clone());
    let hr_user = ts
        .create_user(&format!("{tag}-hr@people.test"))
        .await
        .unwrap();
    let manager_user = ts
        .create_user(&format!("{tag}-boss@people.test"))
        .await
        .unwrap();
    let newcomer_user = ts
        .create_user(&format!("{tag}-new@people.test"))
        .await
        .unwrap();
    let manager = ts
        .create_hr_employee(
            &NewEmployee {
                user_id: Some(manager_user.clone()),
                given_name: "Margot".to_owned(),
                family_name: "Peeters".to_owned(),
                ..Default::default()
            },
            &hr_user,
        )
        .await
        .unwrap();
    let newcomer = ts
        .create_hr_employee(
            &NewEmployee {
                user_id: Some(newcomer_user.clone()),
                given_name: "Adelheid".to_owned(),
                preferred_name: "Ada".to_owned(),
                family_name: "Byron".to_owned(),
                manager_id: Some(manager.clone()),
                ..Default::default()
            },
            &hr_user,
        )
        .await
        .unwrap();
    Company {
        ts,
        hr: store.for_account(tenant, hr_user.clone()),
        hr_user,
        manager_user,
        newcomer_user,
        newcomer,
    }
}

/// The first day the fixture's runs are anchored to.
fn start_day() -> Date {
    day(2026, Month::September, 1)
}

fn run_of(template: &HrChecklistTemplateId) -> NewChecklistRun {
    NewChecklistRun {
        template_id: template.clone(),
        anchor_on: start_day(),
        name: String::new(),
        owners: ChecklistOwners::default(),
    }
}

/// **Wrong tenant.** Tenant A's template is unreachable from tenant B by every
/// verb the module has, and no attempt of B's leaves anything behind.
#[tokio::test]
async fn a_template_is_unreachable_and_unrunnable_from_another_tenant() {
    let store = common::test_store().await;
    let a = company(&store, "iso-a").await;
    let b = company(&store, "iso-b").await;
    let template =
        a.ts.create_hr_checklist_template(&arrival(), &a.hr_user)
            .await
            .unwrap();

    // Read, list, edit, delete.
    assert!(
        b.ts.hr_checklist_template(&template)
            .await
            .unwrap()
            .is_none()
    );
    assert!(b.ts.hr_checklist_templates().await.unwrap().is_empty());
    assert_not_found(
        b.ts.update_hr_checklist_template(&template, &arrival())
            .await,
    );
    assert_not_found(b.ts.delete_hr_checklist_template(&template).await);

    // Run it — for B's own employee, which is the shape a stolen id would take.
    assert_not_found(
        b.hr.instantiate_hr_checklist(&b.newcomer, &run_of(&template))
            .await,
    );
    // And A's template run by A for B's employee, and B's template for A's.
    let b_template =
        b.ts.create_hr_checklist_template(&arrival(), &b.hr_user)
            .await
            .unwrap();
    assert_not_found(
        a.hr.instantiate_hr_checklist(&b.newcomer, &run_of(&template))
            .await,
    );
    assert_not_found(
        b.hr.instantiate_hr_checklist(&a.newcomer, &run_of(&b_template))
            .await,
    );

    // A step assigned to the other tenant's user is refused too: a task nobody
    // can see is a task nobody will ever do.
    let across = NewChecklistRun {
        owners: ChecklistOwners {
            it: Some(a.hr_user.clone()),
            ..Default::default()
        },
        ..run_of(&b_template)
    };
    assert_not_found(b.hr.instantiate_hr_checklist(&b.newcomer, &across).await);

    // Nothing B attempted left a board in either tenant, and A's template is
    // exactly as it was.
    assert!(
        b.hr.task_projects().await.unwrap().iter().all(|project| {
            project.kind == "personal" || !project.name.contains("Nieuwe collega")
        })
    );
    assert!(
        a.hr.hr_employee_checklists(&a.newcomer)
            .await
            .unwrap()
            .is_empty()
    );
    let stored =
        a.ts.hr_checklist_template(&template)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(stored.steps.len(), 4);
    assert_eq!(stored.name, "Nieuwe collega");
}

/// The lifecycle: every field round-trips, the edit replaces the steps as a
/// block, the name rules bind within a kind, and delete means delete.
#[tokio::test]
async fn a_template_round_trips_and_its_edit_replaces_the_steps() {
    let store = common::test_store().await;
    let c = company(&store, "life").await;
    let id =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();

    let stored: ChecklistTemplate = c.ts.hr_checklist_template(&id).await.unwrap().unwrap();
    assert_eq!(stored.name, "Nieuwe collega");
    assert_eq!(stored.kind, ChecklistKind::Onboarding);
    assert_eq!(stored.created_by, c.hr_user.as_str());
    let titles: Vec<&str> = stored.steps.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Order the laptop",
            "Countersign the contract",
            "First-day walkthrough",
            "Read the handbook"
        ],
        "steps read back in the order they were written"
    );
    assert_eq!(stored.steps[0].owner, StepOwner::It);
    assert_eq!(stored.steps[0].day_offset, -5);
    assert_eq!(
        stored.steps[0].detail,
        "Standard developer machine, delivered to the office."
    );
    assert_eq!(stored.steps[3].owner, StepOwner::Employee);
    assert_eq!(stored.steps[3].day_offset, 2);

    // Two live templates of one kind may not share a name; the other kind may.
    assert!(
        conflict(
            c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
                .await
        )
        .contains("name")
    );
    let leaver = NewChecklistTemplate {
        kind: ChecklistKind::Offboarding,
        steps: vec![step("Collect the laptop", StepOwner::It, 1)],
        ..arrival()
    };
    let leaver_id =
        c.ts.create_hr_checklist_template(&leaver, &c.hr_user)
            .await
            .unwrap();

    // The edit replaces the steps wholesale — count, order and content.
    let edited = NewChecklistTemplate {
        name: "Nieuwe collega (2026)".to_owned(),
        kind: ChecklistKind::Onboarding,
        steps: vec![
            step("Order the laptop", StepOwner::It, -10),
            step("Welcome lunch", StepOwner::Manager, 0),
        ],
    };
    c.ts.update_hr_checklist_template(&id, &edited)
        .await
        .unwrap();
    let after = c.ts.hr_checklist_template(&id).await.unwrap().unwrap();
    assert_eq!(after.name, "Nieuwe collega (2026)");
    assert_eq!(
        after.kind,
        ChecklistKind::Onboarding,
        "the kind is not editable"
    );
    assert_eq!(after.steps.len(), 2);
    assert_eq!(after.steps[0].day_offset, -10);
    assert_eq!(after.steps[1].title, "Welcome lunch");
    assert!(
        after.steps[0].id != stored.steps[0].id,
        "a replaced step is a new row, not an edited one"
    );

    // The list carries both templates, each with its own steps.
    let listed = c.ts.hr_checklist_templates().await.unwrap();
    assert_eq!(listed.len(), 2);
    let onboarding = listed
        .iter()
        .find(|t| t.kind == ChecklistKind::Onboarding)
        .unwrap();
    assert_eq!(onboarding.steps.len(), 2);
    let offboarding = listed
        .iter()
        .find(|t| t.kind == ChecklistKind::Offboarding)
        .unwrap();
    assert_eq!(offboarding.steps.len(), 1);
    assert_eq!(offboarding.steps[0].title, "Collect the laptop");

    // A template with no steps at all is refused: an empty board looks done.
    let empty = NewChecklistTemplate {
        steps: Vec::new(),
        ..edited.clone()
    };
    assert!(invalid(c.ts.update_hr_checklist_template(&id, &empty).await).contains("at least one"));
    let too_many = NewChecklistTemplate {
        steps: (0..=TEMPLATE_STEPS_MAX)
            .map(|n| step(&format!("Step {n}"), StepOwner::Hr, 0))
            .collect(),
        ..edited
    };
    assert!(invalid(c.ts.update_hr_checklist_template(&id, &too_many).await).contains("at most"));

    // Delete takes the steps with it, and deleting twice is a clean denial.
    c.ts.delete_hr_checklist_template(&leaver_id).await.unwrap();
    assert!(
        c.ts.hr_checklist_template(&leaver_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_not_found(c.ts.delete_hr_checklist_template(&leaver_id).await);
    assert_eq!(c.ts.hr_checklist_templates().await.unwrap().len(), 1);
}

/// The run: a board of dated, assigned, source-linked tasks, with each role
/// resolved to the person the rules name.
#[tokio::test]
async fn a_run_lands_a_board_of_dated_assigned_tasks() {
    let store = common::test_store().await;
    let c = company(&store, "run").await;
    let template =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();

    let run =
        c.hr.instantiate_hr_checklist(&c.newcomer, &run_of(&template))
            .await
            .unwrap();
    assert_eq!(run.kind, ChecklistKind::Onboarding);
    assert_eq!(
        run.name, "Nieuwe collega — Ada Byron",
        "an unnamed run takes the template's name and the person's preferred one"
    );
    assert_eq!(run.steps.len(), 4);

    // The dates: the anchor moved by each step's offset.
    assert_eq!(run.steps[0].due_on, day(2026, Month::August, 27));
    assert_eq!(run.steps[1].due_on, day(2026, Month::August, 31));
    assert_eq!(run.steps[2].due_on, start_day());
    assert_eq!(run.steps[3].due_on, day(2026, Month::September, 3));

    // The people: IT and HR fall back to whoever drew the checklist, manager to
    // the manager link, employee to their own login.
    assert_eq!(run.steps[0].assignee, c.hr_user, "IT was not stated");
    assert_eq!(run.steps[1].assignee, c.hr_user);
    assert_eq!(run.steps[2].assignee, c.manager_user);
    assert_eq!(run.steps[3].assignee, c.newcomer_user);

    // The board itself, as the Tasks module sees it.
    let tasks = c.hr.tasks_in_project(&run.project_id).await.unwrap();
    assert_eq!(tasks.len(), 4);
    let first = tasks
        .iter()
        .find(|t| t.title == "Order the laptop")
        .unwrap();
    assert_eq!(first.status, "todo");
    assert_eq!(first.state, "active");
    assert_eq!(first.assignee.as_deref(), Some(c.hr_user.as_str()));
    assert_eq!(
        first.description.as_deref(),
        Some("Standard developer machine, delivered to the office.")
    );
    assert_eq!(first.due_at.map(|at| at.date()), Some(run.steps[0].due_on));
    assert_eq!(first.source_kind.as_deref(), Some("hr_employee"));
    assert_eq!(first.source_id.as_deref(), Some(c.newcomer.as_str()));

    // The source link is what finds a person's checklists — no link table.
    let linked =
        c.hr.tasks_for_source("hr_employee", c.newcomer.as_str())
            .await
            .unwrap();
    assert_eq!(linked.len(), 4);

    // A stated owner wins over every fallback, and a named board keeps its name.
    let stated = NewChecklistRun {
        name: "Ada — week one".to_owned(),
        owners: ChecklistOwners {
            it: Some(c.manager_user.clone()),
            employee: Some(c.hr_user.clone()),
            ..Default::default()
        },
        ..run_of(&template)
    };
    let second =
        c.hr.instantiate_hr_checklist(&c.newcomer, &stated)
            .await
            .unwrap();
    assert_eq!(second.name, "Ada — week one");
    assert_eq!(second.steps[0].assignee, c.manager_user);
    assert_eq!(second.steps[3].assignee, c.hr_user);
    assert_ne!(
        second.project_id, run.project_id,
        "a rehire, or a moved start date, is a second run rather than a refusal"
    );
}

/// An instance is a copy: editing or deleting the template afterwards changes
/// nothing somebody is working through.
#[tokio::test]
async fn editing_the_template_leaves_a_running_checklist_alone() {
    let store = common::test_store().await;
    let c = company(&store, "copy").await;
    let template =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();
    let run =
        c.hr.instantiate_hr_checklist(&c.newcomer, &run_of(&template))
            .await
            .unwrap();

    c.ts.update_hr_checklist_template(
        &template,
        &NewChecklistTemplate {
            name: "Nieuwe collega".to_owned(),
            kind: ChecklistKind::Onboarding,
            steps: vec![step("Only this one now", StepOwner::Hr, 0)],
        },
    )
    .await
    .unwrap();
    c.ts.delete_hr_checklist_template(&template).await.unwrap();

    let tasks = c.hr.tasks_in_project(&run.project_id).await.unwrap();
    assert_eq!(tasks.len(), 4);
    assert!(tasks.iter().any(|t| t.title == "Order the laptop"));
    let progress = c.hr.hr_employee_checklists(&c.newcomer).await.unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].total, 4);
}

/// Progress is folded from the tasks themselves, so ticking a card is what moves
/// it and there is no second number to drift.
#[tokio::test]
async fn progress_is_folded_from_the_tasks_themselves() {
    let store = common::test_store().await;
    let c = company(&store, "progress").await;
    let template =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();
    let run =
        c.hr.instantiate_hr_checklist(&c.newcomer, &run_of(&template))
            .await
            .unwrap();

    let before = c.hr.hr_employee_checklists(&c.newcomer).await.unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].project_id, run.project_id);
    assert_eq!(before[0].name, run.name);
    assert_eq!(before[0].total, 4);
    assert_eq!(before[0].done, 0);
    assert_eq!(before[0].first_due_on, Some(day(2026, Month::August, 27)));
    assert_eq!(before[0].last_due_on, Some(day(2026, Month::September, 3)));
    assert!(!before[0].is_complete());

    for (index, planned) in run.steps.iter().enumerate() {
        c.hr.move_task(&planned.task_id, "done", index as f64 + 1.0)
            .await
            .unwrap();
        let now = c.hr.hr_employee_checklists(&c.newcomer).await.unwrap();
        assert_eq!(now[0].done, index as i64 + 1);
    }
    let after = c.hr.hr_employee_checklists(&c.newcomer).await.unwrap();
    assert!(after[0].is_complete());

    // Another tenant's fold of the same employee id is empty, not somebody
    // else's checklist.
    let other = company(&store, "progress-b").await;
    assert!(
        other
            .hr
            .hr_employee_checklists(&c.newcomer)
            .await
            .unwrap()
            .is_empty()
    );
}

/// The refusals that must not leave half a board behind.
#[tokio::test]
async fn an_archived_person_and_an_off_calendar_anchor_are_refused_cleanly() {
    let store = common::test_store().await;
    let c = company(&store, "refuse").await;
    let template =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();
    let boards_before = c.hr.task_projects().await.unwrap().len();

    // An anchor so late that a step falls off the calendar.
    let off_calendar = NewChecklistRun {
        anchor_on: Date::MAX,
        ..run_of(&template)
    };
    assert!(
        invalid(
            c.hr.instantiate_hr_checklist(&c.newcomer, &off_calendar)
                .await
        )
        .contains("calendar")
    );

    // An unknown person, and one whose record has been archived.
    assert_not_found(
        c.hr.instantiate_hr_checklist(
            &HrEmployeeId::new("no-such-employee".to_owned()),
            &run_of(&template),
        )
        .await,
    );
    c.ts.set_hr_employee_archived(&c.newcomer, true)
        .await
        .unwrap();
    assert!(
        invalid(
            c.hr.instantiate_hr_checklist(&c.newcomer, &run_of(&template))
                .await
        )
        .contains("archived")
    );

    assert_eq!(
        c.hr.task_projects().await.unwrap().len(),
        boards_before,
        "a refused run leaves no board behind"
    );

    // Restored, it runs — offboarding is worked through *before* the record is
    // archived, which is the rule the refusal above states.
    c.ts.set_hr_employee_archived(&c.newcomer, false)
        .await
        .unwrap();
    c.hr.instantiate_hr_checklist(&c.newcomer, &run_of(&template))
        .await
        .unwrap();
}

/// The resolution rules, stated once against real ids so the pure function and
/// the stored links cannot disagree about what "the manager" means.
#[tokio::test]
async fn the_resolution_rules_read_the_links_they_claim_to() {
    let store = common::test_store().await;
    let c = company(&store, "resolve").await;
    let stated = ChecklistOwners {
        hr: Some(c.manager_user.clone()),
        ..Default::default()
    };
    assert_eq!(
        resolve_owner(
            StepOwner::Hr,
            &stated,
            Some(&c.newcomer_user),
            Some(&c.manager_user),
            &c.hr_user
        ),
        c.manager_user
    );
    assert_eq!(
        resolve_owner(StepOwner::It, &stated, None, None, &c.hr_user),
        c.hr_user
    );

    // A person with no manager link and no login of their own: every step lands
    // on the person drawing the checklist, which is the only desk that exists.
    let alone =
        c.ts.create_hr_employee(
            &NewEmployee {
                given_name: "Joris".to_owned(),
                family_name: "Claes".to_owned(),
                ..Default::default()
            },
            &c.hr_user,
        )
        .await
        .unwrap();
    let template =
        c.ts.create_hr_checklist_template(&arrival(), &c.hr_user)
            .await
            .unwrap();
    let run =
        c.hr.instantiate_hr_checklist(&alone, &run_of(&template))
            .await
            .unwrap();
    assert!(run.steps.iter().all(|s| s.assignee == c.hr_user));
    assert_eq!(run.name, "Nieuwe collega — Joris Claes");
}
