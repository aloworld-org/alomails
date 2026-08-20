//! The week a person hands in, the decision an admin makes about it, and the
//! **lock** both of those put on the hours inside it (alo Projects, ADR 0035,
//! wave B3.05).
//!
//! [`crate::time_entries`] holds what was worked. This holds whether that week
//! has been handed in, what was decided, and — the reason the table exists —
//! whether its hours may still move.
//!
//! # Two doors, and why they address the same week differently
//!
//! The **personal door** ([`AccountStore`]) submits and withdraws, and addresses
//! a week by its **Monday**: a week nobody has ever submitted has no row and
//! therefore no id, so the Monday is the only thing that can name it.
//!
//! The **tenant door** ([`TenantStore`]) reads the inbox and decides, and
//! addresses a week by its **id**. An approver is always looking at a row that
//! already exists, and naming a colleague's week as (person, date) in a URL
//! would put an employee's identity in every access log between here and the
//! browser. The gate on that door is `Account::require_admin` at the edge — the
//! decision recorded in `docs/design/projects.md` § "Who approves, in B3", where
//! deriving a manager from a project owner is rejected outright: a timesheet is
//! a *person's* week and spans several projects, so a per-project owner cannot
//! approve it. When B6.02 brings the org chart, the approver check widens
//! additively and nothing already approved moves.
//!
//! # A week with no row is open
//!
//! Most weeks are never submitted at all, and a row per person per week since
//! the start of an engagement would be a table of nothing happening. `open` is
//! therefore both "no row" and a stored status, and the two mean the same thing:
//! a week submitted and withdrawn is open exactly as one that never was. Every
//! read here returns `Option`, and absence is the answer rather than an error.
//!
//! # The lock is this row, not a flag on the entry
//!
//! [`require_week_unlocked`] is called by **every** write of an hour — the
//! manual entry, the timer's stop, the correction, the deletion, the acceptance
//! of a proposal — and a correction that moves an entry to another day checks
//! **both** weeks, because otherwise a locked week can be drained one entry at a
//! time. A `locked` boolean on the entry was rejected: it is two places to be
//! right, and reopening a week would have to rewrite every row it contains.
//!
//! # What is not locked, and why
//!
//! Rejecting a proposal ([`AccountStore::reject_time_entry`]) stays legal in a
//! locked week. A proposal is in no total (ADR 0023), so removing one changes
//! nothing anybody approved — and since *creating* one is refused while the week
//! is locked, a proposal can only be found there because the week was locked
//! after it was drafted. Refusing the rejection too would leave it stuck with no
//! way to clear it.
//!
//! # A person's week is personal data
//!
//! The submit binds `user_id` from the account door and there is no function
//! here that takes somebody else's, exactly as in [`crate::time_entries`]. The
//! inbox and the decision cross that line by design and only behind the admin
//! gate; a decision note can name a person or a case, so it never reaches a log.

use std::collections::HashMap;

use sqlx::PgConnection;
use time::{Date, Duration, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::error::{Result, StoreError};
use crate::id::{TimeWeekId, UserId};
use crate::store::TenantStore;

/// Longest reason an approver may attach to a decision. A sentence about why,
/// not a performance review.
pub const DECISION_NOTE_MAX: usize = 500;

/// Days in a week — named because the closing Sunday is derived from the Monday
/// in three places and none of them should spell `6` on their own.
const DAYS_IN_WEEK: i64 = 7;

/// Most weeks one read of the inbox or the caller's own list returns.
const WEEKS_LIMIT: i64 = 500;

/// The columns every read selects, in `WeekRow` order.
const WEEK_COLS: &str = "id, user_id, week_start, status, submitted_at, decided_by, decided_at, \
     decision_note, created_at, updated_at";

/// Where one person's week is in its life.
///
/// ```text
/// (no row) ──submit──> submitted ──approve──> approved ──reopen──> open
///     │                    │                                        │
///     └────────────────────┴──withdraw/reject──> open / rejected ────┘
///                                     (both unlocked, both editable)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeekStatus {
    /// Nothing is pending: never handed in, taken back, or an approval reopened.
    /// The hours are the person's own to change.
    Open,
    /// Handed in and awaiting a decision. **Locked** — the person has said "this
    /// is my week", and an hour that moves under an approver is a timesheet
    /// nobody can rely on.
    Submitted,
    /// Decided yes. **Locked**, and the state the billable handoff (B3.06)
    /// requires before an hour may reach an invoice.
    Approved,
    /// Decided no, with a reason. Unlocked on purpose: the point of a rejection
    /// is that the person fixes the week and submits it again.
    Rejected,
}

