//! The hours themselves — one row per completed piece of work (alo Projects,
//! ADR 0035, wave B3), reached through the account door and **only ever the
//! caller's own**.
//!
//! [`crate::project_clients`] says who a project is worked for; this module
//! says who worked, on what day, for how long, and at what rate that time was
//! priced. Everything downstream — the week grid, the approval (B3.05), the
//! invoice draft (B3.06), the profitability report (B3.08) — is a fold over
//! these rows.
//!
//! # A person's hours are personal data
//!
//! A record of when an employee worked, on what and for how long is personal
//! data under the GDPR and a works-council question in several member states.
//! Every statement here binds `user_id = self.user` from the account door, so
//! reaching a colleague's hours through this API is **unrepresentable, not
//! merely rejected** — there is no function that takes a user id. The
//! cross-user reads and the approval decision live on the tenant door behind
//! `require_admin` and arrive with the week (B3.05). Notes never reach a log:
//! a time note can name a client, a person or a case, so the spans on this path
//! carry ids and minute counts and nothing a human typed.
//!
//! # Minutes are the stored truth
//!
//! Hours exist only on a document. [`MINUTES_MAX`] is a day, and a night shift
//! over midnight is two entries — which is also how it must be billed. The
//! conversion of minutes into a billing line's milli-hour quantity happens once,
//! in one pure function, at the handoff (B3.06); nothing here computes money.
//!
//! # What is a snapshot, and what is a reference
//!
//! `rate_cents`/`currency` are copied onto the entry as it is written, resolved
//! in one order: the caller's explicit rate → the project's `rate_cents` →
//! nothing. A later change to the engagement's rate never rewrites an hour
//! already logged, for the reason a billing line snapshots its price instead of
//! joining to the price list. **A billable entry with no rate is legal**: the
//! person logging the hour is frequently not the person who prices it. What is
//! not legal is billing it — the handoff demands a rate rather than guessing
//! one.
//!
//! Correcting an entry ([`AccountStore::edit_time_entry`]) therefore does not
//! touch the rate. Repricing an hour is not a correction of what happened.
//!
//! # Two things freeze an hour, and they are both somewhere else
//!
//! An entry refuses to move when it is **already on a document**
//! ([`require_unbilled`], here, because `invoice_id` is a column of this table)
//! and when **its week has been handed in or approved**
//! ([`crate::time_weeks::require_week_unlocked`], there, because a week's status
//! is a fact about the week). Every write below asks both questions, and a
//! correction that moves an entry to another day asks the week question
//! **twice** — of the week it leaves and the week it joins — because otherwise a
//! locked week can be drained one entry at a time.
//!
//! The one deliberate exception is [`AccountStore::reject_time_entry`]: a
//! proposal is in no total, so discarding one changes nothing an approver saw,
//! and since *creating* a proposal in a locked week is refused, one found there
//! can only be a draft the lock arrived after. Refusing its rejection too would
//! leave it stuck with no way to clear it.
//!
//! # Scope of this slice (B3.03, extended at B3.04 and B3.05)
//!
//! Create, read, list, correct, delete, and the proposal verbs.
//!
//! B3.04 added two things and no new rules: [`week_totals`], the minute fold a
//! week grid puts at the bottom of its column, and [`insert_entry`] — the one
//! place an hour is written, lifted out of [`AccountStore::log_time`] so that
//! [`crate::time_timer`]'s stop can write inside the same transaction that
//! clears the running clock. B3.05 added the week lock to every one of them.

use sqlx::PgConnection;
use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::{bounded, currency as validate_currency, unit_price_cents};
use crate::error::{Result, StoreError};
use crate::id::{BillingInvoiceId, ProjectId, TaskId, TimeEntryId, UserId};
use crate::time_weeks::require_week_unlocked;

/// The shortest entry that is work at all. Zero minutes is not a piece of work,
/// and an entry of zero would still be a row in every total.
pub const MINUTES_MIN: i64 = 1;

/// The longest entry a single day can hold. A night shift over midnight is two
/// entries, one per day, which is also how it must be billed.
pub const MINUTES_MAX: i64 = 1440;

/// Longest note we keep on an entry. A note is a sentence about the work, not
/// the deliverable.
pub const NOTE_MAX: usize = 500;

/// Longest source kind/id a drafted entry may carry (`'event'` plus the id of
/// the Agenda event it came from, B3.10).
const SOURCE_KIND_MAX: usize = 32;
const SOURCE_ID_MAX: usize = 200;

/// The stored state of a real, countable entry.
const STATE_ACTIVE: &str = "active";

/// The stored state of an agent's suggestion, awaiting a human (ADR 0023).
const STATE_PROPOSED: &str = "proposed";

