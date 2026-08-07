//! Recurring invoices (alo Billing, ADR 0035, wave B2.11) — the standing
//! arrangement that raises the same invoice again every week, month, quarter or
//! year, reached through the account door like [`crate::billing_invoices`].
//!
//! **A schedule never issues anything.** What a due run produces is a *draft*,
//! which a colleague then reads and issues by hand. Issuing spends a number out
//! of a legally gapless series and freezes a document a customer and a tax
//! authority may act on; no unattended job of ours does that on a tenant's
//! behalf, and `docs/features.md` [B2] asks for exactly this — "auto-draft for
//! approval".
//!
//! **The template is a snapshot**, like a document's lines are: the arrangement
//! carries its own copy of the customer, the currency, the terms and the lines,
//! so editing the price list — or the arrangement itself — never rewrites the
//! drafts already raised from it. A due run is therefore a *copy*, and
//! [`crate::billing_line`] is the one line model both sides share so that it can
//! be one.
//!
//! **A period can be billed once.** The run takes the schedule's row lock,
//! raises one draft per occurrence that has come due, and moves `next_run_date`
//! forward inside the same transaction; on top of that the document itself
//! records *which* occurrence it is for, and `(schedule_id, schedule_due_date)`
//! is unique in the database. Two runs racing therefore cannot double-bill a
//! month even if a future caller forgets the lock.
//!
//! **The clock is an argument, never a global.** Every entry point takes
//! `today: Date`. That is what makes the whole feature testable — a test runs
//! April on a database that thinks it is January — and it is also honest about
//! where the date comes from: the HTTP route passes the server's date, the
//! background sweep passes its own, and nothing here reads a clock behind the
//! caller's back.
//!
//! Tenancy is structural: every statement carries `tenant_id` from the handle,
//! the customer link is re-checked under the same handle before it is written,
//! and the database backs that with a composite foreign key on
//! `(tenant_id, customer_id)`.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_cadence::{Cadence, next_occurrence};
use crate::billing_field::{bounded, currency, payment_terms_days, required};
use crate::billing_invoices::InvoiceFromSchedule;
use crate::billing_line::{
    FiguresRow, INVOICE_LINES, Line, NewLine, SCHEDULE_LINES, group_figures,
};
use crate::billing_totals::{LineFigures, Totals, totals};
use crate::error::{Result, StoreError};
use crate::id::{BillingCustomerId, BillingInvoiceId, BillingScheduleId, TenantId, UserId};
use crate::store::Store;

/// What a human calls the arrangement in the list. A phrase, not a document —
/// nothing here is printed on the invoice.
pub const SCHEDULE_NAME_MAX_CHARS: usize = 120;
/// Copied onto every draft, so it carries the invoice's own bounds.
pub const SCHEDULE_REFERENCE_MAX_CHARS: usize = 120;
/// Likewise for the note printed under the lines.
pub const SCHEDULE_NOTE_MAX_CHARS: usize = 2_000;

/// The furthest into the past an arrangement may be dated when it is set up.
///
/// A start date is allowed to be in the past — a subscription agreed at the
/// beginning of the month and entered on the fifteenth still bills for that
/// month — but not arbitrarily so: a schedule backdated by years would mean a
/// standing instruction to raise years of drafts nobody asked for. A year is
/// where the useful case stops and the typo begins.
pub const SCHEDULE_MAX_BACKDATE_DAYS: i64 = 366;
/// The furthest into the future one may be dated: a plausible renewal, not a
/// date typed with the wrong century.
pub const SCHEDULE_MAX_LEAD_DAYS: i64 = 366 * 5;

/// The most drafts one run raises for one arrangement.
///
/// A run catches up: an arrangement that has been due three times since anyone
/// last looked raises three drafts, because three months were billable and a
/// business that quietly skips two of them is losing money. The cap is what
/// keeps a single run bounded — a weekly arrangement set up a year back has
/// fifty-two occurrences to raise — and the remainder is simply raised by the
/// next run, which is minutes away.
pub const SCHEDULE_MAX_PER_RUN: usize = 12;

