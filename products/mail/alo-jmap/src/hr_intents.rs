//! The executors of alo HR's verbs (ADR 0058, AA.5) — what runs when the
//! People agent uses one of the intents `alo_ai::hr_intents` describes.
//!
//! Every executor runs through the asker's account door
//! ([`crate::state::Account::acc`], the tenant-scoped store) and the same
//! people-doors the HR screens answer to: the member directory everybody
//! already reads, and [`crate::hr_leave_door::LeaveDoor`] — mine, my team's,
//! HR's — on everything about a person's time off. The answers are the same
//! record views the `/hr/*` routes serve: the directory's public projection
//! ([`alo_store::DirectoryEntry`], a type with no home address on it to
//! leak), the balance with its working
//! ([`crate::hr_leave_balances::balance_json`]), the request queue
//! ([`crate::hr_leave_requests::request_json`]) and the checklist fold
//! ([`crate::hr_checklists::progress_json`]).
//!
//! **One deliberate narrowing.** What the model is shown of a leave request
//! never includes its `note` or `decisionNote`: the sentence somebody wrote
//! under "why I need the time" is theirs, the screen that decides shows it to
//! the decider, and a model that had read it would eventually repeat it into
//! a chat room. [`request_row`] strips both, and the module test holds that.
//!
//! The kept executors stay in their own file and are reached only from the
//! dispatch below: [`crate::agent_hr`] (the absence view and the letter
//! fill-in).

use serde_json::{Map, Value, json};

use alo_store::hr_leave_requests::{LeaveRequestQuery, LeaveStatus};
use alo_store::{DirectoryEntry, HrEmployeeId, LeaveRequest};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::billing_intents::{Reply, ok};
use crate::error::Problem;
use crate::hr_leave_door::LeaveDoor;
use crate::state::{Account, AppState};

/// How many records a list read returns — enough for a question, small enough
/// to sit inside the turn's result window. People are more numerous than
/// purchase orders, so this is wider than the business modules' bound.
const MAX_LISTED: usize = 25;

/// One colleague as the directory shows them — the same public projection
/// `GET /hr/org` folds its chart from, with the manager's name resolved so an
/// answer about who reports to whom does not need a second read.
fn person_json(person: &DirectoryEntry, directory: &[DirectoryEntry]) -> Value {
    let manager = person.manager_id.as_ref().and_then(|manager_id| {
        directory
            .iter()
            .find(|candidate| candidate.id == *manager_id)
    });
    json!({
        "employeeId": person.id.as_str(),
        "name": person.display_name(),
        "jobTitle": person.job_title,
        "team": person.team,
        "managerId": person.manager_id.as_ref().map(HrEmployeeId::as_str),
        "managerName": manager.map(DirectoryEntry::display_name),
    })
}

/// `who_works_here` — the member directory as every colleague already reads
/// it, narrowed to one team or resolved to one person when the asker named
/// one. A team nobody is on is a refusal that names the teams that exist,
/// which keeps a model from concluding a team is empty because it misspelled
/// it.
pub async fn execute_who_works_here(account: &Account, args: &Value) -> Reply {
    let directory = account.acc.hr_directory().await.map_err(map_store_err)?;
    if let Some(wanted) = string_arg(args, "person").filter(|name| !name.trim().is_empty()) {
        let named: Vec<(String, &DirectoryEntry)> = directory
            .iter()
            .map(|person| (person.display_name(), person))
            .collect();
        let person = pick(
            &wanted,
            named.iter().map(|(name, e)| (name.as_str(), *e)).collect(),
            "colleague",
        )?;
        return ok(json!({
            "kind": "whoWorksHere",
            "person": person_json(person, &directory),
        }));
    }
    let in_scope: Vec<&DirectoryEntry> = match string_arg(args, "team")
        .map(|team| team.trim().to_lowercase())
        .filter(|team| !team.is_empty())
    {
        None => directory.iter().collect(),
        Some(needle) => {
            let matching: Vec<&DirectoryEntry> = directory
                .iter()
                .filter(|person| person.team.to_lowercase().contains(&needle))
                .collect();
            if matching.is_empty() {
                let mut teams: Vec<&str> = directory
                    .iter()
                    .map(|person| person.team.as_str())
                    .filter(|team| !team.is_empty())
                    .collect();
                teams.sort_unstable();
                teams.dedup();
                return Err(unprocessable(if teams.is_empty() {
                    "no team matches — the directory does not record teams yet".to_owned()
                } else {
                    format!("no team matches — the teams here are: {}", teams.join(", "))
                }));
            }
            matching
        }
    };
    // People per team, so "who works here" can answer with the shape of the
    // company even when the list itself is capped.
    let mut by_team: Map<String, Value> = Map::new();
    for person in &in_scope {
        let name = if person.team.is_empty() {
            "(no team)"
        } else {
            person.team.as_str()
        };
        let count = by_team.get(name).and_then(Value::as_u64).unwrap_or(0);
        by_team.insert(name.to_owned(), json!(count + 1));
    }
    ok(json!({
        "kind": "whoWorksHere",
        "team": string_arg(args, "team"),
        "peopleCount": in_scope.len(),
        "byTeam": Value::Object(by_team),
        "shown": in_scope.len().min(MAX_LISTED),
        "people": in_scope
            .iter()
            .take(MAX_LISTED)
            .map(|person| person_json(person, &directory))
            .collect::<Vec<_>>(),
    }))
}