/// The columns every read selects, in [`EntryRow`] order.
const ENTRY_COLS: &str = "id, user_id, project_id, task_id, work_date, started_at, minutes, \
     billable, rate_cents, currency, note, state, source_kind, source_id, invoice_id, \
     billed_at, created_at, updated_at";

/// A piece of work to record.
#[derive(Debug, Clone)]
pub struct NewTimeEntry {
    /// The board it was worked on — a team project, or the caller's own
    /// personal one.
    pub project_id: ProjectId,
    /// The task inside that project, when the worker named one. It must live on
    /// the same project: an hour attributed to a task on another board would be
    /// counted against one engagement and described by another.
    pub task_id: Option<TaskId>,
    /// The day the person says they worked, in their own zone.
    pub work_date: Date,
    /// Provenance only — the instant a timer started or a calendar event began.
    /// Never a period boundary.
    pub started_at: Option<OffsetDateTime>,
    /// How long, in minutes ([`MINUTES_MIN`]…[`MINUTES_MAX`]).
    pub minutes: i64,
    /// Whether the hour is chargeable to the project's customer. Unrelated to
    /// whether it has a rate: an unrated billable hour is legal and counted.
    pub billable: bool,
    /// An explicit rate for this entry in integer cents, which wins over the
    /// project's. `None` — the ordinary case — takes the engagement's rate.
    pub rate_cents: Option<i64>,
    /// The currency of [`Self::rate_cents`]. `None` takes the engagement's own;
    /// only a project with no client facts needs the caller to state one.
    pub currency: Option<String>,
    /// What the person did. May be empty.
    pub note: String,
    /// Whether this is an agent's suggestion rather than a human's record
    /// (ADR 0023). A proposal is excluded from every aggregate and carries no
    /// rate until a human accepts it.
    pub proposed: bool,
    /// Where a drafted entry came from — `"event"` for one drafted off the
    /// caller's Agenda (B3.10).
    pub source_kind: Option<String>,
    /// The id of that source record.
    pub source_id: Option<String>,
}

impl NewTimeEntry {
    /// The minimum a caller must state: a project, a day and a duration.
    /// Billable, because a client project's hours are chargeable unless
    /// somebody says otherwise, and the person logging them is who says so.
    pub fn worked(project_id: ProjectId, work_date: Date, minutes: i64) -> Self {
        Self {
            project_id,
            task_id: None,
            work_date,
            started_at: None,
            minutes,
            billable: true,
            rate_cents: None,
            currency: None,
            note: String::new(),
            proposed: false,
            source_kind: None,
            source_id: None,
        }
    }
}

/// The correctable facts of an entry already written: what was done, when and
/// for how long.
///
/// Neither the project nor the rate is here, and both absences are decisions.
/// Moving an hour to another engagement changes who is billed for it, which is
/// a new record rather than a correction of this one; and the rate is a
/// snapshot taken when the work was written down, so repricing it is not a
/// correction of what happened either.
#[derive(Debug, Clone)]
pub struct TimeEntryEdit {
    /// The day the work now belongs to.
    pub work_date: Date,
    /// The task inside the entry's project, or `None` to detach it.
    pub task_id: Option<TaskId>,
    /// How long, in minutes ([`MINUTES_MIN`]…[`MINUTES_MAX`]).
    pub minutes: i64,
    /// Whether the hour is chargeable.
    pub billable: bool,
    /// What the person did.
    pub note: String,
}

/// One recorded piece of work.
#[derive(Debug, Clone)]
pub struct TimeEntry {
    /// Opaque id, unique within the tenant.
    pub id: TimeEntryId,
    /// Who worked. Always the caller, on this door.
    pub user_id: UserId,
    /// The board the work was done on.
    pub project_id: ProjectId,
    /// The task it was done under, if the worker named one.
    pub task_id: Option<TaskId>,
    /// The day the person says they worked.
    pub work_date: Date,
    /// Provenance: when a timer or calendar event started this. Never used as a
    /// period boundary.
    pub started_at: Option<OffsetDateTime>,
    /// How long, in minutes.
    pub minutes: i64,
    /// Whether the hour is chargeable to the customer.
    pub billable: bool,
    /// The rate this hour was priced at, in integer cents — a snapshot, `None`
    /// when nobody had priced the engagement.
    pub rate_cents: Option<i64>,
    /// The currency of [`Self::rate_cents`], snapshotted with it.
    pub currency: Option<String>,
    /// What the person did. Personal data: never logged.
    pub note: String,
    /// `active` or `proposed` ([`TimeEntry::is_proposed`]).
    pub state: String,
    /// Where a drafted entry came from.
    pub source_kind: Option<String>,
    /// The id of that source record.
    pub source_id: Option<String>,
    /// The document this hour was carried onto (B3.06), if any.
    pub invoice_id: Option<BillingInvoiceId>,
    /// When it was carried there.
    pub billed_at: Option<OffsetDateTime>,
    /// When the entry was written down.
    pub created_at: OffsetDateTime,
    /// When it was last corrected.
    pub updated_at: OffsetDateTime,
}

