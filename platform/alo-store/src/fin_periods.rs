//! Fiscal periods and the soft close (alo Finance, ADR 0035, wave B4.10;
//! `docs/design/finance.md`, "Fiscal periods and the soft close").
//!
//! A ledger everybody can still write into is a ledger nobody can report from:
//! the VAT return filed on Monday stops matching the books on Tuesday because
//! somebody dated a receipt into last quarter. This module is the rule that
//! stops that — and the one way to lift it again.
//!
//! # The lock date is derived, never stored
//!
//! A tenant's periods are rows; the lock date is `max(to_date)` over the
//! **closed** ones ([`AccountStore::fin_lock_date`]). Every write to the books
//! reads it — [`AccountStore::post_fin_entry`] refuses an entry dated on or
//! before it — so a document act that would have posted into a closed quarter
//! is refused whole rather than half-completed with a floating entry.
//!
//! Storing the lock date beside the periods was rejected for the reason every
//! stored derivation is: it is a second answer, and the day it disagrees with
//! the first is the day nobody can tell which one the books obeyed.
//!
//! # Closed periods are a prefix, and that is enforced
//!
//! Because the lock date is a maximum, closing Q3 while Q2 is open would shut
//! Q2 too, silently. So a close refuses while any earlier period is still open,
//! and a reopen refuses while any later period is still closed. Together they
//! keep "the books are closed through X" literally true, which is the sentence
//! every refusal in this module says out loud.
//!
//! # The close is soft
//!
//! An admin reopens a period by saying why (the reason is required, like the
//! bank line's dismissal in [`crate::bank_ignore`]), posts what was missing, and
//! closes it again. Every one of those acts is audited by the `/finance/*`
//! middleware (B2.13), and an entry written after a reopen carries a
//! `created_at` later than the close it follows — so a reader can always see
//! that a reported period was touched again.
//!
//! *Rejected: a hard close.* A small business finds a missing receipt in week
//! three of every quarter. A lock nobody can lift is one people work around by
//! backdating into the open period, which corrupts two periods instead of
//! admitting to one.
//!
//! # What this module does not promise
//!
//! A posting that is **already in flight** when a close commits still lands: the
//! journal reads the lock date inside its own transaction, and serialising every
//! posting against every close would make the books' hot path queue behind an
//! administrative act taken four times a year. The close is a rule about writes
//! that start after it, and `created_at` tells that story honestly.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};
use crate::id::{FinPeriodId, UserId};

/// The longest note or reopen reason kept — a sentence, not a memo. Matches the
/// column bound in migration 0146.
pub const PERIOD_NOTE_MAX_CHARS: usize = 200;

/// The longest span one period may cover, in days. A fiscal year is 365 or 366
/// days; anything longer is a mistyped year in a date field, and it would shut
/// the books for a decade the first time somebody closed it.
pub const PERIOD_MAX_DAYS: i64 = 366;

/// The most periods one tenant may define — a century of quarters. A bound
/// exists so a scripted caller cannot turn the picker into a scroll.
pub const PERIODS_MAX: i64 = 400;

/// Whether a period still accepts postings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodStatus {
    /// Open: entries may be dated into it.
    Open,
    /// Closed: reported, and shut to new entries until an admin reopens it.
    Closed,
}

impl PeriodStatus {
    /// The stored word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Reads the stored word back.
    ///
    /// # Errors
    /// [`StoreError::Validation`] naming the accepted set. A word this build
    /// does not know is a schema disagreement, and failing on it is honest
    /// where guessing would report a closed period as open.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            _ => Err(StoreError::Validation(
                "a period's status must be open or closed".to_owned(),
            )),
        }
    }
}

/// One fiscal period of a tenant.
#[derive(Debug, Clone)]
pub struct FinPeriod {
    /// Opaque id, unique within the tenant.
    pub id: FinPeriodId,
    /// First day of the period, inclusive.
    pub from_date: Date,
    /// Last day of the period, inclusive.
    pub to_date: Date,
    /// Open or closed.
    pub status: PeriodStatus,
    /// Who closed it, while it is closed.
    pub closed_by: Option<UserId>,
    /// When they closed it, while it is closed.
    pub closed_at: Option<OffsetDateTime>,
    /// The note of the current state: what the closer said, or why it was
    /// reopened. Empty when nobody said anything.
    pub note: String,
    /// When the period was defined.
    pub created_at: OffsetDateTime,
}

