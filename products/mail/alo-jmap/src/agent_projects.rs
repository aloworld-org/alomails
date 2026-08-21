//! Executing the **Projects** tools of an approved agent proposal (ADR 0034,
//! ADR 0035 wave B3.10a) — the acting half of what [`alo_ai::agent_projects`]
//! describes to the model.
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. Everything here therefore
//! runs through the caller's own tenant-scoped store handle — an agent can no
//! more reach another tenant's engagement, or a colleague's private board, than
//! the browser that asked it can.
//!
//! Three rules shape this module, and they are why it is not thin glue:
//!
//! - **An hour the agent writes is a proposal, not a timesheet line.** The
//!   entry lands `proposed` ([`alo_store::NewTimeEntry::proposed`]), which puts
//!   it in no total, no submitted week and no invoice until the person whose
//!   timesheet it is accepts it on `POST /projects/time/{id}/accept`. This is
//!   ADR 0023's rule held literally: a machine's guess about somebody's Tuesday
//!   is a suggestion, and a suggestion already inside a total is not one
//!   (`docs/design/projects.md` § Proposed entries are not hours). A proposal
//!   also carries **no rate** — the price is resolved at acceptance, from the
//!   engagement as it stands when a human agrees the work happened.
//! - **The project is found by its name, among the caller's own boards.** The
//!   shared rule ([`crate::agent_args`]) resolves it — exact first, then a
//!   unique containment — and two matches is a refusal that lists them, never a
//!   guess. `task_projects` is already the visibility answer: a colleague's
//!   private board is not in the list, so it cannot be named into existence.
//! - **The summary reads and writes nothing at all.** It is the one agent tool
//!   in the suite whose whole result is figures, and every one of them is read
//!   through the same store functions the `/projects` screens use — hours from
//!   the project-grain aggregate, budget from the engagement's own facts, the
//!   plan from its milestones, the work from its active tasks. No total is
//!   computed here that a screen computes differently elsewhere.
//!
//! The summary answers **figures, not prose**: a sentence composed here would
//! be a user-facing string authored in the server in one language, which is a
//! bug in a European product (CLAUDE.md). The UI renders these numbers through
//! its own catalogue.

use axum::Json;
use serde_json::{Value, json};
use time::{Date, OffsetDateTime};

use alo_store::{
    Milestone, NewTimeEntry, ProjectClient, ProjectHours, ProjectId, Task, TaskId, TaskProject,
};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::state::Account;

/// `log_time` — write one **proposed** entry on the named project, for the
/// caller's own timesheet.
///
/// Everything is resolved and validated before anything is written, the order
/// every executor on this seam uses: a proposal naming a project that does not
/// exist leaves no half-made record behind. The duration's *range*, the note's
/// length and the week lock are the store's rules and are left to it — an entry
/// drafted into a week somebody has already submitted is refused there, with
/// the week named, exactly as a manual entry would be.
///
/// # Errors
/// `422` when the project cannot be resolved to exactly one board, when the day
/// is missing or is not a plain `YYYY-MM-DD`, when the duration is missing or
/// is not a whole number of minutes, or when the named task is not one of that
/// project's; the store's own `422`/`409` otherwise.
pub async fn execute_log_time(account: &Account, args: &Value) -> Result<Json<Value>, Problem> {
    let project = resolve_project(account, args).await?;
    let work_date = work_date(args)?;
    let minutes = minutes(args)?;
    let task = resolve_task(account, &project.id, args).await?;

    let new = NewTimeEntry {
        task_id: task.as_ref().map(|t| TaskId::new(t.id.as_str())),
        billable: args
            .get("billable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        note: string_arg(args, "note").unwrap_or_default(),
        // The whole point of the tool: a suggestion, in no total until a human
        // accepts it in their own timesheet.
        proposed: true,
        ..NewTimeEntry::worked(ProjectId::new(project.id.as_str()), work_date, minutes)
    };
    let entry = account.acc.log_time(&new).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "timeEntry",
            "id": entry.id.as_str(),
            "title": project.name,
            "projectId": entry.project_id.as_str(),
            "taskId": entry.task_id.as_ref().map(TaskId::as_str),
            "workDate": iso_date(entry.work_date),
            "minutes": entry.minutes,
            "billable": entry.billable,
            "note": entry.note,
            // Always true here, and stated rather than implied: the client that
            // renders this result tells the user their timesheet now has
            // something to accept.
            "proposed": entry.is_proposed(),
        }
    })))
}