impl TimeEntry {
    /// Whether this is an agent's suggestion rather than a human's record. A
    /// proposal is in no total until somebody accepts it (ADR 0023).
    pub fn is_proposed(&self) -> bool {
        self.state == STATE_PROPOSED
    }

    /// Whether the hour has been carried onto a document. A billed hour is
    /// frozen: correcting it would restate a document a customer has read.
    pub fn is_billed(&self) -> bool {
        self.invoice_id.is_some()
    }

    /// Whether the hour can be priced at all — a billable entry without a rate
    /// is counted and shown as unrated, never priced at zero.
    pub fn is_rated(&self) -> bool {
        self.rate_cents.is_some()
    }
}

/// What a period of entries adds up to, in minutes — the figure a week grid
/// puts at the bottom of its column.
///
/// Minutes, never money: an hour with no rate is still an hour, and pricing a
/// timesheet is [`crate::billing_line`]'s job at the handoff. Nothing here is a
/// float.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeTotals {
    /// Every real (non-proposed) minute in the period, billable or not.
    pub minutes: i64,
    /// The subset of [`Self::minutes`] that is chargeable to a customer.
    pub billable_minutes: i64,
    /// Minutes still only suggested by an agent, in **no** other total here —
    /// counted separately so a screen can say "and 90 more minutes awaiting
    /// your confirmation" without a suggestion silently joining the week.
    pub proposed_minutes: i64,
}

/// Folds a period's entries into its totals.
///
/// Pure and total: `i64` saturating addition, so a corrupted row could at worst
/// pin a displayed total at the ceiling rather than panic a release build or
/// wrap a week's hours negative. The real bound is the column's own — 1440
/// minutes an entry.
#[must_use]
pub fn week_totals(entries: &[TimeEntry]) -> TimeTotals {
    let mut totals = TimeTotals::default();
    for entry in entries {
        if entry.is_proposed() {
            totals.proposed_minutes = totals.proposed_minutes.saturating_add(entry.minutes);
            continue;
        }
        totals.minutes = totals.minutes.saturating_add(entry.minutes);
        if entry.billable {
            totals.billable_minutes = totals.billable_minutes.saturating_add(entry.minutes);
        }
    }
    totals
}

/// The engagement facts a write needs from the entry's project: what it would
/// be priced at, and in what currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRate {
    pub(crate) rate_cents: Option<i64>,
    pub(crate) currency: Option<String>,
}

/// Validates a duration in minutes.
fn minutes(value: i64) -> Result<i64> {
    if !(MINUTES_MIN..=MINUTES_MAX).contains(&value) {
        return Err(StoreError::Validation(format!(
            "minutes must be between {MINUTES_MIN} and {MINUTES_MAX} — a day is the most one \
             entry can hold, and work over midnight is two entries"
        )));
    }
    Ok(value)
}

/// Resolves the rate snapshot an entry is written with, from the caller's
/// statement and the engagement's own facts. Pure — no database, so the
/// resolution order is unit-tested directly.
///
/// The order is: the caller's explicit rate → the project's → nothing. A
/// currency stated without any rate resolving describes nothing and is dropped:
/// the currency is snapshotted *with* the rate, and a UI that always sends the
/// engagement's currency must not turn an unpriced hour into a priced one.
pub(crate) fn snapshot_rate(
    stated_rate: Option<i64>,
    stated_currency: Option<&str>,
    project: &ProjectRate,
) -> Result<(Option<i64>, Option<String>)> {
    let Some(rate) = stated_rate.or(project.rate_cents) else {
        return Ok((None, None));
    };
    let rate = unit_price_cents("hourly rate", rate)?;
    let currency = match stated_currency {
        Some(stated) => validate_currency(stated)?,
        None => match project.currency.as_deref() {
            Some(own) => validate_currency(own)?,
            None => {
                return Err(StoreError::Validation(
                    "a rate needs a currency: this project has no client facts, so state the \
                     currency with the rate"
                        .to_owned(),
                ));
            }
        },
    };
    Ok((Some(rate), Some(currency)))
}