/// How far the books are shut, and the close that shut them — everything a
/// refusal needs to name (`docs/design/finance.md`: "`409` naming the period and
/// its close date").
#[derive(Debug, Clone)]
pub struct ClosedThrough {
    /// The closed period with the latest end: its first day,
    pub from_date: Date,
    /// its last day — **the lock date**,
    pub to_date: Date,
    /// and the day it was closed on.
    pub closed_on: Date,
}

impl ClosedThrough {
    /// The refusal a write into shut books gets, naming the period, the day the
    /// books are closed through and the day somebody closed them.
    ///
    /// One sentence in one place so the journal, a document act and a screen
    /// cannot each explain the lock differently. Dates only — a lock is not
    /// personal data (law 1).
    pub fn refusal(&self, entry_date: Date) -> String {
        format!(
            "the books are closed through {}: an entry dated {} falls in the period {} – {}, \
             which was closed on {}. Reopen that period to post into it.",
            self.to_date, entry_date, self.from_date, self.to_date, self.closed_on
        )
    }
}

/// Validates a note or a reopen reason: trimmed, and within the stored bound.
fn note_text(field: &str, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > PERIOD_NOTE_MAX_CHARS {
        return Err(StoreError::Validation(format!(
            "{field} may be at most {PERIOD_NOTE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

/// The shape rules of a new period, with no database in sight: both ends the
/// right way round, and a span that is a period rather than a mistyped year.
fn period_span(from_date: Date, to_date: Date) -> Result<()> {
    if to_date < from_date {
        return Err(StoreError::Validation(
            "a period ends on or after the day it starts".to_owned(),
        ));
    }
    let days = (to_date - from_date).whole_days() + 1;
    if days > PERIOD_MAX_DAYS {
        return Err(StoreError::Validation(format!(
            "a fiscal period covers at most {PERIOD_MAX_DAYS} days; that one covers {days}"
        )));
    }
    Ok(())
}

impl AccountStore {
    /// Every period of this tenant, oldest first — the picker, and the answer
    /// to "is Q2 closed?".
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] when a stored
    /// status is a word this build does not know.
    pub async fn fin_periods(&self) -> Result<Vec<FinPeriod>> {
        let rows = sqlx::query_as::<_, PeriodRow>(&format!(
            "SELECT {PERIOD_COLS} FROM fin_periods WHERE tenant_id = $1 \
             ORDER BY from_date, to_date, id"
        ))
        .bind(self.tenant.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        rows.into_iter().map(PeriodRow::into_period).collect()
    }

    /// One period of this tenant, or `None` — including when the id is another
    /// tenant's (indistinguishable by design).
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure; [`StoreError::Validation`] on an unknown
    /// stored status.
    pub async fn fin_period(&self, id: &FinPeriodId) -> Result<Option<FinPeriod>> {
        let row = sqlx::query_as::<_, PeriodRow>(&format!(
            "SELECT {PERIOD_COLS} FROM fin_periods WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        row.map(PeriodRow::into_period).transpose()
    }

    /// The day this tenant's books are shut through, or `None` while nothing is
    /// closed. **The derived lock date** — see the module header.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn fin_lock_date(&self) -> Result<Option<Date>> {
        Ok(self
            .fin_closed_through_on(&self.pool)
            .await?
            .map(|closed| closed.to_date))
    }

    /// [`AccountStore::fin_lock_date`] with the close that produced it, against
    /// any executor — so the journal can ask it **inside** the transaction it is
    /// about to write in, and a close committing next door cannot slip between
    /// the question and the answer.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub(crate) async fn fin_closed_through_on<'e, E>(
        &self,
        executor: E,
    ) -> Result<Option<ClosedThrough>>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row: Option<(Date, Date, OffsetDateTime)> = sqlx::query_as(
            "SELECT from_date, to_date, closed_at FROM fin_periods \
             WHERE tenant_id = $1 AND status = 'closed' AND closed_at IS NOT NULL \
             ORDER BY to_date DESC LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(|(from_date, to_date, closed_at)| ClosedThrough {
            from_date,
            to_date,
            closed_on: closed_at.date(),
        }))
    }

    /// Defines a period. It starts **open**: defining a quarter and reporting on
    /// it are two acts by two people on two days.
    ///
    /// Serialised per tenant on the tenant's own row, because the rule being
    /// enforced is about the whole set — two admins each adding a quarter at the
    /// same moment must not be able to write two overlapping ones between each
    /// other's checks. A period defined four times a year can afford one lock.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the ends are the wrong way round or the
    /// span is longer than [`PERIOD_MAX_DAYS`]; [`StoreError::Conflict`] when it
    /// overlaps a period that exists, when it would sit inside books that are
    /// already closed, or when the tenant already has [`PERIODS_MAX`] periods;
    /// [`StoreError::Db`] on failure.
    pub async fn create_fin_period(&self, from_date: Date, to_date: Date) -> Result<FinPeriod> {
        period_span(from_date, to_date)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;

        // The set-wide rules below are only true if nobody else is writing the
        // set while we read it.
        sqlx::query("SELECT 1 FROM tenants WHERE id = $1 FOR UPDATE")
            .bind(self.tenant.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StoreError::Db)?;

        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM fin_periods WHERE tenant_id = $1")
                .bind(self.tenant.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(StoreError::Db)?;
        if count >= PERIODS_MAX {
            return Err(StoreError::Conflict(format!(
                "a tenant may define at most {PERIODS_MAX} fiscal periods"
            )));
        }

        // Inclusive ends on both sides, so touching is fine and one shared day
        // is not: 1 Jan – 31 Mar and 1 Apr – 30 Jun are neighbours, 31 Mar –
        // 30 Jun overlaps the first.
        let clash: Option<(Date, Date)> = sqlx::query_as(
            "SELECT from_date, to_date FROM fin_periods \
             WHERE tenant_id = $1 AND from_date <= $2 AND to_date >= $3 \
             ORDER BY from_date LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .bind(to_date)
        .bind(from_date)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some((clash_from, clash_to)) = clash {
            return Err(StoreError::Conflict(format!(
                "that period overlaps {clash_from} – {clash_to}, which already exists"
            )));
        }

        // A period wholly inside shut books would show as open and accept
        // nothing — a screen telling a bookkeeper to post where the journal
        // will refuse them. It is refused here instead, where the reason can be
        // said.
        if let Some(closed) = self.fin_closed_through_on(&mut *tx).await?
            && to_date <= closed.to_date
        {
            return Err(StoreError::Conflict(format!(
                "the books are closed through {}; a period ending {} would sit inside them",
                closed.to_date, to_date
            )));
        }

        let id = FinPeriodId::generate();
        sqlx::query(
            "INSERT INTO fin_periods (tenant_id, id, from_date, to_date, status, created_by) \
             VALUES ($1, $2, $3, $4, 'open', $5)",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(from_date)
        .bind(to_date)
        .bind(self.user.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_period_conflict)?;

        let row = self.period_row_in(&mut tx, &id).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_period()
    }

    /// Closes a period: the books are shut through its last day, and every
    /// entry dated on or before that day is refused until somebody reopens it.
    ///
    /// The note is optional and says what the close covered ("filed, VAT return
    /// sent"). Who closed it and when are the period's own state; the audit log
    /// keeps the history.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the period is absent or another tenant's;
    /// [`StoreError::Conflict`] when it is already closed or an earlier period
    /// is still open; [`StoreError::Validation`] when the note is too long;
    /// [`StoreError::Db`] on failure.
    pub async fn close_fin_period(&self, id: &FinPeriodId, note: &str) -> Result<FinPeriod> {
        let note = note_text("a closing note", note)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let current = self.hold_period(&mut tx, id).await?;
        if current.status == PeriodStatus::Closed {
            return Err(StoreError::Conflict(format!(
                "the period {} – {} is already closed",
                current.from_date, current.to_date
            )));
        }

        // Closing out of order would shut the period before it by arithmetic
        // rather than by anybody's decision (module header).
        let earlier: Option<(Date, Date)> = sqlx::query_as(
            "SELECT from_date, to_date FROM fin_periods \
             WHERE tenant_id = $1 AND status = 'open' AND id <> $2 AND to_date < $3 \
             ORDER BY to_date DESC LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(current.to_date)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some((open_from, open_to)) = earlier {
            return Err(StoreError::Conflict(format!(
                "close the periods in order: {open_from} – {open_to} is still open, and closing \
                 this one would shut it too"
            )));
        }

        sqlx::query(
            "UPDATE fin_periods SET status = 'closed', closed_by = $3, closed_at = now(), \
                 note = $4 \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(self.user.as_str())
        .bind(&note)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let row = self.period_row_in(&mut tx, id).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_period()
    }

    /// Reopens a closed period, with the reason it had to be reopened.
    ///
    /// The reason is **required**. A period that was reported and is open again
    /// is the one state an accountant has to be able to explain six months
    /// later, and the person who has the answer is the one clicking the button,
    /// now. It replaces the closing note: a period carries the note of the state
    /// it is in, and the audit log carries the history.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the period is absent or another tenant's;
    /// [`StoreError::Validation`] when the reason is blank or too long;
    /// [`StoreError::Conflict`] when the period is not closed or a later period
    /// is still closed; [`StoreError::Db`] on failure.
    pub async fn reopen_fin_period(&self, id: &FinPeriodId, reason: &str) -> Result<FinPeriod> {
        let reason = note_text("a reopening reason", reason)?;
        if reason.is_empty() {
            return Err(StoreError::Validation(
                "say why this period is being reopened; books that were reported and are open \
                 again is the one state an accountant has to be able to explain"
                    .to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        let current = self.hold_period(&mut tx, id).await?;
        if current.status == PeriodStatus::Open {
            return Err(StoreError::Conflict(format!(
                "the period {} – {} is not closed",
                current.from_date, current.to_date
            )));
        }

        // Reopening out of order would leave this period open underneath a lock
        // date that still shuts it — open on the screen, refused by the journal.
        let later: Option<(Date, Date)> = sqlx::query_as(
            "SELECT from_date, to_date FROM fin_periods \
             WHERE tenant_id = $1 AND status = 'closed' AND id <> $2 AND to_date > $3 \
             ORDER BY to_date LIMIT 1",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(current.to_date)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        if let Some((closed_from, closed_to)) = later {
            return Err(StoreError::Conflict(format!(
                "reopen the periods newest first: {closed_from} – {closed_to} is still closed, \
                 and it would keep this one shut anyway"
            )));
        }

        sqlx::query(
            "UPDATE fin_periods SET status = 'open', closed_by = NULL, closed_at = NULL, \
                 note = $3 \
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .bind(&reason)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        let row = self.period_row_in(&mut tx, id).await?;
        tx.commit().await.map_err(StoreError::Db)?;
        row.into_period()
    }

    /// Reads the three fields a decision turns on, under a row lock, so the
    /// decision is taken against what is stored and not against what was stored
    /// a moment ago. `NotFound` for another tenant's id, like every other read
    /// here.
    async fn hold_period(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &FinPeriodId,
    ) -> Result<HeldPeriod> {
        let row: Option<(String, Date, Date)> = sqlx::query_as(
            "SELECT status, from_date, to_date FROM fin_periods \
             WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?;
        let (status, from_date, to_date) = row.ok_or(StoreError::NotFound)?;
        Ok(HeldPeriod {
            status: PeriodStatus::parse(&status)?,
            from_date,
            to_date,
        })
    }

    /// Re-reads the whole row inside the transaction that just wrote it, so the
    /// caller is answered with what the database holds rather than with what we
    /// believe we sent it.
    async fn period_row_in(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: &FinPeriodId,
    ) -> Result<PeriodRow> {
        sqlx::query_as::<_, PeriodRow>(&format!(
            "SELECT {PERIOD_COLS} FROM fin_periods WHERE tenant_id = $1 AND id = $2"
        ))
        .bind(self.tenant.as_str())
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::Db)?
        .ok_or(StoreError::NotFound)
    }
}

/// Turns the one uniqueness violation this table can raise into the conflict
/// that names what happened. Two periods starting on one day are the same
/// period entered twice, most likely by a double-clicked button.
fn map_period_conflict(error: sqlx::Error) -> StoreError {
    match error {
        sqlx::Error::Database(ref db) if db.code().as_deref() == Some("23505") => {
            StoreError::Conflict("a period already starts on that day".to_owned())
        }
        other => StoreError::Db(other),
    }
}

/// A period held under its row lock: only what a close or a reopen decides on.
/// Deliberately not a [`FinPeriod`] — half a record with the other half filled
/// in from nowhere is how a placeholder ends up in an answer.
struct HeldPeriod {
    status: PeriodStatus,
    from_date: Date,
    to_date: Date,
}

/// The stored columns of a period, in the order [`PeriodRow`] reads them.
const PERIOD_COLS: &str = "id, from_date, to_date, status, closed_by, closed_at, note, created_at";

#[derive(sqlx::FromRow)]
struct PeriodRow {
    id: String,
    from_date: Date,
    to_date: Date,
    status: String,
    closed_by: Option<String>,
    closed_at: Option<OffsetDateTime>,
    note: String,
    created_at: OffsetDateTime,
}

impl PeriodRow {
    fn into_period(self) -> Result<FinPeriod> {
        Ok(FinPeriod {
            id: FinPeriodId::new(self.id),
            from_date: self.from_date,
            to_date: self.to_date,
            status: PeriodStatus::parse(&self.status)?,
            closed_by: self.closed_by.map(UserId::new),
            closed_at: self.closed_at,
            note: self.note,
            created_at: self.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("a real day")
    }

    #[test]
    fn a_period_ends_on_or_after_it_starts() {
        let err = period_span(day(2026, Month::March, 31), day(2026, Month::January, 1))
            .expect_err("backwards");
        assert!(matches!(err, StoreError::Validation(ref m) if m.contains("on or after")));
    }

    #[test]
    fn one_day_and_one_leap_year_are_both_periods() {
        let one = day(2026, Month::March, 31);
        period_span(one, one).expect("a single day is a period");
        period_span(day(2024, Month::January, 1), day(2024, Month::December, 31))
            .expect("a leap year is 366 days");
    }

    #[test]
    fn a_span_longer_than_a_year_is_a_mistyped_date() {
        // A common year plus its anniversary is 366 days, and legal — a fiscal
        // year does not have to start in January. One day more is a typo.
        period_span(day(2026, Month::January, 1), day(2027, Month::January, 1))
            .expect("366 days is a period");
        let err = period_span(day(2026, Month::January, 1), day(2027, Month::January, 2))
            .expect_err("367 days");
        assert!(matches!(err, StoreError::Validation(ref m) if m.contains("covers 367")));
    }

    #[test]
    fn a_note_is_trimmed_and_bounded() {
        assert_eq!(note_text("a note", "  filed  ").expect("fits"), "filed");
        let long = "x".repeat(PERIOD_NOTE_MAX_CHARS + 1);
        assert!(note_text("a note", &long).is_err());
    }

    #[test]
    fn the_refusal_names_the_period_the_lock_date_and_the_close() {
        let closed = ClosedThrough {
            from_date: day(2026, Month::January, 1),
            to_date: day(2026, Month::March, 31),
            closed_on: day(2026, Month::April, 12),
        };
        let message = closed.refusal(day(2026, Month::February, 9));
        assert!(message.contains("closed through 2026-03-31"));
        assert!(message.contains("dated 2026-02-09"));
        assert!(message.contains("2026-01-01 – 2026-03-31"));
        assert!(message.contains("closed on 2026-04-12"));
    }

    #[test]
    fn a_status_reads_back_and_an_unknown_word_fails() {
        assert_eq!(
            PeriodStatus::parse("closed").expect("known"),
            PeriodStatus::Closed
        );
        assert!(PeriodStatus::parse("half-open").is_err());
    }
}
