//! Who is away, and on which days (alo HR, ADR 0035, wave B6.03b;
//! `docs/design/hr.md`, "The absence layer, and why it is not a calendar").
//!
//! This is the module's **one read every member gets about other people**, and
//! it discloses exactly what a team needs in order to plan: *who is not here*.
//! A name, an employee id, and a day. Never the policy, never the kind of
//! leave, never the note — the query below does not even select them, so no
//! careless line downstream can leak what was never loaded.
//!
//! # Why this is not a calendar
//!
//! The obvious design is to write approved leave into a shared calendar and let
//! the Agenda render it. It was rejected in the design note for three reasons,
//! and the shape of this module is what enforces the rejection:
//!
//! - **Every calendar has an owner.** There is no tenant-owned calendar in this
//!   schema, so an absence calendar would belong to a *person* who could then
//!   delete an approved absence they never decided, from a screen that knows
//!   nothing about leave.
//! - **It would be a second source of truth**, drifting the first time a
//!   cancelled request failed to remove its event — somebody marked absent for a
//!   week they worked.
//! - **Events are indiscreet.** A calendar event has a title, and a title in a
//!   shared calendar is the thing most likely to end up saying "Sick —
//!   hospital". A derived layer has the fields we chose and cannot acquire a
//!   fourth by somebody typing into one.
//!
//! So the Agenda draws this read *behind* its week and month views, and the
//! leave-request form draws the same layer behind its date picker.
//!
//! # What counts as away
//!
//! An **approved** request, on a day the person normally works. Somebody whose
//! leave covers a Saturday they never work is not "away" on that Saturday, for
//! the same reason it costs them nothing: the working pattern is what makes a
//! day mean anything (`docs/design/hr.md`, "Minutes, and the working pattern").
//! People who have left the directory are not listed — a planning read is about
//! the team there is.

use std::collections::BTreeMap;

use time::Date;

use crate::error::{Result, StoreError};
use crate::hr_employees::display_name;
use crate::hr_employments::Employment;
use crate::id::HrEmployeeId;
use crate::store::TenantStore;

/// The widest window the layer will answer for. A year of team absence is
/// already more than any view draws, and an unbounded range is a read whose
/// cost the caller chooses.
pub const ABSENCE_WINDOW_MAX_DAYS: i64 = 366;

/// One person who is away — everything the layer discloses about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsentPerson {
    /// Their employee id, so a client can open the directory entry it is
    /// already allowed to read.
    pub employee_id: HrEmployeeId,
    /// Their name as the directory shows it.
    pub name: String,
}

/// One day of the window, and who is not here on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsenceDay {
    /// The day.
    pub day: Date,
    /// The people away on it, by name. Days with nobody away are not returned.
    pub people: Vec<AbsentPerson>,
}

/// One approved absence as the layer loads it: a person, two days, and no
/// fourth column to leak. The name parts are folded by
/// [`crate::hr_employees`]' own rule rather than by an expression in this SQL,
/// so a preferred name is honoured here exactly as it is in the directory.
#[derive(sqlx::FromRow)]
struct AbsenceRow {
    employee_id: String,
    preferred_name: String,
    given_name: String,
    family_name: String,
    from_day: Date,
    to_day: Date,
}

impl AbsenceRow {
    /// What to call this person — the directory's rule, not a second one.
    fn name(&self) -> String {
        display_name(&self.preferred_name, &self.given_name, &self.family_name)
    }
}

impl TenantStore {
    /// Who is away on each day of `from..=to`, days with nobody away omitted.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the window ends before it starts or is
    /// longer than [`ABSENCE_WINDOW_MAX_DAYS`]; [`StoreError::Db`] on failure.
    pub async fn hr_absences(&self, from: Date, to: Date) -> Result<Vec<AbsenceDay>> {
        if to < from {
            return Err(StoreError::Validation(
                "the window must end on or after the day it starts".to_owned(),
            ));
        }
        if (to - from).whole_days() + 1 > ABSENCE_WINDOW_MAX_DAYS {
            return Err(StoreError::Validation(format!(
                "the window must not be longer than {ABSENCE_WINDOW_MAX_DAYS} days"
            )));
        }
        let rows = sqlx::query_as::<_, AbsenceRow>(
            "SELECT r.employee_id, e.preferred_name, e.given_name, e.family_name, \
                 r.from_day, r.to_day \
               FROM hr_leave_requests r \
               JOIN hr_employees e ON e.tenant_id = r.tenant_id AND e.id = r.employee_id \
              WHERE r.tenant_id = $1 AND r.status = 'approved' \
                AND e.archived_at IS NULL \
                AND r.from_day <= $3 AND r.to_day >= $2 \
              ORDER BY e.family_name, e.given_name, r.from_day",
        )
        .bind(self.tenant().as_str())
        .bind(from)
        .bind(to)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;

        // The working pattern decides which days of an absence are days off, so
        // each person's terms are read once however many absences they have in
        // the window.
        let mut terms: BTreeMap<String, Vec<Employment>> = BTreeMap::new();
        let mut days: BTreeMap<Date, Vec<AbsentPerson>> = BTreeMap::new();
        for row in rows {
            if !terms.contains_key(&row.employee_id) {
                let read = self
                    .hr_employments(&HrEmployeeId::new(row.employee_id.clone()))
                    .await?;
                terms.insert(row.employee_id.clone(), read);
            }
            let employments = terms.get(&row.employee_id).map_or(&[][..], Vec::as_slice);
            let mut day = row.from_day.max(from);
            let last = row.to_day.min(to);
            while day <= last {
                let works = employments
                    .iter()
                    .find(|employment| employment.covers(day))
                    .is_some_and(|employment| employment.minutes_on(day) > 0);
                if works {
                    let person = AbsentPerson {
                        employee_id: HrEmployeeId::new(row.employee_id.clone()),
                        name: row.name(),
                    };
                    let people = days.entry(day).or_default();
                    if !people.contains(&person) {
                        people.push(person);
                    }
                }
                match day.next_day() {
                    Some(next) => day = next,
                    None => break,
                }
            }
        }
        Ok(days
            .into_iter()
            .map(|(day, people)| AbsenceDay { day, people })
            .collect())
    }
}