/// The columns every read of a schedule selects, in `ScheduleRow` order.
const SCHEDULE_COLS: &str = "id, customer_id, name, cadence, anchor_day, start_date, end_date, \
     next_run_date, last_run_date, active, currency, payment_terms_days, reference, note, \
     created_by, created_at, updated_at";

/// The writable shape of a new arrangement.
///
/// `currency` and `payment_terms_days` are `None` to mean *take the customer's*,
/// exactly as on [`crate::billing_invoices::NewInvoice`]; whatever is resolved
/// is then stored on the arrangement, so a customer whose terms change next year
/// does not silently restate an arrangement agreed this year.
#[derive(Debug, Clone)]
pub struct NewSchedule {
    /// The party billed. Must be one of this tenant's customers.
    pub customer_id: BillingCustomerId,
    /// What a human calls it in the list.
    pub name: String,
    /// How often it bills.
    pub cadence: Cadence,
    /// The first date it bills on; also the day of the month it is anchored to.
    pub start_date: Date,
    /// The last date it may bill on, or `None` for "until somebody stops it".
    pub end_date: Option<Date>,
    /// ISO 4217 code, or `None` for the customer's default.
    pub currency: Option<String>,
    /// Days from issue to due on the drafts it raises, or `None` for the
    /// customer's terms.
    pub payment_terms_days: Option<i32>,
    /// The customer's own reference, copied onto every draft.
    pub reference: String,
    /// Free-text note, copied onto every draft.
    pub note: String,
}

/// The parts of a stored arrangement that stay editable.
///
/// The customer, the currency, the terms and the start date deliberately are
/// not. An arrangement *is* "this customer, in this currency, from this date";
/// changing any of them makes it a different arrangement, and the drafts it has
/// already raised would then be explained by a schedule that no longer matches
/// them. Ending this one and setting up another is one extra click and leaves a
/// history a bookkeeper can read.
///
/// Changing the **cadence** is allowed and does not move the next date: the
/// occurrence already scheduled stands, and the new rhythm applies from the one
/// after it.
#[derive(Debug, Clone)]
pub struct ScheduleEdit {
    /// What a human calls it.
    pub name: String,
    /// How often it bills from the next occurrence onwards.
    pub cadence: Cadence,
    /// The last date it may bill on, or `None`.
    pub end_date: Option<Date>,
    /// The customer's own reference, copied onto future drafts.
    pub reference: String,
    /// Free-text note, copied onto future drafts.
    pub note: String,
}

/// A stored arrangement. Its money lives in [`Totals`], computed from the
/// template lines.
#[derive(Debug, Clone)]
pub struct Schedule {
    /// Opaque id, unique within the tenant.
    pub id: BillingScheduleId,
    /// The party billed.
    pub customer_id: BillingCustomerId,
    /// What a human calls it.
    pub name: String,
    /// How often it bills.
    pub cadence: Cadence,
    /// The day of the month it is anchored to (1–31); unused by the weekly
    /// cadence, which keeps its weekday.
    pub anchor_day: u8,
    /// The first date it bills on.
    pub start_date: Date,
    /// The last date it may bill on, if it has one.
    pub end_date: Option<Date>,
    /// The next date a run will raise a draft for.
    pub next_run_date: Date,
    /// The day a run last raised anything for it.
    pub last_run_date: Option<Date>,
    /// Whether runs act on it at all. A paused arrangement keeps its dates and
    /// resumes where it left off.
    pub active: bool,
    /// ISO 4217 code the drafts are raised in.
    pub currency: String,
    /// Payment terms snapshotted onto every draft, in days.
    pub payment_terms_days: i32,
    /// The customer's own reference, copied onto every draft.
    pub reference: String,
    /// Free-text note, copied onto every draft.
    pub note: String,
    /// The colleague whose standing instruction this is; the drafts are raised
    /// as them.
    pub created_by: String,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last modification time.
    pub updated_at: OffsetDateTime,
}

impl Schedule {
    /// Whether this arrangement has run out of dates: it has an end date and
    /// the next occurrence is past it.
    ///
    /// Derived, never stored — it is a fact about two dates that are already on
    /// the row, and a stored flag would have to be moved by whichever run
    /// happened to notice. An **ended** arrangement is not the same as a paused
    /// one: it stays `active` and simply has nothing left to bill, which is what
    /// a reader needs to see to know it finished rather than was stopped.
    pub fn is_ended(&self) -> bool {
        self.end_date.is_some_and(|end| self.next_run_date > end)
    }

