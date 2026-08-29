//! The executors of alo Projects' verbs (ADR 0058) — what runs when the
//! Projects agent uses one of the intents `alo_ai::projects_intents`
//! describes.
//!
//! Every executor runs through the asker's account door
//! ([`crate::state::Account::acc`], the tenant- and user-scoped store), so the
//! boards are the ones the asker can open — their own and the team's, a
//! colleague's private board excluded by the same visibility rule the screens
//! obey — and a timesheet read is the asker's own hours and nobody else's.
//! The figures are the store's own, through the same functions the
//! `/projects/*` routes read — the portfolio through
//! [`crate::projects_clients::project_json`], the week through
//! [`crate::projects_time::entry_json`] and [`alo_store::time_entries::week_totals`]
//! — with money made readable beside its integers
//! ([`crate::billing_intents::ok`], the shared rendering). A write only ever
//! runs from the asker's approval ([`crate::agent::execute_tool`] holds that).
//!
//! The kept executors stay in their own files and are reached only from the
//! dispatch below: [`crate::agent_projects`] (the proposed hour and the
//! status summary), [`crate::agent_timesheet`] (the calendar draft).

use std::collections::HashMap;

use serde_json::{Value, json};
use time::{Date, Duration, OffsetDateTime};

use alo_store::time_entries::week_totals;
use alo_store::{ProjectHours, ProjectId, Task, UserId};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::error::Problem;
use crate::state::{Account, AppState};

/// How many records a list read returns — enough for a question, small enough
/// to sit inside the turn's result window.
const MAX_LISTED: usize = 12;

/// The statuses a project can be in, as `/projects` validates them.
const STATUSES: &[&str] = &["planned", "active", "on_hold", "completed", "cancelled"];

/// The status filter the asker named, or `None` for the default — everything
/// unfinished. A word that is not a status is a refusal that lists them.
fn wanted_status(args: &Value) -> Result<Option<String>, Problem> {
    match string_arg(args, "status")
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
    {
        None => Ok(None),
        Some(word) if STATUSES.contains(&word.as_str()) => Ok(Some(word)),
        Some(word) => Err(unprocessable(format!(
            "no project status is called \"{word}\" — say one of {}",
            STATUSES.join(", ")
        ))),
    }
}

/// Whether a project is in scope: the named status exactly, or — by default —
/// still unfinished, which is what "active" means when somebody asks what is
/// running.
fn in_scope(wanted: Option<&str>, status: &str) -> bool {
    match wanted {
        Some(word) => status == word,
        None => !matches!(status, "completed" | "cancelled"),
    }
}

/// `active_projects` — the portfolio as it stands, exactly as `GET /projects`
/// serves it: each board with its client facts, its hours to date and its open
/// work, filtered to what is unfinished unless a status is named.
pub async fn execute_active_projects(account: &Account, args: &Value) -> Reply {
    let wanted = wanted_status(args)?;
    let projects = account.acc.task_projects().await.map_err(map_store_err)?;
    let clients = account.acc.project_clients().await.map_err(map_store_err)?;
    let hours = account.acc.project_hours().await.map_err(map_store_err)?;
    let work = account
        .acc
        .project_work_summaries()
        .await
        .map_err(map_store_err)?;
    let in_scope: Vec<_> = projects
        .iter()
        .filter(|project| in_scope(wanted.as_deref(), &project.status))
        .collect();
    let listed: Vec<Value> = in_scope
        .iter()
        .take(MAX_LISTED)
        .map(|project| {
            let client = clients.iter().find(|c| c.project_id == project.id);
            // A board nobody has logged an hour against has no aggregate row;
            // the honest zero is the answer, not a gap in the list.
            let worked = hours
                .iter()
                .find(|h| h.project_id == project.id)
                .cloned()
                .unwrap_or_else(|| ProjectHours::none_yet(project.id.clone()));
            let summary = work.iter().find(|w| w.project_id == project.id);
            crate::projects_clients::project_json(project, client, &worked, summary)
        })
        .collect();
    ok(json!({
        "kind": "projects",
        "status": wanted,
        "projectCount": in_scope.len(),
        "shown": listed.len(),
        "projects": listed,
    }))
}

/// One person's share of the boards' open work, for [`execute_who_is_on_what`].
fn person_json(who: Option<&str>, tasks: &[(&Task, &str)], now: OffsetDateTime) -> Value {
    let overdue = tasks
        .iter()
        .filter(|(task, _)| task.due_at.is_some_and(|due| due < now))
        .count();
    // Which boards the work sits on, in the order the boards were read.
    let mut boards: Vec<(&str, usize)> = Vec::new();
    for (_, board) in tasks {
        match boards.iter_mut().find(|(name, _)| name == board) {
            Some((_, count)) => *count += 1,
            None => boards.push((board, 1)),
        }
    }
    json!({
        // Nobody's name is invented: work nobody is assigned to says so.
        "who": who,
        "openTasks": tasks.len(),
        "overdueTasks": overdue,
        "projects": boards
            .iter()
            .map(|(name, count)| json!({ "project": name, "openTasks": count }))
            .collect::<Vec<_>>(),
    })
}

