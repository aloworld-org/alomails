//! Executing the **HR** tools of an approved agent proposal (ADR 0034, ADR 0035
//! wave B6.09) — the acting half of what [`alo_ai::agent_hr`] describes to the
//! model.
//!
//! Called only from [`crate::agent::agent_execute`], which is the single acting
//! path: the user saw the proposal and approved it. The read runs through the
//! caller's own tenant, so an agent can no more see another company's people
//! than the browser that asked it can.
//!
//! Three rules shape this module, and they are why it is not thin glue:
//!
//! - **The same store function as the screen.** `who_is_off` reads
//!   [`alo_store::TenantStore::hr_absences`] — the layer the Agenda and the
//!   leave-request form already draw (B6.03b), which loads a name, an employee
//!   id and a day and *does not select* the policy, the kind of leave or the
//!   note. There is no second path to who is away in this product, so the agent
//!   and the calendar cannot disagree, and neither can be made to disclose a
//!   reason that was never loaded.
//! - **The same gate as that layer, which is: every member.** Absence is the
//!   module's one read everybody gets (`docs/design/hr.md` § "The absence
//!   layer"), so this executor adds no role check — adding one here and not on
//!   `GET /hr/absences` would be a second answer to the same question. What it
//!   must never do is *widen*: nothing here reads an employee record, a
//!   balance, a request or a document, and the test at the foot of this file
//!   says so by name.
//! - **Figures and names, never a sentence.** The roll-up counts days and
//!   states the first and last a person is away; the words around them are the
//!   client's, in the reader's own catalogue, because a sentence composed in the
//!   server is a user-facing string in one language (CLAUDE.md).
//!
//! The roll-up is deliberately *not* a range. Somebody away on Monday and on
//! Friday is away two days, and a card that drew "Mon–Fri" from those two rows
//! would put three days of absence on a person who worked them.

use axum::Json;
use serde_json::{Value, json};
use time::Date;

use alo_store::AbsenceDay;

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::error::Problem;
use crate::hr_leave_balances::absence_json;
use crate::state::{Account, AppState};

/// `who_is_off` — which colleagues are away over a stated range of days.
///
/// The range is the proposal's: `from` is required and `to` defaults to it, so
/// "who is off on Friday" is one day rather than a window somebody has to
/// interpret. The store owns the two limits that make an unbounded read
/// impossible — a range that ends before it starts, and one longer than a year
/// — and answers both as a `422` in its own words.
///
/// # Errors
/// `422` when `from` is missing or either day is not written `YYYY-MM-DD`, and
/// for the store's own range refusals; `500` on a store failure.
pub async fn execute_who_is_off(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let from = stated_day(args, "from")?;
    // A day stated on its own is a question about that day. Only an explicitly
    // stated `to` widens it, so a model that omitted one has asked something
    // narrower than it meant rather than something wider.
    let to = match string_arg(args, "to") {
        Some(_) => stated_day(args, "to")?,
        None => from,
    };
    let days = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_absences(from, to)
        .await
        .map_err(map_store_err)?;
    let people = away_people(&days);

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "whoIsOff",
            "from": iso_date(from),
            "to": iso_date(to),
            // The window it actually looked at, stated rather than left to a
            // reader subtracting two dates: "nobody is off" means nothing
            // without the days it means it over.
            "daysInRange": (to - from).whole_days() + 1,
            "people": people.iter().map(person_json).collect::<Vec<_>>(),
            // The layer's own shape, unchanged: the same days the Agenda draws.
            "days": days.iter().map(absence_json).collect::<Vec<_>>(),
        }
    })))
}

/// One colleague the window found away, and how much of it they are away for.
#[derive(Debug, PartialEq, Eq)]
struct PersonAway {
    /// Their employee id, so a client can open the directory entry it is
    /// already allowed to read.
    employee_id: String,
    /// Their name as the directory shows it.
    name: String,
    /// How many days of the window they are away. Not a duration: two rows are
    /// two days whether or not they touch.
    away_days: usize,
    /// The first day of the window they are away.
    first_day: Date,
    /// The last. Equal to the first for a single day, and **not** a promise
    /// that everything between them is absence.
    last_day: Date,
}