impl WeekStatus {
    /// The value stored in the `status` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    /// Parses a stored status, or `None` if it is not one we know.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "submitted" => Some(Self::Submitted),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }

    /// Whether the hours of a week in this state refuse to move.
    ///
    /// **The whole lock is this function.** Submitted and approved are frozen;
    /// open and rejected are the person's own to change.
    #[must_use]
    pub fn is_locked(self) -> bool {
        matches!(self, Self::Submitted | Self::Approved)
    }

    /// Whether a person may hand in a week in this state. A resubmit after a
    /// rejection is the ordinary path; submitting one that is already in
    /// somebody's inbox, or already approved, is a caller that has lost track.
    #[must_use]
    pub fn can_submit(self) -> bool {
        matches!(self, Self::Open | Self::Rejected)
    }

    /// Whether an approver may still decide this week — only one that is waiting
    /// for them. Deciding an already-decided week is a `reopen` first.
    #[must_use]
    pub fn can_decide(self) -> bool {
        matches!(self, Self::Submitted)
    }

    /// Whether an approver may take a decision back. Only a decided week has a
    /// decision to undo; a submitted one is withdrawn by its owner.
    #[must_use]
    pub fn can_reopen(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }
}

/// What an approver decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeekDecision {
    /// Yes. The week stays locked, and its billable hours become eligible for a
    /// draft invoice (B3.06).
    Approve,
    /// No, with a reason. The week unlocks so the person can correct it.
    Reject,
}

impl WeekDecision {
    /// The status a week reaches when this decision is recorded.
    #[must_use]
    pub fn resulting_status(self) -> WeekStatus {
        match self {
            Self::Approve => WeekStatus::Approved,
            Self::Reject => WeekStatus::Rejected,
        }
    }
}

/// One person's week, as stored.
#[derive(Debug, Clone)]
pub struct TimesheetWeek {
    /// Opaque id, unique within the tenant — how the admin door addresses it.
    pub id: TimeWeekId,
    /// Whose week.
    pub user_id: UserId,
    /// The Monday it starts on.
    pub week_start: Date,
    /// Where it is in its life.
    pub status: WeekStatus,
    /// When it was handed in. Kept through a decision, cleared when the week
    /// goes back to open.
    pub submitted_at: Option<OffsetDateTime>,
    /// The admin who decided, while that decision stands.
    pub decided_by: Option<UserId>,
    /// When they decided.
    pub decided_at: Option<OffsetDateTime>,
    /// What they said about it — usually why a week was rejected. Never logged.
    pub decision_note: String,
    /// When the row first appeared (the first submit).
    pub created_at: OffsetDateTime,
    /// When it last changed.
    pub updated_at: OffsetDateTime,
}

impl TimesheetWeek {
    /// The Sunday this week ends on, both ends inclusive — the other half of
    /// every period this module reads.
    #[must_use]
    pub fn week_end(&self) -> Date {
        week_end(self.week_start)
    }
}

/// One week waiting in the approvals inbox: the row, who it belongs to, and
/// what it adds up to.
///
/// The totals are computed **at read time** from the entries, never stored: an
/// approver must see the week as it is now, and a cached total is a number that
/// can disagree with the hours it claims to describe.
#[derive(Debug, Clone)]
pub struct PendingWeek {
    /// The week itself.
    pub week: TimesheetWeek,
    /// The submitter's address — what an inbox shows instead of an opaque id.
    /// Empty when the user record has since been removed.
    pub user_email: String,
    /// Every real (non-proposed) minute in the week.
    pub minutes: i64,
    /// The subset of those that is chargeable to a customer.
    pub billable_minutes: i64,
    /// Privacy-safe project totals that let an approver understand what the
    /// submitted week contains without exposing individual entry notes.
    pub projects: Vec<PendingProjectHours>,
}