/// The people behind a list of open tasks, busiest first, work nobody is
/// assigned to last — counts only, no task titles and no hours: "who is on
/// what" is a question about allocation, not about anybody's timesheet.
fn people_json(
    tasks: &[(Task, String)],
    labels: &HashMap<String, String>,
    now: OffsetDateTime,
) -> Vec<Value> {
    /// One person and their open tasks, each task with the board it sits on.
    type Share<'a> = (Option<&'a str>, Vec<(&'a Task, &'a str)>);
    let mut order: Vec<Option<String>> = Vec::new();
    for (task, _) in tasks {
        if !order.contains(&task.assignee) {
            order.push(task.assignee.clone());
        }
    }
    let mut people: Vec<Share<'_>> = order
        .iter()
        .map(|assignee| {
            let theirs: Vec<(&Task, &str)> = tasks
                .iter()
                .filter(|(task, _)| task.assignee == *assignee)
                .map(|(task, board)| (task, board.as_str()))
                .collect();
            (assignee.as_deref(), theirs)
        })
        .collect();
    // Busiest first, ties by name so the order is stable; unassigned work is
    // real but goes last — it is nobody's answer to "who".
    people.sort_by(|(a, theirs_a), (b, theirs_b)| {
        a.is_none()
            .cmp(&b.is_none())
            .then(theirs_b.len().cmp(&theirs_a.len()))
            .then(a.cmp(b))
    });
    people
        .iter()
        .map(|(assignee, theirs)| {
            let label =
                assignee.map(|user| labels.get(user).cloned().unwrap_or_else(|| user.to_owned()));
            person_json(label.as_deref(), theirs, now)
        })
        .collect()
}

/// `who_is_on_what` — who is carrying what across the boards the asker can
/// open: open tasks per colleague, overdue counted, boards named. Counts, not
/// titles; the boards are the asker's own visibility, so a colleague's private
/// board contributes nothing.
pub async fn execute_who_is_on_what(account: &Account, args: &Value, state: &AppState) -> Reply {
    let boards = match string_arg(args, "project").filter(|name| !name.trim().is_empty()) {
        Some(_) => vec![crate::agent_projects::resolve_project(account, args).await?],
        None => account.acc.task_projects().await.map_err(map_store_err)?,
    };
    let boards_read = boards.len();
    let mut tasks: Vec<(Task, String)> = Vec::new();
    for board in &boards {
        let on_board = account
            .acc
            .tasks_in_project(&ProjectId::new(board.id.as_str()))
            .await
            .map_err(map_store_err)?;
        tasks.extend(
            on_board
                .into_iter()
                .filter(|task| task.completed_at.is_none())
                .map(|task| (task, board.name.clone())),
        );
    }
    // The people who can be named are the assignees already on those boards —
    // never a directory, and each is shown by their address rather than by an
    // opaque id.
    let owners: Vec<UserId> = {
        let mut out: Vec<UserId> = Vec::new();
        for (task, _) in &tasks {
            if let Some(assignee) = &task.assignee {
                let id = UserId::new(assignee.clone());
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    };
    let labels = state
        .store
        .for_tenant(account.tenant.clone())
        .emails_of(&owners)
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "whoIsOnWhat",
        "boardsRead": boards_read,
        "openTasks": tasks.len(),
        "people": people_json(&tasks, &labels, OffsetDateTime::now_utc()),
    }))
}

/// The period `time_this_week` reads: the stated days, or the week `today`
/// sits in — Monday to Sunday, the same week the timesheet screen shows. A
/// stated `from` without a `to` runs to the Sunday of ITS week, so "last week"
/// needs one day, not two.
fn period(args: &Value, today: Date) -> Result<(Date, Date), Problem> {
    let from = match string_arg(args, "from").filter(|raw| !raw.trim().is_empty()) {
        None => today - Duration::days(i64::from(today.weekday().number_days_from_monday())),
        Some(raw) => {
            parse_iso_date(&raw).ok_or_else(|| unprocessable("from must be a date, YYYY-MM-DD"))?
        }
    };
    let to = match string_arg(args, "to").filter(|raw| !raw.trim().is_empty()) {
        None => from + Duration::days(6 - i64::from(from.weekday().number_days_from_monday())),
        Some(raw) => {
            parse_iso_date(&raw).ok_or_else(|| unprocessable("to must be a date, YYYY-MM-DD"))?
        }
    };
    if from > to {
        return Err(unprocessable("from is after to"));
    }
    if to - from > Duration::days(366) {
        return Err(unprocessable("a period of at most a year"));
    }
    Ok((from, to))
}

