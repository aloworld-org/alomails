//! Asking for time off, and the decision on it (alo HR, ADR 0035, wave B6.03b;
//! `docs/design/hr.md`, "The request, and its state machine").
//!
//! # What a request is
//!
//! A first day, a last day, a policy, and a person. **Not** a number of minutes:
//! what an absence costs is folded at read time from the working pattern of the
//! employment in force on each of its days ([`crate::hr_leave_math`]), so a
//! corrected pattern corrects every figure that was ever shown from it. There is
//! no `cost_minutes` column for the same reason there is no `balance_minutes`
//! one — a stored figure nothing can reconcile is the `qty_on_hand` mistake
//! (B5.01) with somebody's holiday in it.
//!
//! # The state machine
//!
//! ```text
//!               withdraw
//!         ┌──────────────────────► withdrawn
//!         │
//!    requested ──approve──► approved ──cancel──► cancelled
//!         │                     │
//!         └──reject──► rejected └──(days pass)──► taken (derived, never stored)
//! ```
//!
//! - **`requested`** is editable — dates and note — and by nobody but its owner
//!   (the door decides that; the store only enforces the status).
//! - **`approved`** is not editable at all. Changing approved leave is a cancel
//!   and a new request, so the history reads as what happened.
//! - **`cancel` works while the leave has not started.** The fact that somebody
//!   was absent last Tuesday is not something a cancel button may erase.
//! - **`taken` is derived** by comparing the days to today, never stored, so
//!   there is no nightly job and one less state to get wrong.
//!
//! # What the store refuses
//!
//! - A range that ends before it starts, or is longer than
//!   [`crate::hr_leave_math::REQUEST_MAX_DAYS`] — `Validation`.
//! - Days outside the person's employment, on either side — `Validation` naming
//!   the bound. Leave cannot be taken from a job somebody did not hold.
//! - A range that costs nothing (a weekend, for this person's pattern) —
//!   `Validation`, because a zero-minute absence is a mis-typed date rather than
//!   a request.
//! - Days another live request of the same person already covers — `Conflict`
//!   **naming that request**, which is the difference between a screen that
//!   explains itself and one that says "constraint violated".
//! - An approval that would take the balance below zero on a policy that does
//!   not allow it — `Conflict` naming the shortfall in minutes.
//! - Any transition the machine above does not have an arrow for — `Conflict`
//!   naming what the request actually is.
//!
//! **Who may approve is not decided here.** It needs the caller's roles and the
//! reporting line, which are the HTTP layer's to know
//! (`products/mail/alo-jmap/src/hr_leave_requests.rs`); this module refuses on
//! the *record's* rules and would refuse the same way whoever asked.

use time::{Date, OffsetDateTime};

use crate::billing_field::bounded;
use crate::error::{Result, StoreError};
use crate::hr_employees::display_name;
use crate::hr_employments::Employment;
use crate::hr_holidays::TenantHolidays;
use crate::hr_leave_math::{REQUEST_MAX_DAYS, RequestCost, RequestedDay, request_cost};
use crate::id::{HrEmployeeId, HrLeavePolicyId, HrLeaveRequestId, UserId};
use crate::store::TenantStore;

/// The longest a note may be — the reason somebody gives, or the reason a
/// manager gives back. Matched by the schema's CHECK.
pub const LEAVE_NOTE_MAX_CHARS: usize = 2_000;

/// The most requests one read returns. A person asks for leave a few dozen
/// times a year; a tenant-wide list bounded at this is a page nobody scrolls
/// past, and an unbounded one is a read that grows without limit.
pub const LEAVE_REQUEST_PAGE_MAX: i64 = 500;

/// Where a request has got to.
///
/// A closed vocabulary matched by the CHECK one layer down, because a word no
/// code knows is a state nothing can compute a balance from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveStatus {
    /// Asked for, nobody has decided.
    Requested,
    /// Decided yes. Consumes the balance from this moment, whether its days are
    /// past (*taken*) or ahead (*booked*).
    Approved,
    /// Decided no. Costs nothing and stays, because "my manager said no" is part
    /// of the record.
    Rejected,
    /// Taken back by the person who asked, before anybody decided.
    Withdrawn,
    /// Approved leave that was cancelled before it started. The balance comes
    /// back; the approval that preceded it stays on the row.
    Cancelled,
}