    /// Whether a run on `today` would raise anything: active, not ended, and
    /// due.
    ///
    /// The same predicate the run itself decides by, so a badge that says "due"
    /// and a run that raises nothing can never disagree.
    pub fn is_due(&self, today: Date) -> bool {
        self.active && !self.is_ended() && self.next_run_date <= today
    }
}

/// An arrangement as a list entry: the header and what one occurrence of it is
/// worth. The totals are computed from the template lines, never stored.
#[derive(Debug, Clone)]
pub struct ScheduleSummary {
    /// The header.
    pub schedule: Schedule,
    /// Net, VAT breakdown and gross of one occurrence.
    pub totals: Totals,
    /// How many drafts this arrangement has raised so far.
    pub raised_count: i64,
}

/// A whole arrangement: header, template lines in print order, and the totals
/// derived from those lines.
#[derive(Debug, Clone)]
pub struct ScheduleDocument {
    /// The header.
    pub schedule: Schedule,
    /// The template lines, in print order.
    pub lines: Vec<Line>,
    /// Net, VAT breakdown and gross of one occurrence.
    pub totals: Totals,
    /// How many drafts this arrangement has raised so far.
    pub raised_count: i64,
}

/// What one run of one arrangement did.
#[derive(Debug, Clone)]
pub struct ScheduleRun {
    /// The arrangement that ran.
    pub schedule_id: BillingScheduleId,
    /// The drafts it raised, oldest occurrence first. Empty when nothing was
    /// due — which is the ordinary answer, not a failure.
    pub raised: Vec<BillingInvoiceId>,
    /// The next date it will bill on after this run.
    pub next_run_date: Date,
}

/// The stored facts a run decides against, read under the schedule's row lock.
#[derive(Debug)]
struct LockedSchedule {
    customer_id: String,
    cadence: Cadence,
    anchor_day: u8,
    start_date: Date,
    end_date: Option<Date>,
    next_run_date: Date,
    active: bool,
    currency: String,
    payment_terms_days: i32,
    reference: String,
    note: String,
    created_by: String,
}

/// Turns a stored cadence string into a cadence, or reports corrupt data.
///
/// A cadence the code does not know is corrupt data, not user input: it is
/// reported as a decode failure rather than guessed at, because guessing here
/// would mean billing an arrangement on a rhythm nobody agreed to.
fn parse_stored_cadence(stored: &str) -> Result<Cadence> {
    Cadence::parse(stored).ok_or_else(|| {
        StoreError::Db(sqlx::Error::Decode(
            "billing_schedules.cadence is not a known cadence".into(),
        ))
    })
}

/// The rule both ends of the date range share, stated once.
fn dates_agree(start: Date, end: Option<Date>) -> Result<()> {
    if end.is_some_and(|end| end < start) {
        return Err(StoreError::Validation(
            "the arrangement ends before it starts".to_owned(),
        ));
    }
    Ok(())
}