/// `time_this_week` — the asker's OWN hours over a period, exactly as
/// `GET /projects/time?from&to` serves them: each entry with its day, project
/// and minutes, and the period's totals with suggestions counted apart.
pub async fn execute_time_this_week(account: &Account, args: &Value) -> Reply {
    let (from, to) = period(args, today())?;
    let board = match string_arg(args, "project").filter(|name| !name.trim().is_empty()) {
        Some(_) => Some(crate::agent_projects::resolve_project(account, args).await?),
        None => None,
    };
    let project = board
        .as_ref()
        .map(|project| ProjectId::new(project.id.as_str()));
    let entries = account
        .acc
        .time_entries(from, to, project.as_ref())
        .await
        .map_err(map_store_err)?;
    let task_ids: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.task_id.as_ref().map(|id| id.as_str().to_owned()))
        .collect();
    let titles = account
        .acc
        .task_titles(&task_ids)
        .await
        .map_err(map_store_err)?;
    let listed: Vec<Value> = entries
        .iter()
        .take(MAX_LISTED)
        .map(|entry| {
            let title = entry
                .task_id
                .as_ref()
                .and_then(|id| titles.get(id.as_str()));
            crate::projects_time::entry_json(entry, title.map(String::as_str))
        })
        .collect();
    ok(json!({
        "kind": "myTime",
        "from": iso_date(from),
        "to": iso_date(to),
        "project": board.as_ref().map(|project| project.name.clone()),
        "entryCount": entries.len(),
        "shown": listed.len(),
        "entries": listed,
        // The whole period's fold, however many entries were listed above.
        "totals": crate::projects_time::totals_json(week_totals(&entries)),
    }))
}