/// `my_leave_balance` — the asker's own balance exactly as `GET
/// /hr/leave-balances` serves it to them: one entry per live policy, each
/// with the whole working. There is no argument by which this could ask
/// about a colleague — the door's own record is the only one read.
pub async fn execute_my_leave_balance(account: &Account, _args: &Value, state: &AppState) -> Reply {
    let door = LeaveDoor::resolve(account).await?;
    let me = door.require_me()?;
    let on = today();
    let balances = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_leave_balances(&me, on)
        .await
        .map_err(map_store_err)?;
    ok(json!({
        "kind": "myLeaveBalance",
        "employeeId": me.as_str(),
        "on": iso_date(on),
        "balances": balances
            .iter()
            .map(crate::hr_leave_balances::balance_json)
            .collect::<Vec<_>>(),
    }))
}

/// One request as the model is shown it: the route's own view
/// ([`crate::hr_leave_requests::request_json`]) with the two note fields
/// stripped — the module header's one deliberate narrowing.
fn request_row(request: &LeaveRequest) -> Value {
    let mut row = crate::hr_leave_requests::request_json(request);
    if let Some(fields) = row.as_object_mut() {
        fields.remove("note");
        fields.remove("decisionNote");
    }
    row
}

/// The undecided requests among the people the asker may see — their own,
/// their reports', and for HR everybody's. The same exact-people query the
/// list route builds, never a read of everybody's filtered afterwards.
async fn waiting_requests(
    account: &Account,
    state: &AppState,
    door: &LeaveDoor,
) -> Result<Vec<LeaveRequest>, Problem> {
    let employees = if door.is_hr {
        None
    } else {
        let mut mine: Vec<HrEmployeeId> = door.me.clone().into_iter().collect();
        mine.extend(door.reports.iter().cloned());
        Some(mine)
    };
    state
        .store
        .for_tenant(account.tenant.clone())
        .hr_leave_requests(&LeaveRequestQuery {
            employees,
            statuses: vec![LeaveStatus::Requested],
            from: None,
            to: None,
        })
        .await
        .map_err(map_store_err)
}

/// `open_leave_requests` — what is still waiting for a decision, as the
/// leave screen's own queue serves it, notes stripped.
pub async fn execute_open_leave_requests(
    account: &Account,
    _args: &Value,
    state: &AppState,
) -> Reply {
    let door = LeaveDoor::resolve(account).await?;
    let requests = waiting_requests(account, state, &door).await?;
    ok(json!({
        "kind": "openLeaveRequests",
        "requestCount": requests.len(),
        "shown": requests.len().min(MAX_LISTED),
        "requests": requests
            .iter()
            .take(MAX_LISTED)
            .map(request_row)
            .collect::<Vec<_>>(),
    }))
}