impl LeaveStatus {
    /// The stored word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Withdrawn => "withdrawn",
            Self::Cancelled => "cancelled",
        }
    }

    /// Reads a status — from a query string or from a stored row.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "requested" => Ok(Self::Requested),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "withdrawn" => Ok(Self::Withdrawn),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(StoreError::Validation(
                "leave status must be one of: requested, approved, rejected, withdrawn, cancelled"
                    .to_owned(),
            )),
        }
    }

    /// Whether a request in this state still occupies its days — the two states
    /// an overlapping request collides with, and the two the absence layer and
    /// the approvals queue read.
    #[must_use]
    pub fn is_live(self) -> bool {
        matches!(self, Self::Requested | Self::Approved)
    }
}

impl std::fmt::Display for LeaveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The writable shape of a request.
#[derive(Debug, Clone)]
pub struct NewLeaveRequest {
    /// Whose absence it is.
    pub employee_id: HrEmployeeId,
    /// Which policy it comes off.
    pub policy_id: HrLeavePolicyId,
    /// First day of the absence, inclusive.
    pub from_day: Date,
    /// Last day of the absence, inclusive.
    pub to_day: Date,
    /// What the person wrote when they asked. Optional everywhere.
    pub note: String,
}

/// One stored request, with the cost folded from the days it covers.
#[derive(Debug, Clone)]
pub struct LeaveRequest {
    /// Opaque id, unique within the tenant.
    pub id: HrLeaveRequestId,
    /// Whose absence it is.
    pub employee_id: HrEmployeeId,
    /// Their name as the directory shows it — the one field a queue needs and
    /// the only thing about the person this type carries.
    pub employee_name: String,
    /// The policy it comes off.
    pub policy_id: HrLeavePolicyId,
    /// That policy's name, so a list does not need a second read.
    pub policy_name: String,
    /// First day, inclusive.
    pub from_day: Date,
    /// Last day, inclusive.
    pub to_day: Date,
    /// Where it has got to.
    pub status: LeaveStatus,
    /// What the person wrote.
    pub note: String,
    /// The user who asked.
    pub requested_by: String,
    /// The user who decided, if anybody has.
    pub decided_by: Option<String>,
    /// When they decided.
    pub decided_at: Option<OffsetDateTime>,
    /// What they wrote with the decision.
    pub decision_note: String,
    /// The user who withdrew or cancelled it.
    pub closed_by: Option<String>,
    /// When they did.
    pub closed_at: Option<OffsetDateTime>,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
    /// **Derived, never stored**: what these days cost at the working pattern in
    /// force on each of them.
    pub cost: RequestCost,
}

impl LeaveRequest {
    /// Whether the request still occupies its days.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.status.is_live()
    }

    /// Whether approved leave has already begun on `today` — the line
    /// [`TenantStore::cancel_hr_leave_request`] refuses to cross.
    #[must_use]
    pub fn has_started(&self, today: Date) -> bool {
        self.from_day <= today
    }
}

/// Which requests a read wants.
#[derive(Debug, Clone, Default)]
pub struct LeaveRequestQuery {
    /// The people to answer about. `None` is every person of the tenant — the
    /// HR scope, and the only one that is not a list of ids resolved by the
    /// door.
    pub employees: Option<Vec<HrEmployeeId>>,
    /// The states to include. Empty is every state.
    pub statuses: Vec<LeaveStatus>,
    /// Only requests touching this day or later.
    pub from: Option<Date>,
    /// Only requests touching this day or earlier.
    pub to: Option<Date>,
}

impl LeaveRequestQuery {
    /// Requests of one person, in every state — somebody's own list.
    #[must_use]
    pub fn for_employee(employee: &HrEmployeeId) -> Self {
        Self {
            employees: Some(vec![employee.clone()]),
            ..Self::default()
        }
    }

    /// Narrows to the states given.
    #[must_use]
    pub fn with_statuses(mut self, statuses: &[LeaveStatus]) -> Self {
        self.statuses = statuses.to_vec();
        self
    }

    /// Narrows to requests touching the window.
    #[must_use]
    pub fn within(mut self, from: Date, to: Date) -> Self {
        self.from = Some(from);
        self.to = Some(to);
        self
    }
}

/// The columns every read of a request selects, in `RequestRow` order.
///
/// The three name parts come back as they are stored and are folded into a name
/// by [`crate::hr_employees`]' own rule, rather than by an expression spelled
/// into this SQL: two rules for what somebody is called is one rule too many,
/// and the second one is always the one that forgets the preferred name.
const REQUEST_COLS: &str = "r.id, r.employee_id, \
     e.preferred_name, e.given_name, e.family_name, \
     r.policy_id, p.name AS policy_name, r.from_day, r.to_day, r.status, r.note, \
     r.requested_by, r.decided_by, r.decided_at, r.decision_note, r.closed_by, r.closed_at, \
     r.created_at, r.updated_at";