/// The module's verbs by name (A4.1c) — Projects' one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The kept executors —
/// [`crate::agent_projects`] for the proposed hour and the status summary,
/// [`crate::agent_timesheet`] for the calendar draft — are reached from here
/// so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "active_projects" => Box::pin(execute_active_projects(account, args)),
        "project_status_summary" => Box::pin(
            crate::agent_projects::execute_project_status_summary(account, args),
        ),
        "who_is_on_what" => Box::pin(execute_who_is_on_what(account, args, state)),
        "time_this_week" => Box::pin(execute_time_this_week(account, args)),
        "log_time" => Box::pin(crate::agent_projects::execute_log_time(account, args)),
        "draft_timesheet_from_calendar" => {
            Box::pin(crate::agent_timesheet::execute_draft_timesheet_from_calendar(account, args))
        }
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alo_ai::projects_intents::PROJECTS;
    use alo_store::TaskId;

    /// Every `/projects` route the router registers is the adapter of a verb
    /// or excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_projects_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = PROJECTS.uncovered(router, "/projects");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route the
        // app does not have.
        let routes = alo_ai::routes_in(router, "/projects");
        for intent in PROJECTS.intents {
            for route in intent.routes {
                assert!(
                    routes.contains(&(*route).to_owned()),
                    "{}: {route} is not a route",
                    intent.name
                );
            }
        }
    }

    #[test]
    fn every_verb_the_registry_offers_is_dispatched() {
        let dispatch = include_str!("projects_intents.rs");
        for intent in PROJECTS.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// Projects' registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("projects_intents::").count(),
            1,
            "agent.rs names Projects only in MODULES"
        );
        assert!(agent.contains("crate::projects_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    #[test]
    fn the_default_scope_is_everything_unfinished_and_a_named_status_is_exact() {
        for still_going in ["planned", "active", "on_hold"] {
            assert!(in_scope(None, still_going), "{still_going}");
        }
        for finished in ["completed", "cancelled"] {
            assert!(!in_scope(None, finished), "{finished}");
        }
        assert!(in_scope(Some("completed"), "completed"));
        assert!(!in_scope(Some("completed"), "active"));
        // The filter word is validated, and a stranger is a refusal that
        // lists the real ones rather than an empty list.
        assert_eq!(wanted_status(&json!({})).unwrap(), None);
        assert_eq!(
            wanted_status(&json!({ "status": " On_Hold " })).unwrap(),
            Some("on_hold".to_owned())
        );
        let refusal = wanted_status(&json!({ "status": "finished" })).expect_err("no such status");
        let detail = refusal.detail.unwrap_or_default();
        assert!(detail.contains("no project status is called \"finished\""));
        assert!(detail.contains("on_hold"), "{detail}");
    }

    fn day(iso: &str) -> Date {
        parse_iso_date(iso).expect("a plain day")
    }

    #[test]
    fn the_default_period_is_the_week_today_sits_in() {
        // 2026-08-27 is a Thursday: the window runs Monday the 24th to Sunday
        // the 30th, which is the week the timesheet screen shows.
        let (from, to) = period(&json!({}), day("2026-08-27")).unwrap();
        assert_eq!(from, day("2026-08-24"));
        assert_eq!(to, day("2026-08-30"));
        // A Monday is its own week's start, not the previous one's.
        let (from, to) = period(&json!({}), day("2026-08-24")).unwrap();
        assert_eq!(from, day("2026-08-24"));
        assert_eq!(to, day("2026-08-30"));
        // A stated from runs to the Sunday of ITS week — "last week" is one
        // day, not two.
        let (from, to) = period(&json!({ "from": "2026-08-19" }), day("2026-08-27")).unwrap();
        assert_eq!(from, day("2026-08-19"));
        assert_eq!(to, day("2026-08-23"));
        // Both stated: taken as said.
        let (from, to) = period(
            &json!({ "from": "2026-08-01", "to": "2026-08-31" }),
            day("2026-08-27"),
        )
        .unwrap();
        assert_eq!(from, day("2026-08-01"));
        assert_eq!(to, day("2026-08-31"));
        // A backwards or unbounded period is a refusal, not a guess.
        assert!(
            period(
                &json!({ "from": "2026-08-31", "to": "2026-08-01" }),
                day("2026-08-27")
            )
            .is_err()
        );
        assert!(
            period(
                &json!({ "from": "2020-01-01", "to": "2026-08-01" }),
                day("2026-08-27")
            )
            .is_err()
        );
        assert!(period(&json!({ "from": "yesterday" }), day("2026-08-27")).is_err());
    }

    fn stamp(iso: &str) -> OffsetDateTime {
        OffsetDateTime::parse(iso, &time::format_description::well_known::Rfc3339)
            .expect("an RFC 3339 instant")
    }

    fn task(title: &str, assignee: Option<&str>, due: Option<&str>) -> Task {
        Task {
            id: TaskId::new(title),
            project_id: ProjectId::new("p1"),
            title: title.to_owned(),
            description: None,
            status: "todo".to_owned(),
            position: 0.0,
            assignee: assignee.map(ToOwned::to_owned),
            due_at: due.map(stamp),
            priority: "none".to_owned(),
            state: "active".to_owned(),
            source_kind: None,
            source_id: None,
            created_by: "u1".to_owned(),
            created_at: stamp("2026-07-01T09:00:00Z"),
            updated_at: stamp("2026-07-01T09:00:00Z"),
            completed_at: None,
            subtask_done: 0,
            subtask_total: 0,
            comment_count: 0,
        }
    }

    #[test]
    fn people_are_grouped_busiest_first_with_unassigned_work_last() {
        let now = stamp("2026-08-27T12:00:00Z");
        let tasks = vec![
            (
                task("Write the brief", Some("u-bo"), None),
                "Relaunch".to_owned(),
            ),
            (
                task("Review copy", Some("u-an"), Some("2026-08-01T09:00:00Z")),
                "Relaunch".to_owned(),
            ),
            (
                task("Ship the pilot", Some("u-an"), None),
                "Pilot".to_owned(),
            ),
            (task("File the notes", None, None), "Pilot".to_owned()),
        ];
        let labels: HashMap<String, String> = [
            ("u-an".to_owned(), "an@example.com".to_owned()),
            ("u-bo".to_owned(), "bo@example.com".to_owned()),
        ]
        .into();
        let people = people_json(&tasks, &labels, now);
        assert_eq!(people.len(), 3);
        // Busiest first, and the label is the address, never the id.
        assert_eq!(people[0]["who"], "an@example.com");
        assert_eq!(people[0]["openTasks"], 2);
        assert_eq!(people[0]["overdueTasks"], 1);
        assert_eq!(people[0]["projects"][0]["project"], "Relaunch");
        assert_eq!(people[0]["projects"][1]["project"], "Pilot");
        assert_eq!(people[1]["who"], "bo@example.com");
        assert_eq!(people[1]["overdueTasks"], 0);
        // Unassigned work is real, said as nobody's, and last.
        assert_eq!(people[2]["who"], Value::Null);
        assert_eq!(people[2]["openTasks"], 1);
        // An id the tenant cannot label falls back to the id rather than to a
        // hole.
        let unlabelled = people_json(&tasks, &HashMap::new(), now);
        assert_eq!(unlabelled[0]["who"], "u-an");
        assert!(people_json(&[], &labels, now).is_empty());
    }
}