/// Folds the layer's day-by-day answer into one line per person.
///
/// Pure, and separated from the read for the reason the module header gives:
/// what a reader is told about a colleague is testable here without a database.
///
/// People are ordered by the name the directory shows them under, so the same
/// window always reads the same way round; the store's own per-day ordering is
/// by family name, which is an order for a *day*, not for a list of people.
fn away_people(days: &[AbsenceDay]) -> Vec<PersonAway> {
    let mut people: Vec<PersonAway> = Vec::new();
    for day in days {
        for person in &day.people {
            match people
                .iter_mut()
                .find(|seen| seen.employee_id == person.employee_id.as_str())
            {
                Some(seen) => {
                    seen.away_days += 1;
                    // The layer returns days in order, but the fold does not
                    // depend on it: a day out of order still widens the ends
                    // rather than replacing them.
                    if day.day < seen.first_day {
                        seen.first_day = day.day;
                    }
                    if day.day > seen.last_day {
                        seen.last_day = day.day;
                    }
                }
                None => people.push(PersonAway {
                    employee_id: person.employee_id.as_str().to_owned(),
                    name: person.name.clone(),
                    away_days: 1,
                    first_day: day.day,
                    last_day: day.day,
                }),
            }
        }
    }
    people.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            // Two colleagues with the same name are two people, and the id is
            // the only thing that tells them apart.
            .then_with(|| a.employee_id.cmp(&b.employee_id))
    });
    people
}

/// One person, as the answer states them. Four fields, and no fifth to leak.
fn person_json(person: &PersonAway) -> Value {
    json!({
        "employeeId": person.employee_id,
        "name": person.name,
        "awayDays": person.away_days,
        "firstDay": iso_date(person.first_day),
        "lastDay": iso_date(person.last_day),
    })
}