/// `project_status_summary` — where one project stands: hours, budget, plan and
/// open work.
///
/// The hours read settles existence and visibility in one statement
/// ([`alo_store::AccountStore::project_hours_for`]), so a board this caller
/// cannot see is the same refusal as one that never existed, before any other
/// fact about it is fetched.
///
/// # Errors
/// `422` when the project cannot be resolved to exactly one board; `404` when
/// the resolved board stops being visible between the two reads; `500` on a
/// store failure.
pub async fn execute_project_status_summary(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let project = resolve_project(account, args).await?;
    let id = ProjectId::new(project.id.as_str());
    let hours = account
        .acc
        .project_hours_for(&id)
        .await
        .map_err(map_store_err)?;
    let client = account
        .acc
        .project_client(&id)
        .await
        .map_err(map_store_err)?;
    let customer = match &client {
        Some(client) => account
            .acc
            .billing_customer(&client.customer_id)
            .await
            .map_err(map_store_err)?
            .map(|customer| customer.name),
        None => None,
    };
    let milestones = account.acc.milestones(&id).await.map_err(map_store_err)?;
    let tasks = account
        .acc
        .tasks_in_project(&id)
        .await
        .map_err(map_store_err)?;
    let today = OffsetDateTime::now_utc().date();

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "projectStatus",
            "id": project.id.as_str(),
            "title": project.name,
            "hours": hours_json(&hours),
            "budget": budget_json(client.as_ref(), &hours, customer),
            "milestones": plan_json(&milestones, today),
            "tasks": work_json(&tasks, OffsetDateTime::now_utc()),
        }
    })))
}

/// The hours to date, in minutes — the project-grain aggregate, with no
/// per-person breakdown anywhere in it ([`alo_store::project_hours`] is where
/// that absence is argued).
fn hours_json(hours: &ProjectHours) -> Value {
    json!({
        "minutes": hours.minutes,
        "billableMinutes": hours.billable_minutes,
        "approvedUnbilledMinutes": hours.approved_unbilled_minutes,
        "submittedUnbilledMinutes": hours.submitted_unbilled_minutes,
        "billedMinutes": hours.billed_minutes,
        "lastWorkedOn": hours.last_worked_on.map(iso_date),
    })
}

/// The engagement's advisory budget and what is left of it, or the honest
/// nulls of a project nobody has priced.
///
/// `consumptionBp` is the store's own basis-point figure, uncapped: a project
/// past its budget reports over 10 000, because that is the one case the number
/// exists to show. Nothing is computed here — no money arithmetic reaches this
/// layer, and none of it reaches a browser.
fn budget_json(
    client: Option<&ProjectClient>,
    hours: &ProjectHours,
    customer: Option<String>,
) -> Value {
    match client {
        None => json!({ "isClientWork": false }),
        Some(client) => json!({
            "isClientWork": true,
            "customer": customer,
            "currency": client.currency,
            "rateCents": client.rate_cents,
            "budgetMinutes": client.budget_minutes,
            "budgetCents": client.budget_cents,
            "consumptionBp": hours.budget_consumption_bp(client.budget_minutes),
        }),
    }
}