/// The joins those columns need: a request is never read without the person it
/// is about and the policy it comes off, because a list of dates with two
/// opaque ids on it is not a screen anybody can read.
const REQUEST_FROM: &str = "FROM hr_leave_requests r \
     JOIN hr_employees e ON e.tenant_id = r.tenant_id AND e.id = r.employee_id \
     JOIN hr_leave_policies p ON p.tenant_id = r.tenant_id AND p.id = r.policy_id";

/// Resolves each day of `from..=to` against the employments that were in force,
/// ready for [`request_cost`].
///
/// A day no employment covers costs nothing: somebody cannot take leave from a
/// job they did not hold, and the create path refuses such a range outright
/// rather than silently charging zero for it.
///
/// A public holiday on the tenant's observed calendar costs nothing (B6.04):
/// somebody who books the week of 25 December spends four days, not five. A
/// tenant that observes no calendar passes [`TenantHolidays::none`] and the fold
/// is what it always was — the working pattern alone
/// (`docs/design/hr.md`, "Public holidays").
#[must_use]
pub fn leave_days(
    employments: &[Employment],
    from: Date,
    to: Date,
    holidays: &TenantHolidays,
) -> Vec<RequestedDay> {
    let public = holidays.days(from, to);
    let mut days = Vec::new();
    let mut day = from;
    while day <= to {
        let minutes = employments
            .iter()
            .find(|employment| employment.covers(day))
            .map_or(0, |employment| employment.minutes_on(day));
        days.push(RequestedDay {
            day,
            pattern_minutes: minutes,
            holiday: public.contains(&day),
            already_covered: false,
        });
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    days
}

/// What one request costs, folded from the employments in force over its days
/// and the tenant's public holidays.
#[must_use]
pub fn leave_request_cost(
    employments: &[Employment],
    from: Date,
    to: Date,
    holidays: &TenantHolidays,
) -> RequestCost {
    request_cost(&leave_days(employments, from, to, holidays))
}

/// Validates a range the way both the create and the edit paths need it.
fn validate_range(from: Date, to: Date) -> Result<()> {
    if to < from {
        return Err(StoreError::Validation(
            "leave must end on or after the day it starts".to_owned(),
        ));
    }
    if (to - from).whole_days() + 1 > REQUEST_MAX_DAYS {
        return Err(StoreError::Validation(format!(
            "leave must not cover more than {REQUEST_MAX_DAYS} days in one request"
        )));
    }
    Ok(())
}

/// Refuses a range that reaches outside the employment history, naming the
/// bound it crossed.
fn within_employment(employments: &[Employment], from: Date, to: Date) -> Result<()> {
    let Some(first) = employments.iter().map(|e| e.started_on).min() else {
        return Err(StoreError::Validation(
            "this person has no recorded terms of employment, so no leave can be booked against \
             them"
                .to_owned(),
        ));
    };
    if from < first {
        return Err(StoreError::Validation(format!(
            "leave cannot start before the employment does ({first})"
        )));
    }
    // An open period has no last day; a history of closed ones ends on the last
    // of them.
    if employments.iter().all(|e| e.ended_on.is_some()) {
        let last = employments.iter().filter_map(|e| e.ended_on).max();
        if let Some(last) = last.filter(|last| to > *last) {
            return Err(StoreError::Validation(format!(
                "leave cannot end after the employment does ({last})"
            )));
        }
    }
    Ok(())
}

impl TenantStore {
    /// Records a request for time off.
    ///
    /// The status it lands in is the **policy's** decision, not the caller's: a
    /// policy that requires approval produces `requested`, and one that does not
    /// (a sick policy a tenant records rather than decides) produces `approved`
    /// with the requester named as the decider — so the record does not pretend
    /// somebody decided.
    ///
    /// `today` is passed in rather than read from a clock, so the balance check
    /// an auto-approval makes is the same fold a test can pin.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the person or the policy is not this
    /// tenant's; [`StoreError::Validation`] on a bad range, a range outside the
    /// employment, a range that costs nothing, or an over-long note;
    /// [`StoreError::Conflict`] when another live request already covers days in
    /// the range, when the policy is archived, when the person has left, or when
    /// an auto-approval would take the balance below zero on a policy that does
    /// not allow it; [`StoreError::Db`] on failure.
    pub async fn create_hr_leave_request(
        &self,
        input: &NewLeaveRequest,
        requester: &UserId,
        today: Date,
    ) -> Result<HrLeaveRequestId> {
        let note = bounded("leave note", &input.note, LEAVE_NOTE_MAX_CHARS)?;
        validate_range(input.from_day, input.to_day)?;

        let employee = self
            .hr_employee(&input.employee_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if employee.is_archived() {
            return Err(StoreError::Conflict(
                "this person has left; leave cannot be booked for them".to_owned(),
            ));
        }
        let policy = self
            .hr_leave_policy(&input.policy_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if policy.is_archived() {
            return Err(StoreError::Conflict(
                "this leave policy has been retired; choose a live one".to_owned(),
            ));
        }

        let employments = self.hr_employments(&input.employee_id).await?;
        within_employment(&employments, input.from_day, input.to_day)?;
        // A year the holiday seed has not been reviewed for is refused rather
        // than folded as if the country had no holidays that year: "none" and
        // "not checked yet" must not look the same to a balance.
        let holidays = self.hr_holidays().await?;
        holidays.covers(input.from_day, input.to_day)?;
        let cost = leave_request_cost(&employments, input.from_day, input.to_day, &holidays);
        if cost.minutes == 0 {
            return Err(StoreError::Validation(
                "these days are not working days for this person, so the request costs nothing"
                    .to_owned(),
            ));
        }
        self.refuse_overlap(&input.employee_id, input.from_day, input.to_day, None)
            .await?;

        // A policy that needs no decision is approved as it is written, so the
        // balance it consumes is checked here rather than at a decision that
        // will never come.
        let approved = !policy.requires_approval;
        if approved && !policy.allow_negative {
            self.refuse_overdrawn(
                &input.employee_id,
                &input.policy_id,
                cost.minutes,
                input.from_day.max(today),
            )
            .await?;
        }

        let id = HrLeaveRequestId::generate();
        let status = if approved {
            LeaveStatus::Approved
        } else {
            LeaveStatus::Requested
        };
        sqlx::query(
            "INSERT INTO hr_leave_requests (tenant_id, id, employee_id, policy_id, from_day, \
                 to_day, status, note, requested_by, decided_by, decided_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 CASE WHEN $10 THEN $9 END, CASE WHEN $10 THEN now() END)",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(input.employee_id.as_str())
        .bind(input.policy_id.as_str())
        .bind(input.from_day)
        .bind(input.to_day)
        .bind(status.as_str())
        .bind(&note)
        .bind(requester.as_str())
        .bind(approved)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(id)
    }

    /// One request of this tenant, or `None` — including when the id belongs to
    /// another tenant, which is indistinguishable by design.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the stored row carries a word this build
    /// does not know; [`StoreError::Db`] on failure.
    pub async fn hr_leave_request(&self, id: &HrLeaveRequestId) -> Result<Option<LeaveRequest>> {
        let row = sqlx::query_as::<_, RequestRow>(&format!(
            "SELECT {REQUEST_COLS} {REQUEST_FROM} WHERE r.tenant_id = $1 AND r.id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let Some(row) = row else { return Ok(None) };
        let employments = self
            .hr_employments(&HrEmployeeId::new(row.employee_id.clone()))
            .await?;
        row.into_request(&employments, &self.hr_holidays().await?)
            .map(Some)
    }

    /// The requests a door asks for, newest first.
    ///
    /// `query.employees` is resolved by the caller — their own id, the ids of
    /// the people who report to them, or `None` for HR's tenant-wide read. The
    /// store answers exactly what it is asked for and never widens it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] on a stored word this build does not know;
    /// [`StoreError::Db`] on failure.
    pub async fn hr_leave_requests(&self, query: &LeaveRequestQuery) -> Result<Vec<LeaveRequest>> {
        // An empty employee list is "nobody", not "everybody": a manager with no
        // reports must get an empty queue rather than the whole tenant's leave.
        if query.employees.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        let employees: Option<Vec<String>> = query
            .employees
            .as_ref()
            .map(|ids| ids.iter().map(|id| id.as_str().to_owned()).collect());
        let statuses: Vec<String> = query
            .statuses
            .iter()
            .map(|status| status.as_str().to_owned())
            .collect();
        let rows = sqlx::query_as::<_, RequestRow>(&format!(
            "SELECT {REQUEST_COLS} {REQUEST_FROM} \
              WHERE r.tenant_id = $1 \
                AND ($2::text[] IS NULL OR r.employee_id = ANY($2)) \
                AND (cardinality($3::text[]) = 0 OR r.status = ANY($3)) \
                AND ($4::date IS NULL OR r.to_day >= $4) \
                AND ($5::date IS NULL OR r.from_day <= $5) \
              ORDER BY r.from_day DESC, r.id \
              LIMIT {LEAVE_REQUEST_PAGE_MAX}"
        ))
        .bind(self.tenant().as_str())
        .bind(employees)
        .bind(statuses)
        .bind(query.from)
        .bind(query.to)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        self.cost_rows(rows).await
    }

    /// Folds the cost of each row, reading every person's terms once however
    /// many requests they have in the list.
    async fn cost_rows(&self, rows: Vec<RequestRow>) -> Result<Vec<LeaveRequest>> {
        let holidays = self.hr_holidays().await?;
        let mut terms: std::collections::HashMap<String, Vec<Employment>> =
            std::collections::HashMap::new();
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let employments = match terms.get(&row.employee_id) {
                Some(known) => known.clone(),
                None => {
                    let read = self
                        .hr_employments(&HrEmployeeId::new(row.employee_id.clone()))
                        .await?;
                    terms.insert(row.employee_id.clone(), read.clone());
                    read
                }
            };
            out.push(row.into_request(&employments, &holidays)?);
        }
        Ok(out)
    }

    /// Changes the dates or the note of a request nobody has decided.
    ///
    /// Approved leave is not editable — it is cancelled and asked for again — so
    /// this refuses anything but `requested`, naming what the request actually
    /// is.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the request is not this tenant's;
    /// [`StoreError::Conflict`] when it has been decided or closed, or when the
    /// new dates collide with another live request; [`StoreError::Validation`]
    /// as for create; [`StoreError::Db`] on failure.
    pub async fn update_hr_leave_request(
        &self,
        id: &HrLeaveRequestId,
        from_day: Date,
        to_day: Date,
        note: &str,
    ) -> Result<()> {
        let note = bounded("leave note", note, LEAVE_NOTE_MAX_CHARS)?;
        validate_range(from_day, to_day)?;
        let stored = self
            .hr_leave_request(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if stored.status != LeaveStatus::Requested {
            return Err(StoreError::Conflict(format!(
                "leave that is {} cannot be edited; cancel it and ask again",
                stored.status
            )));
        }
        let employments = self.hr_employments(&stored.employee_id).await?;
        within_employment(&employments, from_day, to_day)?;
        let holidays = self.hr_holidays().await?;
        holidays.covers(from_day, to_day)?;
        if leave_request_cost(&employments, from_day, to_day, &holidays).minutes == 0 {
            return Err(StoreError::Validation(
                "these days are not working days for this person, so the request costs nothing"
                    .to_owned(),
            ));
        }
        self.refuse_overlap(&stored.employee_id, from_day, to_day, Some(id))
            .await?;
        sqlx::query(
            "UPDATE hr_leave_requests SET from_day = $3, to_day = $4, note = $5, \
                 updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND status = 'requested'",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(from_day)
        .bind(to_day)
        .bind(&note)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(())
    }

    /// Approves or rejects a request.
    ///
    /// **Who may is not decided here** — the reporting line and the roles are the
    /// door's to know. What this refuses is what the *record* forbids: deciding
    /// something already decided, withdrawn or cancelled, and approving an
    /// absence that would overdraw a policy which does not allow it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the request is not this tenant's;
    /// [`StoreError::Conflict`] when it is not awaiting a decision or would
    /// overdraw the balance; [`StoreError::Validation`] on an over-long note;
    /// [`StoreError::Db`] on failure.
    pub async fn decide_hr_leave_request(
        &self,
        id: &HrLeaveRequestId,
        approve: bool,
        decider: &UserId,
        note: &str,
        today: Date,
    ) -> Result<()> {
        let note = bounded("decision note", note, LEAVE_NOTE_MAX_CHARS)?;
        let stored = self
            .hr_leave_request(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if stored.status != LeaveStatus::Requested {
            return Err(StoreError::Conflict(format!(
                "this leave is already {}",
                stored.status
            )));
        }
        if approve {
            let policy = self
                .hr_leave_policy(&stored.policy_id)
                .await?
                .ok_or(StoreError::NotFound)?;
            if !policy.allow_negative {
                self.refuse_overdrawn(
                    &stored.employee_id,
                    &stored.policy_id,
                    stored.cost.minutes,
                    stored.from_day.max(today),
                )
                .await?;
            }
        }
        let status = if approve {
            LeaveStatus::Approved
        } else {
            LeaveStatus::Rejected
        };
        let done = sqlx::query(
            "UPDATE hr_leave_requests \
                SET status = $3, decided_by = $4, decided_at = now(), decision_note = $5, \
                    updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND status = 'requested'",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(status.as_str())
        .bind(decider.as_str())
        .bind(&note)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        // Zero rows means somebody decided it between the read and the write.
        if done.rows_affected() == 0 {
            return Err(StoreError::Conflict(
                "this leave was decided by somebody else a moment ago".to_owned(),
            ));
        }
        Ok(())
    }

    /// Takes back a request nobody has decided.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the request is not this tenant's;
    /// [`StoreError::Conflict`] when it has already been decided or closed;
    /// [`StoreError::Db`] on failure.
    pub async fn withdraw_hr_leave_request(
        &self,
        id: &HrLeaveRequestId,
        actor: &UserId,
    ) -> Result<()> {
        self.close(id, LeaveStatus::Withdrawn, actor, "requested")
            .await
    }

    /// Cancels approved leave that has not started, giving the balance back.
    ///
    /// Leave already begun is not cancellable: the fact that somebody was absent
    /// last Tuesday is amended by HR through an audited correction, never erased
    /// by the person who was away.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the request is not this tenant's;
    /// [`StoreError::Conflict`] when it is not approved, or has already started;
    /// [`StoreError::Db`] on failure.
    pub async fn cancel_hr_leave_request(
        &self,
        id: &HrLeaveRequestId,
        actor: &UserId,
        today: Date,
    ) -> Result<()> {
        let stored = self
            .hr_leave_request(id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if stored.status != LeaveStatus::Approved {
            return Err(StoreError::Conflict(format!(
                "only approved leave can be cancelled; this leave is {}",
                stored.status
            )));
        }
        if stored.has_started(today) {
            return Err(StoreError::Conflict(format!(
                "this leave started on {} and can only be corrected by HR",
                stored.from_day
            )));
        }
        self.close(id, LeaveStatus::Cancelled, actor, "approved")
            .await
    }

    /// The shared tail of withdraw and cancel: the row moves out of `expected`
    /// and records who closed it.
    async fn close(
        &self,
        id: &HrLeaveRequestId,
        status: LeaveStatus,
        actor: &UserId,
        expected: &str,
    ) -> Result<()> {
        let done = sqlx::query(
            "UPDATE hr_leave_requests \
                SET status = $3, closed_by = $4, closed_at = now(), updated_at = now() \
              WHERE tenant_id = $1 AND id = $2 AND status = $5",
        )
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(status.as_str())
        .bind(actor.as_str())
        .bind(expected)
        .execute(self.pool())
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            // Either it is not this tenant's, or it is not in the state the verb
            // needs. Both answers are the same shape as elsewhere in the suite:
            // a missing record is `NotFound`, a wrong state is `Conflict`.
            return match self.hr_leave_request(id).await? {
                None => Err(StoreError::NotFound),
                Some(current) => Err(StoreError::Conflict(format!(
                    "this leave is {}",
                    current.status
                ))),
            };
        }
        Ok(())
    }

    /// Refuses a range that days of another live request already cover, naming
    /// the request that covers them.
    ///
    /// `exclude` is the request being edited, which must not collide with
    /// itself.
    async fn refuse_overlap(
        &self,
        employee: &HrEmployeeId,
        from: Date,
        to: Date,
        exclude: Option<&HrLeaveRequestId>,
    ) -> Result<()> {
        let clash: Option<(String, Date, Date, String)> = sqlx::query_as(
            "SELECT id, from_day, to_day, status FROM hr_leave_requests \
              WHERE tenant_id = $1 AND employee_id = $2 \
                AND status IN ('requested', 'approved') \
                AND from_day <= $4 AND to_day >= $3 \
                AND ($5::text IS NULL OR id <> $5) \
              ORDER BY from_day LIMIT 1",
        )
        .bind(self.tenant().as_str())
        .bind(employee.as_str())
        .bind(from)
        .bind(to)
        .bind(exclude.map(|id| id.as_str().to_owned()))
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match clash {
            None => Ok(()),
            Some((_, clash_from, clash_to, status)) => Err(StoreError::Conflict(format!(
                "leave from {clash_from} to {clash_to} is already {status} for these days"
            ))),
        }
    }

    /// Refuses an approval that would take the balance below zero.
    ///
    /// The balance is read **as of the day the leave starts** (or today, if it
    /// has already started), because a monthly accrual that has not arrived yet
    /// is not a balance somebody can spend — approving March's leave in January
    /// against December's accrual is how a company ends the year owing days it
    /// never granted.
    async fn refuse_overdrawn(
        &self,
        employee: &HrEmployeeId,
        policy: &HrLeavePolicyId,
        cost_minutes: i64,
        as_of: Date,
    ) -> Result<()> {
        let balance = self.hr_leave_balance(employee, policy, as_of).await?;
        let short = cost_minutes - balance.remaining_minutes;
        if short > 0 {
            return Err(StoreError::Conflict(format!(
                "this leave is {short} minutes more than the balance allows on {as_of}"
            )));
        }
        Ok(())
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
pub(crate) struct RequestRow {
    id: String,
    employee_id: String,
    preferred_name: String,
    given_name: String,
    family_name: String,
    policy_id: String,
    policy_name: String,
    from_day: Date,
    to_day: Date,
    status: String,
    note: String,
    requested_by: String,
    decided_by: Option<String>,
    decided_at: Option<OffsetDateTime>,
    decision_note: String,
    closed_by: Option<String>,
    closed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl RequestRow {
    /// Fallible on purpose: a stored status this build does not know is a schema
    /// disagreement, and answering with a guessed state would be worse than
    /// answering with an error.
    fn into_request(
        self,
        employments: &[Employment],
        holidays: &TenantHolidays,
    ) -> Result<LeaveRequest> {
        let cost = leave_request_cost(employments, self.from_day, self.to_day, holidays);
        Ok(LeaveRequest {
            id: HrLeaveRequestId::new(self.id),
            employee_id: HrEmployeeId::new(self.employee_id),
            employee_name: display_name(&self.preferred_name, &self.given_name, &self.family_name),
            policy_id: HrLeavePolicyId::new(self.policy_id),
            policy_name: self.policy_name,
            from_day: self.from_day,
            to_day: self.to_day,
            status: LeaveStatus::parse(&self.status)?,
            note: self.note,
            requested_by: self.requested_by,
            decided_by: self.decided_by,
            decided_at: self.decided_at,
            decision_note: self.decision_note,
            closed_by: self.closed_by,
            closed_at: self.closed_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            cost,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::hr_employments::FULL_TIME_PATTERN;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real date")
    }

    fn employment(from: Date, to: Option<Date>, pattern: [i32; 7]) -> Employment {
        Employment {
            id: crate::id::HrEmploymentId::new("emp"),
            employee_id: HrEmployeeId::new("person"),
            job_title: String::new(),
            team: String::new(),
            contract_kind: crate::hr_employments::ContractKind::Permanent,
            started_on: from,
            ended_on: to,
            pattern_minutes: pattern,
            pay_amount_cents: None,
            pay_period: crate::hr_employments::PayPeriod::Month,
            pay_currency: "EUR".to_owned(),
            created_by: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_vocabulary_is_closed_and_round_trips() {
        for status in [
            LeaveStatus::Requested,
            LeaveStatus::Approved,
            LeaveStatus::Rejected,
            LeaveStatus::Withdrawn,
            LeaveStatus::Cancelled,
        ] {
            assert_eq!(LeaveStatus::parse(status.as_str()).unwrap(), status);
            assert_eq!(status.to_string(), status.as_str());
        }
        assert!(LeaveStatus::parse("taken").is_err(), "taken is derived");
        assert!(LeaveStatus::Requested.is_live());
        assert!(LeaveStatus::Approved.is_live());
        assert!(!LeaveStatus::Cancelled.is_live());
    }

    /// A week off a full-time pattern costs five days; the weekend inside it
    /// costs nothing, and the fold is additive over sub-ranges.
    #[test]
    fn a_week_costs_the_working_days_inside_it() {
        // 2026-03-02 is a Monday.
        let monday = day(2026, Month::March, 2);
        let sunday = day(2026, Month::March, 8);
        let terms = vec![employment(
            day(2020, Month::January, 1),
            None,
            FULL_TIME_PATTERN,
        )];
        let cost = leave_request_cost(&terms, monday, sunday, &TenantHolidays::none());
        assert_eq!(cost.minutes, 5 * 480);
        assert_eq!(cost.working_days, 5);

        let mut summed = 0;
        let mut one = monday;
        while one <= sunday {
            summed += leave_request_cost(&terms, one, one, &TenantHolidays::none()).minutes;
            one = one.next_day().unwrap();
        }
        assert_eq!(summed, cost.minutes, "booked at once or day by day");
    }

    /// The week of Christmas costs four days on a Belgian calendar and five on
    /// none — the whole reason the calendars exist (B6.04).
    #[test]
    fn a_public_holiday_inside_the_range_costs_nothing() {
        // Monday 21 to Friday 25 December 2026; the 25th is Christmas Day.
        let monday = day(2026, Month::December, 21);
        let friday = day(2026, Month::December, 25);
        let terms = vec![employment(
            day(2020, Month::January, 1),
            None,
            FULL_TIME_PATTERN,
        )];
        let observed = TenantHolidays::for_calendar("BE");
        let cost = leave_request_cost(&terms, monday, friday, &observed);
        assert_eq!(cost.minutes, 4 * 480);
        assert_eq!(cost.working_days, 4);
        assert_eq!(cost.holiday_minutes, 480);
        // Booked day by day, the total is the same: the fold stays additive with
        // holidays in it.
        let mut summed = 0;
        let mut one = monday;
        while one <= friday {
            summed += leave_request_cost(&terms, one, one, &observed).minutes;
            one = one.next_day().unwrap();
        }
        assert_eq!(summed, cost.minutes);
        // A tenant observing nothing is charged the full week, as before B6.04.
        assert_eq!(
            leave_request_cost(&terms, monday, friday, &TenantHolidays::none()).minutes,
            5 * 480
        );
    }

    /// A request spanning a change of terms is folded day by day, so the Friday
    /// after a move to a four-day week costs nothing.
    #[test]
    fn a_range_spanning_a_change_of_terms_folds_each_side() {
        let terms = vec![
            employment(
                day(2020, Month::January, 1),
                Some(day(2026, Month::March, 4)),
                FULL_TIME_PATTERN,
            ),
            employment(
                day(2026, Month::March, 5),
                None,
                [480, 480, 480, 480, 0, 0, 0],
            ),
        ];
        // Monday 2 March to Friday 6 March: three full days, a Thursday on the
        // new terms, and a Friday that is no longer a working day.
        let cost = leave_request_cost(
            &terms,
            day(2026, Month::March, 2),
            day(2026, Month::March, 6),
            &TenantHolidays::none(),
        );
        assert_eq!(cost.minutes, 4 * 480);
        assert_eq!(cost.working_days, 4);
    }

    /// Days outside the employment cost nothing — and the create path refuses
    /// such a range outright rather than charging zero for it.
    #[test]
    fn leave_cannot_reach_outside_the_employment() {
        let terms = vec![employment(
            day(2026, Month::February, 2),
            Some(day(2026, Month::April, 30)),
            FULL_TIME_PATTERN,
        )];
        assert!(
            within_employment(
                &terms,
                day(2026, Month::February, 2),
                day(2026, Month::April, 30)
            )
            .is_ok()
        );
        let early = within_employment(
            &terms,
            day(2026, Month::January, 5),
            day(2026, Month::February, 6),
        );
        assert!(format!("{:?}", early.unwrap_err()).contains("2026-02-02"));
        let late = within_employment(
            &terms,
            day(2026, Month::April, 27),
            day(2026, Month::May, 8),
        );
        assert!(format!("{:?}", late.unwrap_err()).contains("2026-04-30"));
        // An open period has no end to reach past.
        let open = vec![employment(
            day(2026, Month::February, 2),
            None,
            FULL_TIME_PATTERN,
        )];
        assert!(
            within_employment(&open, day(2027, Month::June, 1), day(2027, Month::June, 5)).is_ok()
        );
        // Nobody with no terms at all can book anything.
        assert!(
            within_employment(&[], day(2026, Month::March, 2), day(2026, Month::March, 3)).is_err()
        );
    }

    #[test]
    fn a_range_must_end_after_it_starts_and_stay_inside_a_year() {
        assert!(validate_range(day(2026, Month::March, 2), day(2026, Month::March, 2)).is_ok());
        assert!(validate_range(day(2026, Month::March, 3), day(2026, Month::March, 2)).is_err());
        assert!(
            validate_range(day(2026, Month::January, 1), day(2026, Month::December, 31)).is_ok()
        );
        assert!(
            validate_range(day(2026, Month::January, 1), day(2027, Month::January, 2)).is_err()
        );
    }
}