/// One project's contribution to a submitted week.
#[derive(Debug, Clone)]
pub struct PendingProjectHours {
    pub project_id: String,
    pub project_name: String,
    pub minutes: i64,
    pub billable_minutes: i64,
}

/// The Monday of the week `day` falls in.
///
/// ISO 8601 week-numbering weeks, Monday-start — the same convention
/// `insight_series` buckets by, and the one every European timesheet uses. Pure
/// and total: `number_days_from_monday` is 0…6, so the subtraction cannot leave
/// the calendar for any date this system can hold.
#[must_use]
pub fn week_start(day: Date) -> Date {
    let back = i64::from(day.weekday().number_days_from_monday());
    day.saturating_sub(Duration::days(back))
}

/// The Sunday closing the week that starts on `monday`, both ends inclusive.
#[must_use]
pub fn week_end(monday: Date) -> Date {
    monday.saturating_add(Duration::days(DAYS_IN_WEEK - 1))
}

/// Confirms a caller-supplied day really is a Monday.
///
/// The personal door addresses a week by its Monday, so a request naming a
/// Wednesday is ambiguous — it could mean "the week containing this day", which
/// is a *rounding* of somebody's intent, and a timesheet that silently submits a
/// different week than the one asked for is the single worst bug this module
/// could ship. Refused, naming the Monday that was probably meant.
///
/// # Errors
/// [`StoreError::Validation`] when `day` is not a Monday.
pub fn require_monday(day: Date) -> Result<Date> {
    if day.weekday() != time::Weekday::Monday {
        return Err(StoreError::Validation(format!(
            "a week is addressed by its Monday; {day} is a {:?} — did you mean {}?",
            day.weekday(),
            week_start(day)
        )));
    }
    Ok(day)
}

/// Refuses to write, move or remove an hour in a week that has been handed in or
/// approved — **the lock**, called by every write in [`crate::time_entries`] and
/// [`crate::time_timer`].
///
/// Takes the day the hour belongs to (not a Monday) and resolves the week
/// itself, so no caller can disagree about a week boundary.
///
/// The read is not row-locked against a simultaneous submit, and deliberately
/// so: a week's totals are always recomputed from its entries, so no total is
/// ever wrong. The only reachable outcome of that race is an hour landing in a
/// week in the same instant it was handed in — which the approver still sees,
/// because the inbox counts the entries as they are when it is read.
///
/// # Errors
/// [`StoreError::Conflict`] naming the week when it is submitted or approved;
/// [`StoreError::Db`] on failure.
pub(crate) async fn require_week_unlocked(
    conn: &mut PgConnection,
    tenant: &str,
    user: &str,
    day: Date,
) -> Result<()> {
    let monday = week_start(day);
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT status FROM time_weeks \
         WHERE tenant_id = $1 AND user_id = $2 AND week_start = $3",
    )
    .bind(tenant)
    .bind(user)
    .bind(monday)
    .fetch_optional(conn)
    .await
    .map_err(StoreError::Db)?;
    // An unknown status string is treated as unlocked rather than as a refusal:
    // it can only come from a future migration this binary predates, and
    // freezing a person's timesheet on a value we do not understand is the worse
    // of the two failures. The CHECK constraint makes it unreachable anyway.
    let Some(status) = row.and_then(|(value,)| WeekStatus::parse(&value)) else {
        return Ok(());
    };
    if status.is_locked() {
        return Err(StoreError::Conflict(format!(
            "the week of {monday} is {} and its hours are locked; withdraw it or ask an \
             approver to reopen it",
            status.as_str()
        )));
    }
    Ok(())
}