/// `open_checklists` — every onboarding and offboarding checklist still open
/// for the people the asker may read, folded exactly as `GET
/// /hr/employees/{id}/checklists` folds each person's.
pub async fn execute_open_checklists(account: &Account, args: &Value) -> Reply {
    let door = LeaveDoor::resolve(account).await?;
    let directory = account.acc.hr_directory().await.map_err(map_store_err)?;
    let visible: Vec<&DirectoryEntry> = directory
        .iter()
        .filter(|person| door.may_read(&person.id))
        .collect();
    let in_scope: Vec<&DirectoryEntry> =
        match string_arg(args, "person").filter(|name| !name.trim().is_empty()) {
            None => visible,
            Some(wanted) => {
                // Resolved over the people the asker may read, so a stranger's
                // name gets the same "no colleague" answer whether the person
                // exists or not.
                let named: Vec<(String, &DirectoryEntry)> = visible
                    .iter()
                    .map(|person| (person.display_name(), *person))
                    .collect();
                vec![pick(
                    &wanted,
                    named.iter().map(|(name, e)| (name.as_str(), *e)).collect(),
                    "colleague",
                )?]
            }
        };
    let mut open = Vec::new();
    for person in in_scope {
        let checklists = account
            .acc
            .hr_employee_checklists(&person.id)
            .await
            .map_err(map_store_err)?;
        for progress in checklists.iter().filter(|list| !list.is_complete()) {
            let mut row = crate::hr_checklists::progress_json(progress);
            if let Some(fields) = row.as_object_mut() {
                fields.insert("employeeId".to_owned(), json!(person.id.as_str()));
                fields.insert("person".to_owned(), json!(person.display_name()));
            }
            open.push(row);
        }
    }
    ok(json!({
        "kind": "openChecklists",
        "openCount": open.len(),
        "shown": open.len().min(MAX_LISTED),
        "checklists": open.into_iter().take(MAX_LISTED).collect::<Vec<_>>(),
    }))
}