/// Validates a piece of work, prices it, and writes the row — **the one place
/// an hour is inserted**.
///
/// A free function over a connection rather than a method, because the two
/// callers need different transaction scopes: [`AccountStore::log_time`] writes
/// on its own, and the timer's stop
/// ([`crate::time_timer::AccountStore::stop_timer`]) writes inside the same
/// transaction that clears the running row, so the hour and the clearing stand
/// or fall together. Both get the same validation, the same rate snapshot and
/// the same columns because they call the same function.
///
/// Visibility is **not** checked here: it is a rule about which board a person
/// may start work on, and the two callers answer it at the moments it applies —
/// `log_time` before writing, `stop_timer` when the clock was started. An hour
/// already worked is not un-worked by the board being archived since.
///
/// The **week lock** *is* checked here, on the caller's connection, so that the
/// timer's stop tests the week inside the same transaction that writes the hour.
/// It applies to a proposal as much as to real work: a suggestion that could
/// never be accepted is not a suggestion.
///
/// # Errors
/// [`StoreError::NotFound`] never — the caller has already resolved the
/// project; [`StoreError::Validation`] when the duration, the note, the source
/// or the rate breaks its rule; [`StoreError::Conflict`] when the week the hour
/// falls in is submitted or approved; [`StoreError::Db`] on failure.
pub(crate) async fn insert_entry(
    conn: &mut PgConnection,
    tenant: &str,
    user: &str,
    new: &NewTimeEntry,
    project: &ProjectRate,
) -> Result<TimeEntry> {
    require_week_unlocked(conn, tenant, user, new.work_date).await?;
    let minutes = minutes(new.minutes)?;
    let note = bounded("note", &new.note, NOTE_MAX)?;
    let source_kind = optional_bounded("source kind", new.source_kind.as_deref(), SOURCE_KIND_MAX)?;
    let source_id = optional_bounded("source id", new.source_id.as_deref(), SOURCE_ID_MAX)?;
    // A proposal carries no rate: the price is resolved at acceptance, because
    // until then nobody has agreed that the work happened.
    let (rate_cents, currency) = if new.proposed {
        (None, None)
    } else {
        snapshot_rate(new.rate_cents, new.currency.as_deref(), project)?
    };
    let state = if new.proposed {
        STATE_PROPOSED
    } else {
        STATE_ACTIVE
    };

    let id = TimeEntryId::generate();
    let row = sqlx::query_as::<_, EntryRow>(&format!(
        "INSERT INTO time_entries (tenant_id, id, user_id, project_id, task_id, work_date, \
             started_at, minutes, billable, rate_cents, currency, note, state, source_kind, \
             source_id, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $3) \
         RETURNING {ENTRY_COLS}"
    ))
    .bind(tenant)
    .bind(id.as_str())
    .bind(user)
    .bind(new.project_id.as_str())
    .bind(new.task_id.as_ref().map(TaskId::as_str))
    .bind(new.work_date)
    .bind(new.started_at)
    .bind(minutes)
    .bind(new.billable)
    .bind(rate_cents)
    .bind(currency)
    .bind(note)
    .bind(state)
    .bind(source_kind)
    .bind(source_id)
    .fetch_one(conn)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.into_entry())
}

/// The engagement's price facts for a project this tenant owns, **without the
/// visibility check** — the read a stop makes inside its own transaction.
///
/// [`AccountStore::writable_project`] answers "may this person start work
/// here?", which is a question about a board somebody can open. This answers
/// "what is an hour here worth?", which is a question about the engagement and
/// stays answerable after the board is archived — otherwise a clock left
/// running over an archiving would lose the hour it had already counted.
///
/// # Errors
/// [`StoreError::NotFound`] when the project is not this tenant's;
/// [`StoreError::Db`] on failure.
pub(crate) async fn project_rate(
    conn: &mut PgConnection,
    tenant: &str,
    project: &ProjectId,
) -> Result<ProjectRate> {
    let row = sqlx::query_as::<_, (Option<i64>, Option<String>)>(
        "SELECT c.rate_cents, c.currency FROM task_projects p \
         LEFT JOIN project_clients c \
           ON c.tenant_id = p.tenant_id AND c.project_id = p.id \
         WHERE p.tenant_id = $1 AND p.id = $2",
    )
    .bind(tenant)
    .bind(project.as_str())
    .fetch_optional(conn)
    .await
    .map_err(StoreError::Db)?;
    let (rate_cents, currency) = row.ok_or(StoreError::NotFound)?;
    Ok(ProjectRate {
        rate_cents,
        currency,
    })
}

impl AccountStore {
    /// Records a piece of work the caller did.
    ///
    /// The rate is snapshotted here and never again ([`snapshot_rate`]) —
    /// except for a proposal, which is not work yet and is priced when a human
    /// accepts it ([`AccountStore::accept_time_entry`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the project is not one the caller can see
    /// (another tenant's, a colleague's personal board, or archived), or when
    /// the task is not one they can see — existence is never disclosed;
    /// [`StoreError::Validation`] when the duration, the note, the rate or the
    /// currency breaks its rule, or the task belongs to another project;
    /// [`StoreError::Db`] on failure.
    pub async fn log_time(&self, new: &NewTimeEntry) -> Result<TimeEntry> {
        let project = self.writable_project(&new.project_id).await?;
        self.require_task_on_project(new.task_id.as_ref(), &new.project_id)
            .await?;
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        insert_entry(
            &mut conn,
            self.tenant.as_str(),
            self.user.as_str(),
            new,
            &project,
        )
        .await
    }