impl AccountStore {
    /// The caller's **own** week, or `None` when they have never submitted it —
    /// which is what open means.
    ///
    /// A colleague's week is not addressable through this door: there is no
    /// argument for one.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `week_start` is not a Monday;
    /// [`StoreError::Db`] on failure.
    pub async fn timesheet_week(&self, week_start: Date) -> Result<Option<TimesheetWeek>> {
        let monday = require_monday(week_start)?;
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "SELECT {WEEK_COLS} FROM time_weeks \
             WHERE tenant_id = $1 AND user_id = $2 AND week_start = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(monday)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(WeekRow::into_week))
    }

    /// The caller's own weeks that have a status, between `from` and `to`
    /// inclusive, oldest first.
    ///
    /// Only weeks with a row come back. A week the list does not mention is
    /// open, which is the answer and not a gap: the alternative — synthesising a
    /// row for every Monday in the period — would invent records that do not
    /// exist and would have to invent ids for them too.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn timesheet_weeks(&self, from: Date, to: Date) -> Result<Vec<TimesheetWeek>> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, WeekRow>(&format!(
            "SELECT {WEEK_COLS} FROM time_weeks \
             WHERE tenant_id = $1 AND user_id = $2 AND week_start >= $3 AND week_start <= $4 \
             ORDER BY week_start LIMIT $5"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(from)
        .bind(to)
        .bind(WEEKS_LIMIT)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(WeekRow::into_week).collect())
    }

    /// Hands the caller's own week in for approval, **locking its hours**.
    ///
    /// One statement, not a check-then-write: the unique constraint on
    /// `(tenant, user, week)` is what makes the upsert safe, and its `WHERE`
    /// clause is the state machine — a week that is already submitted or already
    /// approved updates nothing and is read back to name what it actually is. A
    /// resubmit after a rejection clears the old decision, because a decision
    /// that no longer stands must not still be displayed on the record; the
    /// history of it is in the audit log, which is what an append-only log is
    /// for.
    ///
    /// An empty week may be submitted. "I worked nothing this week" is a real
    /// statement, and refusing it would leave a person with no way to say it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `week_start` is not a Monday;
    /// [`StoreError::Conflict`] when the week is already submitted or approved;
    /// [`StoreError::Db`] on failure.
    pub async fn submit_week(&self, week_start: Date) -> Result<TimesheetWeek> {
        let monday = require_monday(week_start)?;
        let id = TimeWeekId::generate();
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "INSERT INTO time_weeks (tenant_id, id, user_id, week_start, status, submitted_at) \
             VALUES ($1, $2, $3, $4, 'submitted', now()) \
             ON CONFLICT (tenant_id, user_id, week_start) DO UPDATE \
                 SET status = 'submitted', submitted_at = now(), decided_by = NULL, \
                     decided_at = NULL, decision_note = '', updated_at = now() \
                 WHERE time_weeks.status IN ('open', 'rejected') \
             RETURNING {WEEK_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(monday)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(row.into_week()),
            None => Err(self.week_conflict(monday, "submitted").await),
        }
    }

    /// Takes the caller's own submitted week back, unlocking its hours.
    ///
    /// Only a week nobody has decided yet. An approved week is not the person's
    /// to reopen — the hours may already be on a document — and a rejected or
    /// open one is unlocked already, so there is nothing to withdraw.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when `week_start` is not a Monday;
    /// [`StoreError::Conflict`] when the week is not currently submitted;
    /// [`StoreError::Db`] on failure.
    pub async fn withdraw_week(&self, week_start: Date) -> Result<TimesheetWeek> {
        let monday = require_monday(week_start)?;
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "UPDATE time_weeks SET status = 'open', submitted_at = NULL, updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND week_start = $3 AND status = 'submitted' \
             RETURNING {WEEK_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(monday)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(row.into_week()),
            None => Err(self.week_conflict(monday, "withdrawn").await),
        }
    }

    /// Names, in a refusal, what the week actually is — read after a statement
    /// declined to move it.
    ///
    /// A second read rather than a guess: "already submitted" and "already
    /// approved" want different answers from the person reading them, and
    /// `open` (no row at all) is the third. A read that itself fails degrades to
    /// the plain refusal instead of turning a `409` into a `500`.
    async fn week_conflict(&self, monday: Date, verb: &str) -> StoreError {
        let status = match self.timesheet_week(monday).await {
            Ok(Some(week)) => week.status,
            Ok(None) => WeekStatus::Open,
            Err(error) => return error,
        };
        StoreError::Conflict(format!(
            "the week of {monday} is {} and cannot be {verb}",
            status.as_str()
        ))
    }
}