impl AccountStore {
    /// Creates a recurring arrangement with its template lines, in **one**
    /// transaction: either the whole arrangement exists or none of it does.
    ///
    /// The template may not be empty. A schedule with no lines would raise
    /// drafts worth nothing on a rhythm — a standing instruction to produce
    /// litter — so it is refused here rather than discovered next month.
    ///
    /// The start date may be in the past (a subscription agreed at the start of
    /// the month and entered on the fifteenth still bills for that month) but
    /// not by more than [`SCHEDULE_MAX_BACKDATE_DAYS`], and not further ahead
    /// than [`SCHEDULE_MAX_LEAD_DAYS`]. Both are judged against the
    /// **database's** date, read inside the same transaction.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the customer is not this tenant's;
    /// [`StoreError::Validation`] when the customer is archived, a field breaks
    /// its rule, the dates disagree, the start date is out of range, or the
    /// template is empty; [`StoreError::Db`] on failure.
    pub async fn create_billing_schedule(
        &self,
        input: &NewSchedule,
        lines: &[NewLine],
    ) -> Result<BillingScheduleId> {
        if lines.is_empty() {
            return Err(StoreError::Validation(
                "a recurring invoice needs at least one line; it is what gets billed".to_owned(),
            ));
        }
        let customer = self
            .billing_customer(&input.customer_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        if customer.is_archived() {
            return Err(StoreError::Validation(
                "the customer is archived; restore it before billing it again".to_owned(),
            ));
        }
        let name = required("name", &input.name, SCHEDULE_NAME_MAX_CHARS)?;
        let reference = bounded("reference", &input.reference, SCHEDULE_REFERENCE_MAX_CHARS)?;
        let note = bounded("note", &input.note, SCHEDULE_NOTE_MAX_CHARS)?;
        let resolved_currency = match input.currency.as_deref() {
            Some(code) => currency(code)?,
            None => customer.currency,
        };
        let resolved_terms = match input.payment_terms_days {
            Some(days) => payment_terms_days(days)?,
            None => customer.payment_terms_days,
        };
        dates_agree(input.start_date, input.end_date)?;

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // One clock for the whole transaction, and the same clock the row's own
        // timestamps use — never a date the caller sent.
        let today: Date = sqlx::query_scalar("SELECT CURRENT_DATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        let lead = (input.start_date - today).whole_days();
        if lead < -SCHEDULE_MAX_BACKDATE_DAYS {
            return Err(StoreError::Validation(format!(
                "a recurring invoice cannot start more than {SCHEDULE_MAX_BACKDATE_DAYS} days in \
                 the past"
            )));
        }
        if lead > SCHEDULE_MAX_LEAD_DAYS {
            return Err(StoreError::Validation(format!(
                "a recurring invoice cannot start more than {SCHEDULE_MAX_LEAD_DAYS} days from now"
            )));
        }

        let id = BillingScheduleId::generate();
        sqlx::query(
            "INSERT INTO billing_schedules (tenant_id, id, customer_id, name, cadence, \
                 anchor_day, start_date, end_date, next_run_date, currency, payment_terms_days, \
                 reference, note, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $7, $9, $10, $11, $12, $13)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(customer.id.as_str())
        .bind(&name)
        .bind(input.cadence.as_str())
        .bind(i16::from(input.start_date.day()))
        .bind(input.start_date)
        .bind(input.end_date)
        .bind(&resolved_currency)
        .bind(resolved_terms)
        .bind(&reference)
        .bind(&note)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        SCHEDULE_LINES
            .replace(&mut tx, self.tenant.as_str(), id.as_str(), lines)
            .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(id)
    }

    /// The tenant's recurring arrangements, newest first, each with what one
    /// occurrence of it is worth and how many drafts it has raised.
    ///
    /// Three statements whatever the length of the list: the headers, then every
    /// listed arrangement's template lines, then the counts.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_schedules(&self) -> Result<Vec<ScheduleSummary>> {
        let rows = sqlx::query_as::<_, ScheduleRow>(&format!(
            "SELECT {SCHEDULE_COLS} FROM billing_schedules \
             WHERE tenant_id = $1 ORDER BY created_at DESC, id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let figures = sqlx::query_as::<_, FiguresRow>(
            "SELECT schedule_id AS doc_id, qty_milli, unit_price_cents, vat_rate_bp \
             FROM billing_schedule_lines WHERE tenant_id = $1",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_schedule = group_figures(figures);

        let counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT schedule_id, count(*)::bigint FROM billing_invoices \
             WHERE tenant_id = $1 AND schedule_id IS NOT NULL GROUP BY schedule_id",
        )
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        let mut by_count: std::collections::HashMap<String, i64> = counts.into_iter().collect();

        rows.into_iter()
            .map(|row| {
                let lines = by_schedule.remove(&row.id).unwrap_or_default();
                let raised_count = by_count.remove(&row.id).unwrap_or(0);
                Ok(ScheduleSummary {
                    schedule: row.into_schedule()?,
                    totals: totals(&lines),
                    raised_count,
                })
            })
            .collect()
    }

    /// One arrangement of the tenant with its template lines and totals, or
    /// `None` — including when the id belongs to another tenant
    /// (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn billing_schedule(
        &self,
        id: &BillingScheduleId,
    ) -> Result<Option<ScheduleDocument>> {
        let Some(row) = sqlx::query_as::<_, ScheduleRow>(&format!(
            "SELECT {SCHEDULE_COLS} FROM billing_schedules WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?
        else {
            return Ok(None);
        };
        let lines = SCHEDULE_LINES
            .read(&self.pool, self.tenant.as_str(), id.as_str())
            .await?;
        let figures: Vec<LineFigures> = lines.iter().map(Line::figures).collect();
        let raised_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM billing_invoices \
             WHERE tenant_id = $1 AND schedule_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(Some(ScheduleDocument {
            schedule: row.into_schedule()?,
            lines,
            totals: totals(&figures),
            raised_count,
        }))
    }

    /// Edits the parts of an arrangement that stay editable, and optionally
    /// replaces its template lines, in one transaction under the row's lock.
    ///
    /// A run that raced this edit either lands first (and this edit applies on
    /// top of the moved dates) or waits for it — the lock is the same one the
    /// run takes, so a draft is never raised from half a template.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the arrangement is absent or another
    /// tenant's; [`StoreError::Validation`] when a field breaks its rule, the
    /// end date falls before the start, or the template is emptied;
    /// [`StoreError::Db`] on failure.
    pub async fn update_billing_schedule(
        &self,
        id: &BillingScheduleId,
        input: &ScheduleEdit,
        lines: Option<&[NewLine]>,
    ) -> Result<()> {
        let name = required("name", &input.name, SCHEDULE_NAME_MAX_CHARS)?;
        let reference = bounded("reference", &input.reference, SCHEDULE_REFERENCE_MAX_CHARS)?;
        let note = bounded("note", &input.note, SCHEDULE_NOTE_MAX_CHARS)?;
        if lines.is_some_and(<[NewLine]>::is_empty) {
            return Err(StoreError::Validation(
                "a recurring invoice needs at least one line; it is what gets billed".to_owned(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The start date is not editable, so it is the stored one that the end
        // date has to agree with — read under the lock the write goes through.
        // Ending an arrangement before the occurrence it is already committed to
        // is allowed and leaves `next_run_date` beyond the end, which reads as
        // "ended": stopping it early is exactly what that caller meant.
        let locked = self.lock_schedule(&mut tx, id).await?;
        dates_agree(locked.start_date, input.end_date)?;

        sqlx::query(
            "UPDATE billing_schedules SET name = $3, cadence = $4, end_date = $5, \
                 reference = $6, note = $7, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&name)
        .bind(input.cadence.as_str())
        .bind(input.end_date)
        .bind(&reference)
        .bind(&note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        if let Some(lines) = lines {
            SCHEDULE_LINES
                .replace(&mut tx, self.tenant.as_str(), id.as_str(), lines)
                .await?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Pauses or resumes an arrangement.
    ///
    /// A paused one keeps every date it had and is skipped by every run; when it
    /// is resumed it bills the occurrences it missed, because they were still
    /// months the customer was under contract for. Somebody who does not want
    /// them deletes the drafts, which costs nothing — a draft carries no number.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the arrangement is absent or another
    /// tenant's; [`StoreError::Db`] on failure.
    pub async fn set_billing_schedule_active(
        &self,
        id: &BillingScheduleId,
        active: bool,
    ) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE billing_schedules SET active = $3, updated_at = now() \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(active)
        .execute(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        if changed.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Deletes an arrangement that has **never raised anything**, and its
    /// template lines with it.
    ///
    /// One that has raised drafts is refused: the documents point back at it, and
    /// deleting it would either take real invoices with it or erase where they
    /// came from. It is paused instead, which stops it just as completely and
    /// leaves the history readable. The check runs under the row's lock, so a run
    /// that raced this deletion cannot slip a draft in behind it.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the arrangement is absent or another
    /// tenant's; [`StoreError::Conflict`] when it has raised documents;
    /// [`StoreError::Db`] on failure.
    pub async fn delete_billing_schedule(&self, id: &BillingScheduleId) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        self.lock_schedule(&mut tx, id).await?;
        let raised: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM billing_invoices \
             WHERE tenant_id = $1 AND schedule_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if raised > 0 {
            return Err(StoreError::Conflict(
                "this recurring invoice has already raised documents; pause it instead of \
                 deleting it"
                    .to_owned(),
            ));
        }
        sqlx::query("DELETE FROM billing_schedules WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(())
    }

    /// Takes the arrangement's row lock inside `tx` and returns the facts a run
    /// decides against, so a caller can check and then write without any other
    /// transaction slipping in between. Two runs of one arrangement serialise
    /// here, which is what makes an occurrence bill exactly once.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the id is absent **or another tenant's**;
    /// [`StoreError::Db`] on failure or on a cadence the code does not know.
    async fn lock_schedule(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &BillingScheduleId,
    ) -> Result<LockedSchedule> {
        let row: Option<LockedRow> = sqlx::query_as(
            "SELECT customer_id, cadence, anchor_day, start_date, end_date, next_run_date, \
                 active, currency, payment_terms_days, reference, note, created_by \
             FROM billing_schedules WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(LockedSchedule {
            customer_id: row.customer_id,
            cadence: parse_stored_cadence(&row.cadence)?,
            anchor_day: u8::try_from(row.anchor_day).unwrap_or(1),
            start_date: row.start_date,
            end_date: row.end_date,
            next_run_date: row.next_run_date,
            active: row.active,
            currency: row.currency,
            payment_terms_days: row.payment_terms_days,
            reference: row.reference,
            note: row.note,
            created_by: row.created_by,
        })
    }

    /// Runs **one** of this tenant's arrangements as of `today`: raises a draft
    /// for every occurrence that has come due since it last ran, and moves the
    /// next date forward, all under the arrangement's row lock.
    ///
    /// It raises **nothing** — and that is a success, not a refusal — when the
    /// arrangement is paused, has run past its end date, or is simply not due
    /// yet. A run is something that happens on a rhythm, so "there was nothing
    /// to do" is its most common outcome and must not be an error a sweep has to
    /// distinguish from a real one.
    ///
    /// It catches up: three months missed means three drafts, because three
    /// months were billable. At most [`SCHEDULE_MAX_PER_RUN`] per run, so one
    /// call stays bounded; the rest follows on the next run.
    ///
    /// Each draft is a copy of the template — the frozen customer, currency,
    /// terms, reference, note and lines — and carries the date of the occurrence
    /// it is for, which the database holds unique per arrangement.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the arrangement is absent or another
    /// tenant's; [`StoreError::Validation`] when the calendar arithmetic runs
    /// off the supported range; [`StoreError::Db`] on failure.
    pub async fn run_billing_schedule(
        &self,
        id: &BillingScheduleId,
        today: Date,
    ) -> Result<ScheduleRun> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let locked = self.lock_schedule(&mut tx, id).await?;
        let mut due = locked.next_run_date;
        if !locked.active {
            return Ok(ScheduleRun {
                schedule_id: id.clone(),
                raised: Vec::new(),
                next_run_date: due,
            });
        }
        // Read under the same lock as the writes: an edit that raced this run
        // either lands first (and this draft carries the new lines) or waits.
        let template = SCHEDULE_LINES
            .read(&mut *tx, self.tenant.as_str(), id.as_str())
            .await?;

        let mut raised = Vec::new();
        // An empty template is refused at every write path, so this can only be
        // corrupt data; raising empty documents on a rhythm would be the worse
        // answer to it.
        while due <= today
            && raised.len() < SCHEDULE_MAX_PER_RUN
            && locked.end_date.is_none_or(|end| due <= end)
            && !template.is_empty()
        {
            let invoice_id = self
                .insert_invoice_from_schedule(
                    &mut tx,
                    &InvoiceFromSchedule {
                        schedule_id: id.as_str(),
                        due_date: due,
                        customer_id: &locked.customer_id,
                        currency: &locked.currency,
                        payment_terms_days: locked.payment_terms_days,
                        reference: &locked.reference,
                        note: &locked.note,
                        created_by: &locked.created_by,
                    },
                )
                .await?;
            // Copied line by line through the same helper an accepted quote and
            // a credit note use: the frozen values, in the template's order,
            // with an id of their own. Already normalised — a stored line was
            // validated on the way in and the rules are the same on both sides.
            for line in &template {
                INVOICE_LINES
                    .write(
                        &mut tx,
                        self.tenant.as_str(),
                        invoice_id.as_str(),
                        line.line_order,
                        &line.copied(),
                    )
                    .await?;
            }
            raised.push(invoice_id);
            due = next_occurrence(due, locked.cadence, locked.anchor_day).ok_or_else(|| {
                StoreError::Validation(
                    "this arrangement's next date falls outside the supported range".to_owned(),
                )
            })?;
        }

        if !raised.is_empty() {
            sqlx::query(
                "UPDATE billing_schedules SET next_run_date = $3, last_run_date = $4, \
                     updated_at = now() \
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(self.tenant.as_str())
            .bind(id.as_str())
            .bind(due)
            .bind(today)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::Db)?;
        }
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(ScheduleRun {
            schedule_id: id.clone(),
            raised,
            next_run_date: due,
        })
    }

    /// Runs every one of this tenant's arrangements that is due as of `today`,
    /// oldest date first, and answers what each of them did.
    ///
    /// The due list is read without a lock and each arrangement is then run
    /// under its own; one that stopped being due in between simply raises
    /// nothing, which is why [`AccountStore::run_billing_schedule`] treats that
    /// as an ordinary outcome. Arrangements that raise nothing are left out of
    /// the answer — a caller wants to know what appeared, not what did not.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; the first arrangement that fails stops the
    /// run, leaving the ones already run committed (each is its own
    /// transaction).
    pub async fn run_due_billing_schedules(&self, today: Date) -> Result<Vec<ScheduleRun>> {
        let due: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM billing_schedules \
             WHERE tenant_id = $1 AND active AND next_run_date <= $2 \
             ORDER BY next_run_date, id",
        )
        .bind(self.tenant.as_str())
        .bind(today)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let mut runs = Vec::new();
        for id in due {
            let run = self
                .run_billing_schedule(&BillingScheduleId::new(id), today)
                .await?;
            if !run.raised.is_empty() {
                runs.push(run);
            }
        }
        Ok(runs)
    }
}

impl Store {
    /// Runs every tenant's due arrangements as of `today`, and answers how many
    /// drafts were raised in total — the background sweep behind recurring
    /// invoices (B2.11), the mirror of the snooze and share sweeps.
    ///
    /// It asks the cross-tenant question once and then does every piece of work
    /// through the owning tenant's **own account door**, as the schedule's own
    /// owner, so nothing here writes across a tenant boundary and the drafts are
    /// created by the colleague whose standing instruction raised them.
    ///
    /// One arrangement failing does not stop the sweep: the failure is reported
    /// to the caller's log and the next tenant is still served. A sweep that
    /// abandoned every other tenant because one row was unreadable would be a
    /// worse outage than the row.
    ///
    /// # Errors
    /// [`StoreError::Db`] when the due list itself cannot be read.
    pub async fn sweep_billing_schedules(&self, today: Date) -> Result<usize> {
        let due: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id, tenant_id, created_by FROM billing_schedules \
             WHERE active AND next_run_date <= $1 ORDER BY next_run_date, id LIMIT 500",
        )
        .bind(today)
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::Db)?;

        let mut raised = 0;
        for (id, tenant, owner) in due {
            let account = self.for_account(TenantId::new(tenant), UserId::new(owner));
            match account
                .run_billing_schedule(&BillingScheduleId::new(id), today)
                .await
            {
                Ok(run) => raised += run.raised.len(),
                Err(error) => {
                    tracing::warn!(%error, "a recurring invoice could not be run");
                }
            }
        }
        Ok(raised)
    }
}

// ---- row types --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct LockedRow {
    customer_id: String,
    cadence: String,
    anchor_day: i16,
    start_date: Date,
    end_date: Option<Date>,
    next_run_date: Date,
    active: bool,
    currency: String,
    payment_terms_days: i32,
    reference: String,
    note: String,
    created_by: String,
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
    id: String,
    customer_id: String,
    name: String,
    cadence: String,
    anchor_day: i16,
    start_date: Date,
    end_date: Option<Date>,
    next_run_date: Date,
    last_run_date: Option<Date>,
    active: bool,
    currency: String,
    payment_terms_days: i32,
    reference: String,
    note: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl ScheduleRow {
    fn into_schedule(self) -> Result<Schedule> {
        let cadence = parse_stored_cadence(&self.cadence)?;
        Ok(Schedule {
            id: BillingScheduleId::new(self.id),
            customer_id: BillingCustomerId::new(self.customer_id),
            name: self.name,
            cadence,
            // Constrained to 1–31 by the table; a stored value outside it is
            // corrupt data, and the cadence arithmetic clamps it again anyway.
            anchor_day: u8::try_from(self.anchor_day).unwrap_or(1),
            start_date: self.start_date,
            end_date: self.end_date,
            next_run_date: self.next_run_date,
            last_run_date: self.last_run_date,
            active: self.active,
            currency: self.currency,
            payment_terms_days: self.payment_terms_days,
            reference: self.reference,
            note: self.note,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month::{April, February, January, March};

    fn day(year: i32, month: time::Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    /// An arrangement in a given position, for the derived predicates: nothing
    /// else about it matters to them.
    fn standing(next_run: Date, end: Option<Date>, active: bool) -> Schedule {
        Schedule {
            id: BillingScheduleId::new("sch"),
            customer_id: BillingCustomerId::new("cust"),
            name: "Hosting".to_owned(),
            cadence: Cadence::Monthly,
            anchor_day: 1,
            start_date: day(2026, January, 1),
            end_date: end,
            next_run_date: next_run,
            last_run_date: None,
            active,
            currency: "EUR".to_owned(),
            payment_terms_days: 30,
            reference: String::new(),
            note: String::new(),
            created_by: "u".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn an_arrangement_has_ended_when_its_next_date_is_past_its_last_one() {
        assert!(!standing(day(2026, March, 1), None, true).is_ended());
        assert!(!standing(day(2026, March, 1), Some(day(2026, March, 1)), true).is_ended());
        assert!(standing(day(2026, April, 1), Some(day(2026, March, 1)), true).is_ended());
        // Ending is not pausing: an ended arrangement is still `active`, and a
        // reader has to be able to tell "it finished" from "somebody stopped it".
        let ended = standing(day(2026, April, 1), Some(day(2026, March, 1)), true);
        assert!(ended.active && ended.is_ended());
    }

    #[test]
    fn only_an_active_unended_arrangement_that_has_come_due_is_due() {
        let today = day(2026, March, 15);
        assert!(standing(day(2026, March, 1), None, true).is_due(today));
        assert!(
            standing(today, None, true).is_due(today),
            "due today is due"
        );
        assert!(!standing(day(2026, April, 1), None, true).is_due(today));
        assert!(!standing(day(2026, March, 1), None, false).is_due(today));
        assert!(
            !standing(day(2026, March, 1), Some(day(2026, February, 28)), true).is_due(today),
            "past its end date it bills nothing, however overdue the date looks"
        );
    }

    #[test]
    fn a_corrupt_stored_cadence_is_a_decode_failure_not_a_guess() {
        // Never `Validation` (that would blame the caller) and never a guessed
        // rhythm: billing on a rhythm nobody agreed to is worse than not
        // billing.
        match parse_stored_cadence("fortnightly") {
            Err(StoreError::Db(_)) => {}
            other => panic!("expected a decode failure, got {other:?}"),
        }
        assert!(parse_stored_cadence("monthly").is_ok());
    }

    #[test]
    fn an_arrangement_that_ends_before_it_starts_is_refused() {
        let start = day(2026, March, 1);
        assert!(dates_agree(start, None).is_ok());
        assert!(dates_agree(start, Some(start)).is_ok());
        assert!(dates_agree(start, Some(day(2026, April, 1))).is_ok());
        match dates_agree(start, Some(day(2026, February, 28))) {
            Err(StoreError::Validation(message)) => assert!(message.contains("ends before")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}