    /// One of the caller's **own** entries, or `None`.
    ///
    /// A colleague's entry inside the same tenant reads exactly like another
    /// tenant's and like one that never existed: absent. Not a `Forbidden`,
    /// which would confirm that somebody worked that day.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn time_entry(&self, id: &TimeEntryId) -> Result<Option<TimeEntry>> {
        let row = sqlx::query_as::<_, EntryRow>(&format!(
            "SELECT {ENTRY_COLS} FROM time_entries \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(EntryRow::into_entry))
    }

    /// The caller's own entries between `from` and `to`, both days included,
    /// optionally on one project — the week grid's read.
    ///
    /// Proposals are returned alongside real entries, each saying which it is:
    /// the screen that offers a suggestion for acceptance is the same screen
    /// that shows the week, and it is the **totals** that exclude proposals,
    /// not the list. Ordered by day, then by the order they were written.
    ///
    /// The caller's own hours are returned whatever the state of the board they
    /// were worked on: a project archived after the fact must not silently
    /// empty somebody's timesheet. Project visibility is a rule about *writing*
    /// an hour, not about remembering one.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::Db`] on failure.
    pub async fn time_entries(
        &self,
        from: Date,
        to: Date,
        project: Option<&ProjectId>,
    ) -> Result<Vec<TimeEntry>> {
        if to < from {
            return Err(StoreError::Validation(
                "the end of the period must not be before its start".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, EntryRow>(&format!(
            "SELECT {ENTRY_COLS} FROM time_entries \
             WHERE tenant_id = $1 AND user_id = $2 AND work_date >= $3 AND work_date <= $4 \
               AND ($5::text IS NULL OR project_id = $5) \
             ORDER BY work_date, created_at, id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(from)
        .bind(to)
        .bind(project.map(ProjectId::as_str))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(EntryRow::into_entry).collect())
    }

    /// The caller's own pending entry proposals (ADR 0023), newest first —
    /// awaiting accept or reject.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn time_entry_proposals(&self) -> Result<Vec<TimeEntry>> {
        let rows = sqlx::query_as::<_, EntryRow>(&format!(
            "SELECT {ENTRY_COLS} FROM time_entries \
             WHERE tenant_id = $1 AND user_id = $2 AND state = '{STATE_PROPOSED}' \
             ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(rows.into_iter().map(EntryRow::into_entry).collect())
    }

    /// Corrects one of the caller's own entries.
    ///
    /// The rate is untouched by design (see [`TimeEntryEdit`]). An entry
    /// already carried onto a document is frozen: the hours are on paper a
    /// customer has read, and the way back is to void or credit that document
    /// (B1's own verbs), not to edit history underneath it.
    ///
    /// **Both weeks are checked** — the one the entry is in and the one the
    /// correction moves it to. Checking only the destination would let a locked
    /// week be drained a day at a time; checking only the source would let hours
    /// be pushed into a week somebody has already approved.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the entry is not the caller's own;
    /// [`StoreError::Conflict`] when it is already billed, or either week is
    /// submitted or approved; [`StoreError::Validation`] when a field breaks its
    /// rule or the task belongs to another project; [`StoreError::Db`] on
    /// failure.
    pub async fn edit_time_entry(
        &self,
        id: &TimeEntryId,
        edit: &TimeEntryEdit,
    ) -> Result<TimeEntry> {
        let entry = self.time_entry(id).await?.ok_or(StoreError::NotFound)?;
        require_unbilled(&entry)?;
        self.require_weeks_unlocked(&[entry.work_date, edit.work_date])
            .await?;
        self.require_task_on_project(edit.task_id.as_ref(), &entry.project_id)
            .await?;
        let minutes = minutes(edit.minutes)?;
        let note = bounded("note", &edit.note, NOTE_MAX)?;
        let row = sqlx::query_as::<_, EntryRow>(&format!(
            "UPDATE time_entries SET work_date = $4, task_id = $5, minutes = $6, billable = $7, \
                 note = $8, updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 \
             RETURNING {ENTRY_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(edit.work_date)
        .bind(edit.task_id.as_ref().map(TaskId::as_str))
        .bind(minutes)
        .bind(edit.billable)
        .bind(note)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        Ok(row.into_entry())
    }

    /// Removes one of the caller's own entries.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the entry is not the caller's own;
    /// [`StoreError::Conflict`] when it is already billed or its week is
    /// submitted or approved; [`StoreError::Db`] on failure.
    pub async fn delete_time_entry(&self, id: &TimeEntryId) -> Result<()> {
        let entry = self.time_entry(id).await?.ok_or(StoreError::NotFound)?;
        require_unbilled(&entry)?;
        self.require_weeks_unlocked(&[entry.work_date]).await?;
        let done = sqlx::query(
            "DELETE FROM time_entries WHERE tenant_id = $1 AND user_id = $2 AND id = $3",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Accepts one of the caller's own proposed entries: it becomes real work,
    /// and **its rate is resolved now** — at the moment a human agreed the work
    /// happened, from the engagement's facts as they stand today.
    ///
    /// Accepting is what puts the hour into the week's totals, so it is a write
    /// like any other and the week lock applies.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the entry is not the caller's own pending
    /// proposal — an already-accepted one included, so a double accept cannot
    /// reprice an hour; [`StoreError::Conflict`] when its week is submitted or
    /// approved; [`StoreError::Validation`] when the project has since gained a
    /// rate with no currency to express it in; [`StoreError::Db`] on failure.
    pub async fn accept_time_entry(&self, id: &TimeEntryId) -> Result<TimeEntry> {
        let entry = self.time_entry(id).await?.ok_or(StoreError::NotFound)?;
        if !entry.is_proposed() {
            return Err(StoreError::NotFound);
        }
        self.require_weeks_unlocked(&[entry.work_date]).await?;
        // The board may have been archived, or the engagement repriced, since
        // the suggestion was drafted; both are answered by resolving against
        // what is true now rather than what was true then.
        let project = self.writable_project(&entry.project_id).await?;
        let (rate_cents, currency) = snapshot_rate(None, None, &project)?;
        let row = sqlx::query_as::<_, EntryRow>(&format!(
            "UPDATE time_entries SET state = '{STATE_ACTIVE}', rate_cents = $4, currency = $5, \
                 updated_at = now() \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND state = '{STATE_PROPOSED}' \
             RETURNING {ENTRY_COLS}"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .bind(rate_cents)
        .bind(currency)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)?;
        Ok(row.into_entry())
    }

    /// Rejects one of the caller's own proposed entries by deleting it. A
    /// suggestion nobody accepted is not a record of anything.
    ///
    /// **The week lock deliberately does not apply here.** A proposal is in no
    /// total, so discarding one changes nothing an approver saw; and since
    /// creating a proposal in a locked week is refused, one found in a locked
    /// week is a draft the lock arrived after. Refusing its rejection would
    /// leave it stuck with no way to clear it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the entry is not the caller's own pending
    /// proposal; [`StoreError::Db`] on failure.
    pub async fn reject_time_entry(&self, id: &TimeEntryId) -> Result<()> {
        let done = sqlx::query(&format!(
            "DELETE FROM time_entries \
             WHERE tenant_id = $1 AND user_id = $2 AND id = $3 AND state = '{STATE_PROPOSED}'"
        ))
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Confirms the caller may log work on this project, and answers with the
    /// engagement's rate in the same round trip.
    ///
    /// Visibility is the rule the Tasks module already enforces on its own
    /// board — a team project, or the caller's own personal one, and not
    /// archived — because an hour is logged against a board somebody can open.
    /// A board they cannot see reads as absent, never as a refusal that would
    /// confirm it exists.
    pub(crate) async fn writable_project(&self, project: &ProjectId) -> Result<ProjectRate> {
        let row = sqlx::query_as::<_, (Option<i64>, Option<String>)>(
            "SELECT c.rate_cents, c.currency FROM task_projects p \
             LEFT JOIN project_clients c \
               ON c.tenant_id = p.tenant_id AND c.project_id = p.id \
             WHERE p.tenant_id = $1 AND p.id = $3 AND p.archived = false \
               AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(project.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (rate_cents, currency) = row.ok_or(StoreError::NotFound)?;
        Ok(ProjectRate {
            rate_cents,
            currency,
        })
    }

    /// Confirms every week these days fall in is still the caller's to change.
    ///
    /// Takes days rather than Mondays so no caller here resolves a week
    /// boundary itself, and takes a slice because a correction that moves an
    /// entry has two weeks to answer for and both must pass before either is
    /// touched. Duplicates are harmless: an entry corrected within its own week
    /// asks the same question twice and gets the same answer.
    async fn require_weeks_unlocked(&self, days: &[Date]) -> Result<()> {
        let mut conn = self.pool.acquire().await.map_err(StoreError::Db)?;
        for day in days {
            require_week_unlocked(&mut conn, self.tenant.as_str(), self.user.as_str(), *day)
                .await?;
        }
        Ok(())
    }

    /// Confirms a named task is one the caller can see and lives on the
    /// entry's project. `None` is always fine — a task is optional detail.
    pub(crate) async fn require_task_on_project(
        &self,
        task: Option<&TaskId>,
        project: &ProjectId,
    ) -> Result<()> {
        let Some(task) = task else { return Ok(()) };
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT t.project_id FROM tasks t \
             JOIN task_projects p ON p.tenant_id = t.tenant_id AND p.id = t.project_id \
             WHERE t.tenant_id = $1 AND t.id = $3 \
               AND (p.kind = 'team' OR (p.kind = 'personal' AND p.owner_user_id = $2))",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(task.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let (on_project,) = row.ok_or(StoreError::NotFound)?;
        if on_project != project.as_str() {
            return Err(StoreError::Validation(
                "the task must be on the same project as the entry".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Refuses to move an hour that is already on a document.
fn require_unbilled(entry: &TimeEntry) -> Result<()> {
    if entry.is_billed() {
        return Err(StoreError::Conflict(
            "this entry is already on an invoice; void or credit the document to release it"
                .to_owned(),
        ));
    }
    Ok(())
}

/// [`bounded`] over an optional field: absent stays absent, and a value that is
/// only whitespace is absent too.
fn optional_bounded(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = bounded(field, raw, max)?;
            Ok(if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            })
        }
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct EntryRow {
    id: String,
    user_id: String,
    project_id: String,
    task_id: Option<String>,
    work_date: Date,
    started_at: Option<OffsetDateTime>,
    minutes: i64,
    billable: bool,
    rate_cents: Option<i64>,
    currency: Option<String>,
    note: String,
    state: String,
    source_kind: Option<String>,
    source_id: Option<String>,
    invoice_id: Option<String>,
    billed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl EntryRow {
    fn into_entry(self) -> TimeEntry {
        TimeEntry {
            id: TimeEntryId::new(self.id),
            user_id: UserId::new(self.user_id),
            project_id: ProjectId::new(self.project_id),
            task_id: self.task_id.map(TaskId::new),
            work_date: self.work_date,
            started_at: self.started_at,
            minutes: self.minutes,
            billable: self.billable,
            rate_cents: self.rate_cents,
            currency: self.currency,
            note: self.note,
            state: self.state,
            source_kind: self.source_kind,
            source_id: self.source_id,
            invoice_id: self.invoice_id.map(BillingInvoiceId::new),
            billed_at: self.billed_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(StoreError::Validation(msg)) => msg,
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn unpriced() -> ProjectRate {
        ProjectRate {
            rate_cents: None,
            currency: None,
        }
    }

    fn priced(rate: i64, currency: &str) -> ProjectRate {
        ProjectRate {
            rate_cents: Some(rate),
            currency: Some(currency.to_owned()),
        }
    }

    #[test]
    fn a_day_is_the_most_one_entry_holds() {
        for ok in [MINUTES_MIN, 30, 480, MINUTES_MAX] {
            assert_eq!(minutes(ok).unwrap_or(0), ok, "the bounds are inclusive");
        }
        for bad in [0, -1, MINUTES_MAX + 1, i64::MIN, i64::MAX] {
            assert!(
                message(minutes(bad)).contains("minutes must be between"),
                "expected a refusal naming the rule: {bad}"
            );
        }
    }

    #[test]
    fn an_unpriced_engagement_writes_an_unrated_hour() {
        // Legal and normal: the person logging the hour is frequently not the
        // person who prices it.
        let (rate, currency) =
            snapshot_rate(None, None, &unpriced()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, None);
        assert_eq!(currency, None);
    }

    #[test]
    fn the_engagements_rate_is_what_an_hour_is_priced_at() {
        let (rate, currency) =
            snapshot_rate(None, None, &priced(9_500, "chf")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, Some(9_500));
        assert_eq!(currency.as_deref(), Some("CHF"), "and it is uppercased");
    }

    #[test]
    fn an_explicit_rate_wins_over_the_engagements() {
        let (rate, currency) = snapshot_rate(Some(12_000), None, &priced(9_500, "EUR"))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, Some(12_000));
        assert_eq!(
            currency.as_deref(),
            Some("EUR"),
            "an unstated currency is still the engagement's"
        );
        let (_, stated) = snapshot_rate(Some(12_000), Some("usd"), &priced(9_500, "EUR"))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(stated.as_deref(), Some("USD"));
    }

    #[test]
    fn a_rate_on_an_internal_project_needs_its_own_currency() {
        assert!(
            message(snapshot_rate(Some(12_000), None, &unpriced())).contains("needs a currency")
        );
        let (rate, currency) = snapshot_rate(Some(12_000), Some("EUR"), &unpriced())
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, Some(12_000));
        assert_eq!(currency.as_deref(), Some("EUR"));
    }

    #[test]
    fn a_currency_without_a_rate_describes_nothing_and_is_dropped() {
        // A UI that always sends the engagement's currency must not turn an
        // unpriced hour into a priced one.
        let (rate, currency) =
            snapshot_rate(None, Some("EUR"), &unpriced()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, None);
        assert_eq!(currency, None);
    }

    #[test]
    fn a_rate_shares_the_billing_line_ceiling() {
        let max = crate::billing_field::UNIT_PRICE_MAX_CENTS;
        let (rate, _) =
            snapshot_rate(Some(max), Some("EUR"), &unpriced()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(rate, Some(max), "the ceiling itself is legal");
        for bad in [-1, max + 1] {
            assert!(
                message(snapshot_rate(Some(bad), Some("EUR"), &unpriced())).contains("hourly rate")
            );
        }
        // A rate that reached the project row is validated on the way out too:
        // a bad snapshot must not become a bad invoice line.
        assert!(message(snapshot_rate(None, None, &priced(-1, "EUR"))).contains("hourly rate"));
        assert!(message(snapshot_rate(None, None, &priced(9_500, "EURO"))).contains("ISO 4217"));
    }

    #[test]
    fn a_stated_currency_that_is_not_a_code_is_refused() {
        assert!(
            message(snapshot_rate(Some(9_500), Some("EURO"), &unpriced())).contains("ISO 4217")
        );
    }

    #[test]
    fn an_empty_source_is_no_source() {
        assert_eq!(
            optional_bounded("source kind", Some("  "), SOURCE_KIND_MAX)
                .unwrap_or(Some("x".into())),
            None
        );
        assert_eq!(
            optional_bounded("source kind", Some(" event "), SOURCE_KIND_MAX)
                .unwrap_or(None)
                .as_deref(),
            Some("event")
        );
        let long = "e".repeat(SOURCE_KIND_MAX + 1);
        assert!(
            message(optional_bounded(
                "source kind",
                Some(&long),
                SOURCE_KIND_MAX
            ))
            .contains("source kind")
        );
    }

    #[test]
    fn a_proposal_and_a_billed_hour_say_what_they_are() {
        let base = TimeEntry {
            id: TimeEntryId::new("e"),
            user_id: UserId::new("u"),
            project_id: ProjectId::new("p"),
            task_id: None,
            work_date: Date::from_calendar_date(2026, time::Month::August, 3).unwrap_or(Date::MIN),
            started_at: None,
            minutes: 60,
            billable: true,
            rate_cents: None,
            currency: None,
            note: String::new(),
            state: STATE_ACTIVE.to_owned(),
            source_kind: None,
            source_id: None,
            invoice_id: None,
            billed_at: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        };
        assert!(!base.is_proposed() && !base.is_billed() && !base.is_rated());
        assert!(require_unbilled(&base).is_ok());

        let proposal = TimeEntry {
            state: STATE_PROPOSED.to_owned(),
            ..base.clone()
        };
        assert!(proposal.is_proposed());

        let billed = TimeEntry {
            invoice_id: Some(BillingInvoiceId::new("inv")),
            rate_cents: Some(9_500),
            currency: Some("EUR".to_owned()),
            ..base
        };
        assert!(billed.is_billed() && billed.is_rated());
        assert!(matches!(
            require_unbilled(&billed),
            Err(StoreError::Conflict(_))
        ));
    }

    /// One entry of `minutes`, in the state the argument names.
    fn entry(minutes: i64, billable: bool, proposed: bool) -> TimeEntry {
        TimeEntry {
            id: TimeEntryId::new("e"),
            user_id: UserId::new("u"),
            project_id: ProjectId::new("p"),
            task_id: None,
            work_date: Date::from_calendar_date(2026, time::Month::August, 3).unwrap_or(Date::MIN),
            started_at: None,
            minutes,
            billable,
            rate_cents: None,
            currency: None,
            note: String::new(),
            state: if proposed {
                STATE_PROPOSED
            } else {
                STATE_ACTIVE
            }
            .to_owned(),
            source_kind: None,
            source_id: None,
            invoice_id: None,
            billed_at: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_period_with_no_hours_totals_nothing() {
        assert_eq!(week_totals(&[]), TimeTotals::default());
    }

    #[test]
    fn a_suggestion_is_counted_apart_and_never_inside_the_week() {
        let totals = week_totals(&[
            entry(60, true, false),
            entry(30, false, false),
            entry(90, true, true),
        ]);
        assert_eq!(totals.minutes, 90, "the two real entries, billable or not");
        assert_eq!(totals.billable_minutes, 60);
        assert_eq!(
            totals.proposed_minutes, 90,
            "a suggestion is visible as a suggestion and in no other total"
        );
    }

    #[test]
    fn a_weeks_total_never_wraps() {
        let huge = [entry(i64::MAX, true, false), entry(1_440, true, false)];
        let totals = week_totals(&huge);
        assert_eq!(totals.minutes, i64::MAX, "saturating, never wrapped");
        assert_eq!(totals.billable_minutes, i64::MAX);
    }
}