impl TenantStore {
    /// Every week of this tenant awaiting a decision, oldest submission first —
    /// the approvals inbox.
    ///
    /// **Admin only**, gated at the edge by `Account::require_admin`. It crosses
    /// the personal-data line the account door exists to hold, so it is
    /// deliberately the narrowest cross-user read the module has: submitted
    /// weeks, their owners' addresses, and their minute totals. No notes, no
    /// entries, nothing about what anybody actually did.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn pending_weeks(&self) -> Result<Vec<PendingWeek>> {
        let rows = sqlx::query_as::<_, PendingRow>(&format!(
            "SELECT {}, \
                    COALESCE(u.email, '') AS user_email, \
                    COALESCE(t.minutes, 0)::bigint AS minutes, \
                    COALESCE(t.billable_minutes, 0)::bigint AS billable_minutes \
             FROM time_weeks w \
             LEFT JOIN users u ON u.tenant_id = w.tenant_id AND u.id = w.user_id \
             LEFT JOIN LATERAL ( \
                 SELECT SUM(e.minutes) AS minutes, \
                        SUM(e.minutes) FILTER (WHERE e.billable) AS billable_minutes \
                 FROM time_entries e \
                 WHERE e.tenant_id = w.tenant_id AND e.user_id = w.user_id \
                   AND e.state = 'active' \
                   AND e.work_date >= w.week_start \
                   AND e.work_date < w.week_start + {DAYS_IN_WEEK} \
             ) t ON true \
             WHERE w.tenant_id = $1 AND w.status = 'submitted' \
             ORDER BY w.submitted_at, w.id LIMIT $2",
            week_cols_prefixed("w")
        ))
        .bind(self.tenant().as_str())
        .bind(WEEKS_LIMIT)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut weeks = rows
            .into_iter()
            .map(PendingRow::into_pending)
            .collect::<Vec<_>>();
        if weeks.is_empty() {
            return Ok(weeks);
        }

