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
//!
//! The second tool, `draft_letter_from_template` (B6.09b), obeys the same three
//! rules and adds a fourth of its own: **the company's words, never ours.** It
//! merges a template the tenant wrote with facts out of the member directory,
//! and there is no branch anywhere below that composes prose — a letter nobody
//! in the company has written is a refusal that names the ones they have.

use axum::Json;
use serde_json::{Value, json};
use time::Date;

use alo_store::hr_letters::{LetterFacts, LetterTemplate, render_letter};
use alo_store::{AbsenceDay, DirectoryEntry};

use crate::agent_args::{pick, string_arg, unprocessable};
use crate::billing::{iso_date, map_store_err, parse_iso_date};
use crate::billing_document::today;
use crate::drafts;
use crate::error::Problem;
use crate::hr_leave_balances::absence_json;
use crate::hr_leave_door::LeaveDoor;
use crate::mime::{Addr, Outgoing};
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

/// `draft_letter_from_template` — fill one of the tenant's **own** letters in
/// about a colleague and leave it in the caller's Drafts (B6.09b).
///
/// The whole act, in order: resolve the colleague over the directory the caller
/// can already see, check the door on that *person*, resolve the letter over the
/// tenant's templates, merge the directory facts and the company's letterhead
/// into the stored text, and save the result as a draft. **Nothing is sent**,
/// nothing is filed on the person's record, and nobody is told.
///
/// Two doors, not one. The template surface itself is HR's
/// ([`crate::hr_letters`]) — browsing and *writing* the letters this company
/// will put its name to is an HR screen. Filling one in is subject to the door
/// on the person ([`LeaveDoor`]): HR, their manager, or themselves, so "draft my
/// employment confirmation for my landlord" is a thing somebody can ask for
/// about their own record and about nobody else's.
///
/// A name that matches no template is a refusal that **names the letters that
/// exist** — the one answer that keeps a model from filling the gap itself,
/// which is the failure this whole design exists to prevent.
///
/// # Errors
/// `422` when the letter or the colleague is not stated, names nothing of this
/// tenant's, or matches more than one; `422` when the letter states a fact the
/// person has not got, or when `to` is not an address; `500` on a store failure.
pub async fn execute_draft_letter_from_template(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Json<Value>, Problem> {
    let wanted_letter = string_arg(args, "template")
        .or_else(|| string_arg(args, "letter"))
        .ok_or_else(|| {
            unprocessable(
                "which letter is required: this fills in a letter your company has written, \
                 and never writes one of its own",
            )
        })?;
    let wanted_person = string_arg(args, "employee")
        .or_else(|| string_arg(args, "person"))
        .ok_or_else(|| unprocessable("who the letter is about is required"))?;
    let to = stated_address(args)?;

    // The colleague, resolved over the directory every member already reads —
    // never over a record — and then the door on that person. A colleague the
    // caller may not write about is refused in the **same words** as one who
    // does not exist: an answer that distinguished them would confirm who works
    // here to somebody who cannot see it.
    let directory = account.acc.hr_directory().await.map_err(map_store_err)?;
    let named: Vec<(String, &DirectoryEntry)> = directory
        .iter()
        .map(|person| (person.display_name(), person))
        .collect();
    let person = pick(
        &wanted_person,
        named.iter().map(|(name, e)| (name.as_str(), *e)).collect(),
        "colleague",
    )?;
    let door = LeaveDoor::resolve(account).await?;
    if !door.may_read(&person.id) {
        return Err(unprocessable(format!(
            "no colleague of yours is called {wanted_person}"
        )));
    }

    let templates = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_letter_templates()
        .await
        .map_err(map_store_err)?;
    let template = pick_letter(&templates, &wanted_letter)?;

    let company = account
        .acc
        .billing_settings()
        .await
        .map_err(map_store_err)?;
    // `today` is the server's own date, never a caller's: a letter dated by an
    // argument is a letter somebody can back-date by asking.
    let facts = LetterFacts::of(person, &company, today());
    let letter = render_letter(template, &facts).map_err(map_store_err)?;

    let from = drafts::from_address(account, state).await?;
    let outgoing = Outgoing {
        from: Addr {
            name: None,
            email: from.clone(),
        },
        // No recipient unless the user named one. A letter for a landlord goes
        // to an address only they know, and a draft addressed to the *colleague*
        // by default would eventually be sent to them by somebody clicking send.
        to: to
            .iter()
            .map(|email| Addr {
                name: None,
                email: email.clone(),
            })
            .collect(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: letter.subject.clone(),
        in_reply_to: Vec::new(),
        references: Vec::new(),
        body_text: letter.body,
        body_html: None,
        attachments: Vec::new(),
        message_id_domain: crate::api::domain_of(&from),
        message_id_token: crate::api::new_message_token(),
    };
    let saved = drafts::save(account, &outgoing).await?;

    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "letterDraft",
            "id": saved.as_str(),
            "template": template.name,
            "templateId": template.id.as_str(),
            "employee": person.display_name(),
            "employeeId": person.id.as_str(),
            "subject": letter.subject,
            "to": to.unwrap_or_default(),
            // What the letter states about them, by field name — so the card can
            // say what went into it without quoting the letter back at somebody
            // sitting in an open-plan office.
            "fields": template.fields.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        }
    })))
}