/// The plan: how many milestones there are, how many a human has closed, how
/// many are late as at `today`, and the next one still ahead.
fn plan_json(milestones: &[Milestone], today: Date) -> Value {
    let done = milestones.iter().filter(|m| m.is_done()).count();
    let late = milestones.iter().filter(|m| m.is_late(today)).count();
    // The nearest date nobody has closed yet, late ones included: "what is next"
    // is what a person is asked about, and an overdue milestone is very much
    // next. `milestones` arrives ordered by day, so the first open one is it.
    let next = milestones.iter().find(|m| !m.is_done()).map(|m| {
        json!({
            "name": m.name,
            "dueOn": iso_date(m.due_on),
            "late": m.is_late(today),
        })
    });
    json!({
        "total": milestones.len(),
        "done": done,
        "late": late,
        "next": next,
    })
}

/// The work: the project's active tasks, split the way a status question means
/// it — what is still open, and how much of that is past its date.
///
/// A task is *done* when a human moved it to the done column, which is what set
/// `completed_at`; the column's own name is a board decision and not this
/// count's business.
fn work_json(tasks: &[Task], now: OffsetDateTime) -> Value {
    let open = tasks.iter().filter(|t| t.completed_at.is_none()).count();
    let overdue = tasks
        .iter()
        .filter(|t| t.completed_at.is_none() && t.due_at.is_some_and(|due| due < now))
        .count();
    json!({
        "total": tasks.len(),
        "open": open,
        "overdue": overdue,
        "done": tasks.len() - open,
    })
}

/// The project a proposal names, resolved among the boards this caller can see.
///
/// Shared with [`crate::agent_timesheet`], which resolves the same name the same
/// way: two readings of "the Hansen project" would be two ways to reach the
/// wrong engagement.
///
/// `task_projects` is the visibility answer already: a colleague's private
/// board is not in the list, so no name can reach it.
pub(crate) async fn resolve_project(
    account: &Account,
    args: &Value,
) -> Result<TaskProject, Problem> {
    let wanted = string_arg(args, "project")
        .or_else(|| string_arg(args, "projectName"))
        .ok_or_else(|| unprocessable("which project this is about is required"))?;
    let projects = account.acc.task_projects().await.map_err(map_store_err)?;
    let picked = pick(
        &wanted,
        projects.iter().map(|p| (p.name.as_str(), p)).collect(),
        "project",
    )?;
    Ok(picked.clone())
}

/// The task an entry was worked under, when the proposal names one — resolved
/// among **that project's** own active tasks, so a title that belongs to
/// another board is "no task of yours is called …" rather than the store's
/// flatter refusal about a task on another project.
async fn resolve_task(
    account: &Account,
    project: &ProjectId,
    args: &Value,
) -> Result<Option<Task>, Problem> {
    let Some(wanted) = string_arg(args, "task") else {
        return Ok(None);
    };
    let tasks = account
        .acc
        .tasks_in_project(project)
        .await
        .map_err(map_store_err)?;
    let picked = pick(
        &wanted,
        tasks.iter().map(|t| (t.title.as_str(), t)).collect(),
        "task",
    )?;
    Ok(Some(picked.clone()))
}

/// How long the work took, in whole minutes.
///
/// Read here rather than through the shared [`crate::agent_args::integer`],
/// whose refusal is written for money ("a whole number of cents") — a duration
/// told it must be cents is a refusal nobody can act on. The *range* is the
/// store's rule and stays there; what this refuses is a duration that is not a
/// whole number of minutes at all, and it refuses rather than rounds: an hour
/// rounded on the way in is an hour somebody has to argue about later.
fn minutes(args: &Value) -> Result<i64, Problem> {
    match args.get("minutes") {
        None | Some(Value::Null) => Err(unprocessable(
            "how long the work took, in whole minutes, is required",
        )),
        Some(Value::Number(stated)) => stated.as_i64().ok_or_else(|| {
            unprocessable(format!(
                "minutes must be a whole number of minutes, not {stated} — write 90 for an hour \
                 and a half"
            ))
        }),
        Some(other) => Err(unprocessable(format!(
            "minutes must be a whole number of minutes, not {other}"
        ))),
    }
}