        let week_ids = weeks
            .iter()
            .map(|pending| pending.week.id.as_str().to_owned())
            .collect::<Vec<_>>();
        let project_rows = sqlx::query_as::<_, PendingProjectRow>(
            "SELECT w.id AS week_id, e.project_id, p.name AS project_name, \
                    SUM(e.minutes)::bigint AS minutes, \
                    COALESCE(SUM(e.minutes) FILTER (WHERE e.billable), 0)::bigint AS billable_minutes \
             FROM time_weeks w \
             JOIN time_entries e ON e.tenant_id = w.tenant_id AND e.user_id = w.user_id \
                AND e.state = 'active' AND e.work_date >= w.week_start \
                AND e.work_date < w.week_start + 7 \
             JOIN task_projects p ON p.tenant_id = e.tenant_id AND p.id = e.project_id \
             WHERE w.tenant_id = $1 AND w.id = ANY($2) \
             GROUP BY w.id, e.project_id, p.name \
             ORDER BY p.name, e.project_id",
        )
        .bind(self.tenant().as_str())
        .bind(&week_ids)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let mut projects_by_week = HashMap::<String, Vec<PendingProjectHours>>::new();
        for row in project_rows {
            let week_id = row.week_id.clone();
            projects_by_week
                .entry(week_id)
                .or_default()
                .push(row.into_project());
        }
        for pending in &mut weeks {
            pending.projects = projects_by_week
                .remove(pending.week.id.as_str())
                .unwrap_or_default();
        }
        Ok(weeks)
    }

    /// One of this tenant's weeks by id, whoever it belongs to — **admin only**,
    /// the read behind the decision routes.
    ///
    /// Another tenant's id is `None`, exactly like one that was never issued:
    /// there is no existence oracle here.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn week_by_id(&self, id: &TimeWeekId) -> Result<Option<TimesheetWeek>> {
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "SELECT {WEEK_COLS} FROM time_weeks WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(WeekRow::into_week))
    }

    /// Records an approver's decision on a submitted week — **admin only**.
    ///
    /// An approved week stays locked (that is what approval means, and B3.06
    /// requires it before an hour may reach an invoice); a rejected one unlocks,
    /// because the point of a rejection is that the person fixes it and submits
    /// again. `decided_by` is the acting admin, taken from the authenticated
    /// caller and never from request input.
    ///
    /// An admin may decide their own week: a one-person tenant has nobody else,
    /// and the entry the audit trail writes records who it was.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the week is not this tenant's;
    /// [`StoreError::Conflict`] when it is not awaiting a decision;
    /// [`StoreError::Validation`] when the note is too long;
    /// [`StoreError::Db`] on failure.
    pub async fn decide_week(
        &self,
        id: &TimeWeekId,
        decision: WeekDecision,
        approver: &UserId,
        note: &str,
    ) -> Result<TimesheetWeek> {
        let note = bounded("decision note", note, DECISION_NOTE_MAX)?;
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "UPDATE time_weeks \
                SET status = $3, decided_by = $4, decided_at = now(), decision_note = $5, \
                    updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status = 'submitted' \
             RETURNING {WEEK_COLS}"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .bind(decision.resulting_status().as_str())
        .bind(approver.as_str())
        .bind(note)
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(row.into_week()),
            None => Err(self.decision_refusal(id, "decided").await),
        }
    }

    /// Takes a decision back: a decided week returns to open and its hours
    /// unlock — **admin only**.
    ///
    /// **Reopening an approved week whose hours are already on a document is a
    /// refusal**, naming how many and which invoice. The hours have left this
    /// module and are on paper a customer has read; the way back is to void or
    /// credit that document (B1's own verbs), not to edit history underneath it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the week is not this tenant's;
    /// [`StoreError::Conflict`] when it has not been decided, or carries billed
    /// hours; [`StoreError::Db`] on failure.
    pub async fn reopen_week(&self, id: &TimeWeekId) -> Result<TimesheetWeek> {
        let Some(week) = self.week_by_id(id).await? else {
            return Err(StoreError::NotFound);
        };
        if !week.status.can_reopen() {
            return Err(StoreError::Conflict(format!(
                "the week of {} is {} and has no decision to take back",
                week.week_start,
                week.status.as_str()
            )));
        }
        if let Some(billed) = self.billed_in_week(&week).await? {
            return Err(StoreError::Conflict(billed.refusal()));
        }
        let row = sqlx::query_as::<_, WeekRow>(&format!(
            "UPDATE time_weeks \
                SET status = 'open', submitted_at = NULL, decided_by = NULL, decided_at = NULL, \
                    decision_note = '', updated_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND status IN ('approved', 'rejected') \
             RETURNING {WEEK_COLS}"
        ))
        .bind(self.tenant().as_str())
        .bind(id.as_str())
        .fetch_optional(self.pool())
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(row.into_week()),
            None => Err(self.decision_refusal(id, "reopened").await),
        }
    }

    /// The hours of `week` that are already on a document, if any.
    async fn billed_in_week(&self, week: &TimesheetWeek) -> Result<Option<BilledHours>> {
        let row = sqlx::query_as::<_, (i64, Option<String>)>(
            "SELECT COUNT(*), MIN(i.number) FROM time_entries e \
             LEFT JOIN billing_invoices i \
                    ON i.tenant_id = e.tenant_id AND i.id = e.invoice_id \
             WHERE e.tenant_id = $1 AND e.user_id = $2 \
               AND e.work_date >= $3 AND e.work_date <= $4 \
               AND e.invoice_id IS NOT NULL",
        )
        .bind(self.tenant().as_str())
        .bind(week.user_id.as_str())
        .bind(week.week_start)
        .bind(week.week_end())
        .fetch_one(self.pool())
        .await
        .map_err(StoreError::Db)?;
        let (count, number) = row;
        if count == 0 {
            return Ok(None);
        }
        Ok(Some(BilledHours { count, number }))
    }

    /// Names, in a refusal, why a decision route moved nothing — read after the
    /// statement declined. A week that is not this tenant's is `NotFound` and
    /// never a conflict, so the refusal is not an existence oracle.
    async fn decision_refusal(&self, id: &TimeWeekId, verb: &str) -> StoreError {
        match self.week_by_id(id).await {
            Ok(None) => StoreError::NotFound,
            Ok(Some(week)) => StoreError::Conflict(format!(
                "the week of {} is {} and cannot be {verb}",
                week.week_start,
                week.status.as_str()
            )),
            Err(error) => error,
        }
    }
}