/// A day the proposal states. `from` has no default; `to` is only read when the
/// proposal stated one.
///
/// The refusal names which end is wrong, so a caller with two malformed dates
/// learns which one it is being told about — the same courtesy
/// [`crate::agent_finance_answers`] extends to a period, in the same words.
fn stated_day(args: &Value, name: &str) -> Result<Date, Problem> {
    let stated = string_arg(args, name).ok_or_else(|| {
        unprocessable(format!(
            "{name} is required: an absence answer is always about stated days"
        ))
    })?;
    parse_iso_date(&stated)
        .ok_or_else(|| unprocessable(format!("{name} must be a date written YYYY-MM-DD")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use alo_store::{AbsentPerson, HrEmployeeId};

    fn day(iso: &str) -> Date {
        parse_iso_date(iso).expect("a plain day")
    }

    fn absent(id: &str, name: &str) -> AbsentPerson {
        AbsentPerson {
            employee_id: HrEmployeeId::new(id.to_owned()),
            name: name.to_owned(),
        }
    }

    fn away(iso: &str, people: Vec<AbsentPerson>) -> AbsenceDay {
        AbsenceDay {
            day: day(iso),
            people,
        }
    }

    #[test]
    fn one_person_over_several_days_is_one_line_with_the_days_counted() {
        let people = away_people(&[
            away("2026-08-10", vec![absent("e1", "Amara van den Berg")]),
            away("2026-08-11", vec![absent("e1", "Amara van den Berg")]),
            away("2026-08-12", vec![absent("e1", "Amara van den Berg")]),
        ]);
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].away_days, 3);
        assert_eq!(people[0].first_day, day("2026-08-10"));
        assert_eq!(people[0].last_day, day("2026-08-12"));
        assert_eq!(people[0].employee_id, "e1");
    }

    #[test]
    fn two_days_apart_are_two_days_and_never_the_span_between_them() {
        // The module header's rule: a person off on Monday and on Friday worked
        // the three days in between, and no field here says otherwise.
        let people = away_people(&[
            away("2026-08-10", vec![absent("e1", "Amara")]),
            away("2026-08-14", vec![absent("e1", "Amara")]),
        ]);
        assert_eq!(people[0].away_days, 2);
        assert_eq!(people[0].first_day, day("2026-08-10"));
        assert_eq!(people[0].last_day, day("2026-08-14"));
        // Nothing in what travels claims the days between are absence.
        let stated = person_json(&people[0]);
        assert_eq!(stated["awayDays"], 2);
        assert_eq!(stated["firstDay"], "2026-08-10");
        assert_eq!(stated["lastDay"], "2026-08-14");
    }

    #[test]
    fn people_are_listed_in_one_order_however_the_days_arrive() {
        let people = away_people(&[
            away(
                "2026-08-10",
                vec![absent("e2", "Zoë Bakker"), absent("e1", "amara")],
            ),
            away("2026-08-11", vec![absent("e3", "Mikkel Sørensen")]),
        ]);
        let names: Vec<&str> = people.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(names, ["amara", "Mikkel Sørensen", "Zoë Bakker"]);
    }

    #[test]
    fn two_colleagues_of_the_same_name_stay_two_people() {
        let people = away_people(&[away(
            "2026-08-10",
            vec![absent("e2", "Jan Jansen"), absent("e1", "Jan Jansen")],
        )]);
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].employee_id, "e1");
        assert_eq!(people[1].employee_id, "e2");
        assert!(people.iter().all(|one| one.away_days == 1));
    }

    #[test]
    fn a_window_with_nobody_away_is_an_answer_rather_than_nothing() {
        assert!(away_people(&[]).is_empty());
    }

    #[test]
    fn a_day_out_of_order_widens_the_ends_rather_than_replacing_them() {
        let people = away_people(&[
            away("2026-08-14", vec![absent("e1", "Amara")]),
            away("2026-08-10", vec![absent("e1", "Amara")]),
        ]);
        assert_eq!(people[0].first_day, day("2026-08-10"));
        assert_eq!(people[0].last_day, day("2026-08-14"));
    }

    #[test]
    fn nothing_a_person_is_told_carries_a_reason_a_kind_or_a_note() {
        // The disclosure rule, as code. `AbsentPerson` has two fields and
        // `AbsenceDay` has two, so there is nothing here to leak — this test is
        // what fails if a later hand widens the answer with a field the layer
        // was careful not to load.
        let stated =
            person_json(&away_people(&[away("2026-08-10", vec![absent("e1", "Amara")])])[0]);
        let object = stated.as_object().expect("an object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["awayDays", "employeeId", "firstDay", "lastDay", "name"]
        );
    }

    #[test]
    fn the_first_day_is_required_and_the_refusal_says_which_end_is_missing() {
        let problem = stated_day(&json!({}), "from").expect_err("accepted a missing day");
        assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        let detail = problem.detail.unwrap_or_default();
        assert!(detail.starts_with("from"), "{detail}");
        assert!(detail.contains("required"), "{detail}");
        assert_eq!(
            stated_day(&json!({ "from": " 2026-08-10 " }), "from").unwrap(),
            day("2026-08-10")
        );
    }

    #[test]
    fn a_day_that_is_not_a_plain_day_is_refused_rather_than_guessed() {
        for bad in [
            "next week",
            "10/08/2026",
            "2026-08-10T00:00:00Z",
            "2026-02-30",
        ] {
            let problem =
                stated_day(&json!({ "from": bad }), "from").expect_err("accepted a bad day");
            assert_eq!(problem.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                problem.detail.as_deref(),
                Some("from must be a date written YYYY-MM-DD")
            );
        }
    }

    #[test]
    fn this_module_reads_the_absence_layer_and_nothing_else_about_a_person() {
        // The header's second rule, held by the source itself: the widening a
        // later hand would reach for — a record, a balance, a request, a
        // document, an applicant — is not called here, and a `who_is_off` that
        // started calling one would be a disclosure nobody reviewed.
        // The acting half only: the names below appear in this list too, and a
        // test that matched itself would pass on any file that contained it.
        let source = include_str!("agent_hr.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("the module above its own tests");
        for forbidden in [
            "hr_employee(",
            "hr_employees(",
            "hr_employments(",
            "hr_leave_balances(",
            "hr_leave_requests(",
            "hr_documents(",
            "hr_applicants(",
            "hr_applicant_notes(",
            "hr_payroll",
        ] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
        assert!(source.contains("hr_absences(from, to)"));
    }
}