/// Resolves the letter the proposal named to exactly one of the tenant's
/// templates.
///
/// The refusal is the point of this function. A miss answers with **the letters
/// that exist**, and a tenant with none is told who writes the first one —
/// because "no such template" is the moment a model is most tempted to write the
/// letter itself, and the only cure is an answer that says what to do instead.
fn pick_letter<'a>(
    templates: &'a [LetterTemplate],
    wanted: &str,
) -> Result<&'a LetterTemplate, Problem> {
    let candidates = templates
        .iter()
        .map(|template| (template.name.as_str(), template))
        .collect();
    match crate::agent_args::pick_name(wanted, candidates, "letter") {
        Ok(template) => Ok(template),
        Err(_) if templates.is_empty() => Err(unprocessable(
            "your company has not written any letter templates yet — somebody in HR writes the \
             letter once, under HR settings, and this fills it in from then on",
        )),
        Err(message) => Err(unprocessable(format!(
            "{message}. The letters your company has written are: {}",
            templates
                .iter()
                .map(|template| template.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The address the draft is put to, when the proposal named one.
///
/// Absent is ordinary and is the default: a letter for a landlord goes to an
/// address only the user knows, so the draft waits for it. What is refused is a
/// *sentence* where an address belongs — "her landlord" in a `To:` header is a
/// draft that fails on send with a message nobody can act on.
fn stated_address(args: &Value) -> Result<Option<String>, Problem> {
    let Some(stated) = string_arg(args, "to") else {
        return Ok(None);
    };
    let parts: Vec<&str> = stated.split('@').collect();
    let plausible = parts.len() == 2
        && parts.iter().all(|part| !part.is_empty())
        && parts[1].contains('.')
        && !stated.chars().any(char::is_whitespace);
    if !plausible {
        return Err(unprocessable(format!(
            "{stated} is not an email address; leave it out and the draft waits for one"
        )));
    }
    Ok(Some(stated))
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

    /// One of the tenant's letters, as the store hands it over.
    fn letter(name: &str) -> LetterTemplate {
        LetterTemplate {
            id: alo_store::HrLetterTemplateId::new(format!("t-{name}")),
            name: name.to_owned(),
            subject: "Verklaring voor {{employee.name}}".to_owned(),
            body: "In dienst sinds {{employee.started_on}}.".to_owned(),
            fields: Vec::new(),
            created_by: "u-1".to_owned(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
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
    fn a_letter_nobody_wrote_is_refused_with_the_letters_somebody_did() {
        // The refusal this whole design turns on: a model that hears "no such
        // template" and writes the employment confirmation itself. The answer
        // names what exists, so the next move is picking one rather than
        // inventing one.
        let templates = [letter("Werkgeversverklaring"), letter("Reference")];
        let refused = pick_letter(&templates, "salary certificate").expect_err("invented a letter");
        assert_eq!(refused.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        let detail = refused.detail.unwrap_or_default();
        assert!(detail.contains("no letter of yours"), "{detail}");
        assert!(
            detail.contains("Werkgeversverklaring, Reference"),
            "{detail}"
        );

        // A tenant with none is told who writes the first one, rather than being
        // told a list that is empty.
        let none: [LetterTemplate; 0] = [];
        let detail = pick_letter(&none, "anything")
            .expect_err("filled in a letter from an empty company")
            .detail
            .unwrap_or_default();
        assert!(detail.contains("has not written any"), "{detail}");
        assert!(detail.contains("HR"), "{detail}");
    }

    #[test]
    fn a_letter_is_matched_by_name_and_an_ambiguous_one_is_asked_about() {
        let templates = [
            letter("Werkgeversverklaring"),
            letter("Werkgeversverklaring 2026"),
            letter("Reference"),
        ];
        // An exact name wins over the longer one that contains it.
        assert_eq!(
            pick_letter(&templates, "werkgeversverklaring")
                .expect("the exact name")
                .name,
            "Werkgeversverklaring"
        );
        assert_eq!(
            pick_letter(&templates, "refer")
                .expect("one partial match")
                .name,
            "Reference"
        );
        let detail = pick_letter(&templates, "verklaring")
            .expect_err("guessed between two")
            .detail
            .unwrap_or_default();
        assert!(detail.contains("more than one"), "{detail}");
    }

    #[test]
    fn an_address_is_optional_and_a_sentence_is_not_an_address() {
        assert_eq!(stated_address(&json!({})).expect("none is ordinary"), None);
        assert_eq!(
            stated_address(&json!({ "to": " landlord@example.test " })).expect("an address"),
            Some("landlord@example.test".to_owned())
        );
        for bad in [
            "her landlord",
            "landlord@example",
            "@example.test",
            "landlord@",
            "two people@example.test",
            "a@b.test, c@d.test",
        ] {
            let refused = stated_address(&json!({ "to": bad })).expect_err("accepted {bad}");
            assert_eq!(refused.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                refused.detail.unwrap_or_default().contains("not an email"),
                "{bad}"
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
        // The letter tool reads the two things it is allowed to and nothing
        // else: the member **directory** (which carries no private field at all,
        // by the type's own construction) and the tenant's letter templates. A
        // hand that reached for `hr_employee(` to get a home address onto a
        // letter fails on the list above, which is where that decision belongs.
        assert!(source.contains("hr_directory()"));
        assert!(source.contains("hr_letter_templates()"));
    }
}