/// The hours of a week that have already reached a document — what a refusal to
/// reopen it has to say.
struct BilledHours {
    count: i64,
    /// The lowest document number carrying them; `None` only if the invoice row
    /// vanished under us, or it is still an unnumbered draft.
    number: Option<String>,
}

impl BilledHours {
    /// The refusal's words: how many hours have left the module, and where they
    /// went.
    fn refusal(&self) -> String {
        let count = self.count;
        match self.number.as_deref() {
            Some(number) => format!(
                "{count} of this week's hours are already on invoice {number}; void or credit \
                 that document to release them before reopening the week"
            ),
            None => format!(
                "{count} of this week's hours are already on a draft invoice; delete or credit \
                 that document to release them before reopening the week"
            ),
        }
    }
}

/// [`WEEK_COLS`] qualified with a table alias, for the joined read.
fn week_cols_prefixed(alias: &str) -> String {
    WEEK_COLS
        .split(", ")
        .map(|column| format!("{alias}.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct WeekRow {
    id: String,
    user_id: String,
    week_start: Date,
    status: String,
    submitted_at: Option<OffsetDateTime>,
    decided_by: Option<String>,
    decided_at: Option<OffsetDateTime>,
    decision_note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl WeekRow {
    fn into_week(self) -> TimesheetWeek {
        TimesheetWeek {
            id: TimeWeekId::new(self.id),
            user_id: UserId::new(self.user_id),
            week_start: self.week_start,
            // An unknown stored value can only come from a future migration this
            // binary predates; reading it as open is the same choice
            // `require_week_unlocked` makes, and for the same reason.
            status: WeekStatus::parse(&self.status).unwrap_or(WeekStatus::Open),
            submitted_at: self.submitted_at,
            decided_by: self.decided_by.map(UserId::new),
            decided_at: self.decided_at,
            decision_note: self.decision_note,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    #[sqlx(flatten)]
    week: WeekRow,
    user_email: String,
    minutes: i64,
    billable_minutes: i64,
}

impl PendingRow {
    fn into_pending(self) -> PendingWeek {
        PendingWeek {
            week: self.week.into_week(),
            user_email: self.user_email,
            minutes: self.minutes,
            billable_minutes: self.billable_minutes,
            projects: Vec::new(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct PendingProjectRow {
    week_id: String,
    project_id: String,
    project_name: String,
    minutes: i64,
    billable_minutes: i64,
}

impl PendingProjectRow {
    fn into_project(self) -> PendingProjectHours {
        PendingProjectHours {
            project_id: self.project_id,
            project_name: self.project_name,
            minutes: self.minutes,
            billable_minutes: self.billable_minutes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(d: u8) -> Date {
        Date::from_calendar_date(2026, Month::August, d).unwrap_or(Date::MIN)
    }

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn every_day_of_a_week_resolves_to_the_same_monday() {
        // 2026-08-03 is a Monday; 2026-08-09 the Sunday that closes its week.
        for d in 3..=9 {
            assert_eq!(week_start(day(d)), day(3), "the {d}th");
        }
        assert_eq!(week_start(day(10)), day(10), "the next Monday is its own");
        assert_eq!(week_end(day(3)), day(9));
    }

    #[test]
    fn a_week_is_addressed_by_its_monday_and_nothing_else() {
        assert_eq!(require_monday(day(3)).unwrap_or(Date::MIN), day(3));
        for d in [4, 5, 6, 7, 8, 9] {
            let refusal = message(require_monday(day(d)));
            assert!(refusal.contains("addressed by its Monday"), "{refusal}");
            assert!(
                refusal.contains("2026-08-03"),
                "the refusal names the Monday that was probably meant: {refusal}"
            );
        }
    }

    #[test]
    fn a_week_boundary_holds_across_a_year_and_across_the_iso_year() {
        // 2027-01-01 is a Friday: its week starts in the previous calendar year,
        // which is the case `insight_series` also had to get right.
        let friday = Date::from_calendar_date(2027, Month::January, 1).unwrap_or(Date::MIN);
        let monday = Date::from_calendar_date(2026, Month::December, 28).unwrap_or(Date::MIN);
        assert_eq!(week_start(friday), monday);
        assert_eq!(
            week_end(monday),
            Date::from_calendar_date(2027, Month::January, 3).unwrap_or(Date::MIN)
        );
    }

    #[test]
    fn the_lock_is_exactly_submitted_and_approved() {
        assert!(WeekStatus::Submitted.is_locked());
        assert!(WeekStatus::Approved.is_locked());
        assert!(
            !WeekStatus::Open.is_locked(),
            "nothing pending, nothing held"
        );
        assert!(
            !WeekStatus::Rejected.is_locked(),
            "the point of a rejection is that the week can be fixed"
        );
    }

    #[test]
    fn the_transitions_are_the_state_machine_and_nothing_else() {
        assert!(WeekStatus::Open.can_submit() && WeekStatus::Rejected.can_submit());
        assert!(!WeekStatus::Submitted.can_submit() && !WeekStatus::Approved.can_submit());

        assert!(WeekStatus::Submitted.can_decide());
        for already in [WeekStatus::Open, WeekStatus::Approved, WeekStatus::Rejected] {
            assert!(!already.can_decide(), "{already:?}");
        }

        assert!(WeekStatus::Approved.can_reopen() && WeekStatus::Rejected.can_reopen());
        assert!(!WeekStatus::Open.can_reopen() && !WeekStatus::Submitted.can_reopen());
    }

    #[test]
    fn a_decision_names_the_state_it_produces() {
        assert_eq!(
            WeekDecision::Approve.resulting_status(),
            WeekStatus::Approved
        );
        assert_eq!(
            WeekDecision::Reject.resulting_status(),
            WeekStatus::Rejected
        );
        assert!(WeekDecision::Approve.resulting_status().is_locked());
        assert!(!WeekDecision::Reject.resulting_status().is_locked());
    }

    #[test]
    fn every_stored_status_round_trips_and_nothing_else_parses() {
        for status in [
            WeekStatus::Open,
            WeekStatus::Submitted,
            WeekStatus::Approved,
            WeekStatus::Rejected,
        ] {
            assert_eq!(WeekStatus::parse(status.as_str()), Some(status));
        }
        for unknown in ["", "OPEN", "locked", "withdrawn"] {
            assert_eq!(WeekStatus::parse(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn a_refusal_to_reopen_says_how_many_hours_left_and_where_they_went() {
        let numbered = BilledHours {
            count: 3,
            number: Some("INV-2026-00007".to_owned()),
        }
        .refusal();
        assert!(numbered.contains("3 of this week's hours"));
        assert!(numbered.contains("INV-2026-00007"));
        assert!(numbered.contains("void or credit"));

        let draft = BilledHours {
            count: 1,
            number: None,
        }
        .refusal();
        assert!(draft.contains("draft invoice"), "{draft}");
    }

    #[test]
    fn the_joined_read_qualifies_every_column_it_selects() {
        let prefixed = week_cols_prefixed("w");
        assert!(prefixed.starts_with("w.id, w.user_id, w.week_start, w.status"));
        assert_eq!(
            prefixed.split(", ").count(),
            WEEK_COLS.split(", ").count(),
            "no column is dropped or duplicated by the prefixing"
        );
        assert!(
            prefixed.split(", ").all(|c| c.starts_with("w.")),
            "an unqualified column would be ambiguous against the joined tables: {prefixed}"
        );
    }
}