/// `approve_leave_request` — approve the ONE waiting request the proposal
/// names, exactly as `POST /hr/leave-requests/{id}/approve` does: the same
/// door on the person ([`LeaveDoor::require_decide`] — a manager for their
/// reports, HR for anybody, nobody for themselves), the same store refusals
/// on the record (decided twice, overdraft). The request resolves by the
/// person's name among what is waiting for the asker; several of one
/// person's are a refusal that lists their days, never a guess.
pub async fn execute_approve_leave_request(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Reply {
    let wanted = string_arg(args, "employee")
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| unprocessable("whose leave this decides is required"))?;
    let from = match string_arg(args, "from") {
        None => None,
        Some(stated) => Some(
            parse_iso_date(&stated)
                .ok_or_else(|| unprocessable("from must be a date written YYYY-MM-DD"))?,
        ),
    };
    let door = LeaveDoor::resolve(account).await?;
    let needle = wanted.to_lowercase();
    let candidates: Vec<LeaveRequest> = waiting_requests(account, state, &door)
        .await?
        .into_iter()
        .filter(|request| request.employee_name.to_lowercase().contains(&needle))
        .filter(|request| from.is_none_or(|day| request.from_day == day))
        // Only what is actually the asker's to decide: their own request is
        // in their queue but not their decision (unless they are the admin),
        // and the refusal below must not offer it as a candidate.
        .filter(|request| door.require_decide(&request.employee_id).is_ok())
        .collect();
    let request = match candidates.as_slice() {
        [one] => one,
        [] => {
            return Err(unprocessable(format!(
                "no leave request of \"{wanted}\"'s is waiting for your decision"
            )));
        }
        several => {
            return Err(unprocessable(format!(
                "more than one request matches \"{wanted}\": {} — say which first day",
                several
                    .iter()
                    .map(|request| format!(
                        "{} ({} to {})",
                        request.employee_name,
                        iso_date(request.from_day),
                        iso_date(request.to_day)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };
    let hr = state.store.for_tenant(account.tenant.clone());
    let note = string_arg(args, "note").unwrap_or_default();
    hr.decide_hr_leave_request(&request.id, true, &account.user, &note, today())
        .await
        .map_err(map_store_err)?;
    let decided = hr
        .hr_leave_request(&request.id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| unprocessable("the request vanished while being decided"))?;
    ok(json!({
        "kind": "leaveApproval",
        "request": request_row(&decided),
    }))
}

/// The module's verbs by name (A4.1c) — HR's one row in the agent's
/// dispatcher list, `crate::agent::MODULES`. `None` is "not mine": the
/// dispatcher then asks the next module. The kept executors —
/// [`crate::agent_hr`] for the absence view and the letter fill-in — are
/// reached from here so the agent has one place to look.
pub(crate) fn dispatch<'a>(
    state: &'a AppState,
    account: &'a Account,
    tool: &'a str,
    args: &'a Value,
) -> Option<crate::agent::Dispatched<'a>> {
    let run: crate::agent::Dispatched<'a> = match tool {
        "who_is_off" => Box::pin(crate::agent_hr::execute_who_is_off(account, args, state)),
        "who_works_here" => Box::pin(execute_who_works_here(account, args)),
        "my_leave_balance" => Box::pin(execute_my_leave_balance(account, args, state)),
        "open_leave_requests" => Box::pin(execute_open_leave_requests(account, args, state)),
        "open_checklists" => Box::pin(execute_open_checklists(account, args)),
        "approve_leave_request" => Box::pin(execute_approve_leave_request(account, args, state)),
        "draft_letter_from_template" => Box::pin(
            crate::agent_hr::execute_draft_letter_from_template(account, args, state),
        ),
        _ => return None,
    };
    Some(run)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use alo_ai::hr_intents::HR;

    /// Every `/hr` route the router registers is the adapter of a verb or
    /// excluded with a reason — the coverage ADR 0058 makes structural.
    #[test]
    fn every_hr_route_is_a_verb_or_an_exclusion() {
        let router = include_str!("server.rs");
        let missing = HR.uncovered(router, "/hr");
        assert!(
            missing.is_empty(),
            "routes with neither a verb nor a reason: {missing:?}"
        );
        // …and every verb's route exists, so an intent cannot name a route
        // the app does not have. `draft_letter_from_template` is the one
        // routeless verb: the fill-in deliberately has no /hr route, so the
        // letter surface itself stays HR-only.
        let routes = alo_ai::routes_in(router, "/hr");
        for intent in HR.intents {
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
        let dispatch = include_str!("hr_intents.rs");
        for intent in HR.intents {
            assert!(
                dispatch.contains(&format!("\"{}\" =>", intent.name)),
                "{} has no executor in the dispatch",
                intent.name
            );
        }
    }

    /// HR's registration is one row in each list (A4.1c): the agent's
    /// dispatcher names this module once, the registry names it once, and the
    /// two lists are the same length — every moved module has its dispatcher.
    #[test]
    fn the_module_is_one_row_in_each_list() {
        let agent = include_str!("agent.rs");
        assert_eq!(
            agent.matches("hr_intents::").count(),
            1,
            "agent.rs names HR only in MODULES"
        );
        assert!(agent.contains("crate::hr_intents::dispatch"));
        assert_eq!(
            crate::agent::MODULES.len(),
            alo_ai::MOVED.len(),
            "a moved module without a dispatcher, or the reverse"
        );
    }

    /// The module header's one deliberate narrowing, held by a test: what a
    /// model is shown of a leave request carries no note in either direction.
    #[test]
    fn a_requests_notes_never_reach_the_model() {
        let source = include_str!("hr_intents.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the module above its own tests");
        assert!(source.contains("fields.remove(\"note\")"));
        assert!(source.contains("fields.remove(\"decisionNote\")"));
        // The raw view is called exactly once — inside `request_row`, where
        // the stripping is — so no listing can bypass it.
        assert_eq!(
            source
                .matches("crate::hr_leave_requests::request_json(")
                .count(),
            1,
            "a listing bypasses request_row"
        );
    }

    /// The widening a later hand would reach for — the full employee record,
    /// a document, a candidate, payroll — is not called here, so a verb
    /// cannot quietly start reading what the directory was careful not to
    /// carry.
    #[test]
    fn this_module_reads_the_public_projections_and_nothing_private() {
        let source = include_str!("hr_intents.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the module above its own tests");
        for forbidden in [
            "hr_employee(",
            "hr_employees(",
            "hr_employments(",
            "hr_documents(",
            "hr_applicants(",
            "hr_applicant_notes(",
            "hr_payroll",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
        assert!(source.contains("hr_directory()"));
    }
}