/// The day the work belongs to, as a plain `YYYY-MM-DD` in the worker's own
/// zone — never derived from the server's clock, which is the rule the whole
/// hours surface holds (`crate::projects_time`): an entry dated by a fallback
/// nobody asked for is the one thing an employee will dispute.
fn work_date(args: &Value) -> Result<Date, Problem> {
    let stated = string_arg(args, "date")
        .or_else(|| string_arg(args, "workDate"))
        .or_else(|| string_arg(args, "day"))
        .ok_or_else(|| unprocessable("the day the work was done is required"))?;
    parse_iso_date(&stated)
        .ok_or_else(|| unprocessable("the day must be a date written YYYY-MM-DD"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::ProjectMilestoneId;

    fn day(iso: &str) -> Date {
        parse_iso_date(iso).expect("a plain day")
    }

    fn stamp(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
            .expect("an RFC 3339 instant")
    }

    fn milestone(name: &str, due: &str, done: bool) -> Milestone {
        Milestone {
            id: ProjectMilestoneId::new(name),
            project_id: ProjectId::new("p1"),
            name: name.to_owned(),
            due_on: day(due),
            done_at: done.then(|| stamp("2026-08-01T09:00:00Z")),
            position: 0,
            created_by: "u1".to_owned(),
            created_at: stamp("2026-07-01T09:00:00Z"),
            updated_at: stamp("2026-07-01T09:00:00Z"),
            task_count: 0,
            task_done_count: 0,
        }
    }

    fn task(title: &str, due: Option<&str>, done: bool) -> Task {
        Task {
            id: TaskId::new(title),
            project_id: ProjectId::new("p1"),
            title: title.to_owned(),
            description: None,
            status: if done { "done" } else { "todo" }.to_owned(),
            position: 0.0,
            assignee: None,
            due_at: due.map(stamp),
            priority: "none".to_owned(),
            state: "active".to_owned(),
            source_kind: None,
            source_id: None,
            created_by: "u1".to_owned(),
            created_at: stamp("2026-07-01T09:00:00Z"),
            updated_at: stamp("2026-07-01T09:00:00Z"),
            completed_at: done.then(|| stamp("2026-08-02T09:00:00Z")),
            subtask_done: 0,
            subtask_total: 0,
            comment_count: 0,
        }
    }

    #[test]
    fn a_day_is_stated_plainly_or_the_entry_is_refused() {
        assert_eq!(
            work_date(&json!({ "date": " 2026-08-03 " })).unwrap(),
            day("2026-08-03")
        );
        // The three spellings a model reaches for all mean the same day.
        assert_eq!(
            work_date(&json!({ "workDate": "2026-08-03" })).unwrap(),
            day("2026-08-03")
        );
        assert_eq!(
            work_date(&json!({ "day": "2026-08-03" })).unwrap(),
            day("2026-08-03")
        );
        // A missing day is a refusal, never today: an hour belongs to a stated
        // day or to no timesheet at all.
        let problem = work_date(&json!({ "minutes": 60 })).expect_err("refused");
        assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        for bad in [
            "yesterday",
            "03/08/2026",
            "2026-08-03T09:00:00Z",
            "2026-13-01",
        ] {
            let problem = work_date(&json!({ "date": bad })).expect_err("accepted a bad day");
            assert_eq!(
                problem.status,
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "{bad}"
            );
        }
    }

    #[test]
    fn a_duration_arrives_whole_or_not_at_all_and_the_refusal_speaks_of_minutes() {
        assert_eq!(minutes(&json!({ "minutes": 90 })).unwrap(), 90);
        // Absent is a refusal that says what is missing…
        let problem = minutes(&json!({ "date": "2026-08-05" })).expect_err("refused");
        assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            problem
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("whole minutes")
        );
        // …and a fraction is refused, never rounded — in the unit this tool is
        // about. The shared money reader would have said "cents" here.
        for bad in [json!(90.5), json!("90"), json!(true)] {
            let problem = minutes(&json!({ "minutes": bad })).expect_err("accepted");
            let detail = problem.detail.unwrap_or_default();
            assert!(detail.contains("whole number of minutes"), "{detail}");
            assert!(!detail.contains("cents"), "{detail}");
        }
    }

    #[test]
    fn the_plan_counts_done_late_and_what_is_next() {
        let today = day("2026-08-08");
        let plan = plan_json(
            &[
                milestone("Kickoff", "2026-07-01", true),
                milestone("Draft delivered", "2026-08-01", false),
                milestone("Launch", "2026-09-01", false),
            ],
            today,
        );
        assert_eq!(plan["total"], 3);
        assert_eq!(plan["done"], 1);
        // Late is "open and behind us" — a milestone closed after its day is not.
        assert_eq!(plan["late"], 1);
        // Next is the nearest open one, late included: an overdue milestone is
        // very much what comes next.
        assert_eq!(plan["next"]["name"], "Draft delivered");
        assert_eq!(plan["next"]["dueOn"], "2026-08-01");
        assert_eq!(plan["next"]["late"], true);

        // Everything closed: nothing is next, and it says so with null rather
        // than with a milestone from the past.
        let all_done = plan_json(&[milestone("Kickoff", "2026-07-01", true)], today);
        assert_eq!(all_done["next"], Value::Null);
        assert_eq!(all_done["late"], 0);
        // A project with no plan at all is zeroes, not an absent block.
        let none = plan_json(&[], today);
        assert_eq!(none["total"], 0);
        assert_eq!(none["next"], Value::Null);
    }

    #[test]
    fn the_work_counts_open_and_overdue_by_completion_not_by_column() {
        let now = stamp("2026-08-08T12:00:00Z");
        let tasks = [
            task("Write the brief", Some("2026-08-01T09:00:00Z"), false),
            task("Review copy", Some("2026-09-01T09:00:00Z"), false),
            task("Kick off", Some("2026-07-01T09:00:00Z"), true),
            task("No date", None, false),
        ];
        let work = work_json(&tasks, now);
        assert_eq!(work["total"], 4);
        assert_eq!(work["open"], 3);
        assert_eq!(work["done"], 1);
        // Only the open one whose day has passed; a completed task is never
        // overdue however late it was closed, and an undated one never is.
        assert_eq!(work["overdue"], 1);
        assert_eq!(work_json(&[], now)["open"], 0);
    }

    #[test]
    fn an_internal_project_reports_no_budget_rather_than_zeroes() {
        let hours = ProjectHours {
            project_id: ProjectId::new("p1"),
            minutes: 600,
            billable_minutes: 540,
            approved_unbilled_minutes: 420,
            submitted_unbilled_minutes: 0,
            billed_minutes: 120,
            last_worked_on: Some(day("2026-08-07")),
        };
        let budget = budget_json(None, &hours, None);
        assert_eq!(budget["isClientWork"], false);
        assert_eq!(budget["budgetMinutes"], Value::Null, "absent, not zero");

        let told = hours_json(&hours);
        assert_eq!(told["minutes"], 600);
        assert_eq!(told["billableMinutes"], 540);
        assert_eq!(told["approvedUnbilledMinutes"], 420);
        assert_eq!(told["submittedUnbilledMinutes"], 0);
        assert_eq!(told["billedMinutes"], 120);
        assert_eq!(told["lastWorkedOn"], "2026-08-07");
        // Nobody has worked on it: the day is null, not an invented one.
        assert_eq!(
            hours_json(&ProjectHours::none_yet(ProjectId::new("p1")))["lastWorkedOn"],
            Value::Null
        );
    }
}
