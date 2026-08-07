//! The running timer, and the hour a stop writes (alo Projects, ADR 0035, wave
//! B3.04) — one row per person, or none, reached only through their own door.
//!
//! [`crate::time_entries`] holds work that is finished. This holds the clock
//! that is still going, and it is deliberately a **different table**
//! (`docs/design/projects.md`, "The running timer is not an entry"): a
//! `time_entries` row with a null duration would make every aggregate in the
//! module responsible for remembering to exclude the row that is still running,
//! and the one that forgets bills a timer nobody has stopped. Here the module's
//! central rule is the primary key — `(tenant_id, user_id)`, so **a second
//! concurrent start cannot represent itself**, and the conflict a caller sees is
//! a race the database settled rather than a check-then-write that lost one.
//!
//! # Starting while one runs is a refusal, not an implicit stop
//!
//! Stopping a timer writes a billable fact with a duration. A write nobody asked
//! for is not a convenience, so [`AccountStore::start_timer`] returns
//! [`StoreError::Conflict`] and the caller decides. The UI's one button makes
//! two calls, and both are audited.
//!
//! # Stopping is one transaction
//!
//! The clearing of the running row and the writing of the hour stand or fall
//! together: an hour written without the row cleared would be logged twice by
//! the next stop, and a row cleared without its hour written is work silently
//! thrown away. The delete is also the lock — `DELETE … RETURNING` is what
//! claims the timer, so two simultaneous stops produce exactly one entry and one
//! [`StoreError::NotFound`].
//!
//! # A person's timer is personal data
//!
//! `user_id` is bound from the account door on every statement, so reaching a
//! colleague's clock is unrepresentable here rather than merely rejected — the
//! same stance [`crate::time_entries`] takes, for the same GDPR reason. The note
//! a timer carries can name a client or a case, so it never reaches a log: the
//! spans on this path carry ids and minute counts and nothing a human typed.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::billing_field::bounded;
use crate::error::{Result, StoreError};
use crate::id::{ProjectId, TaskId};
use crate::time_entries::{
    MINUTES_MAX, MINUTES_MIN, NOTE_MAX, NewTimeEntry, TimeEntry, insert_entry, project_rate,
};

/// How many seconds make a minute — named because the rounding below is the
/// point, not the arithmetic.
const SECONDS_PER_MINUTE: i64 = 60;

/// What a caller states to start their clock.
#[derive(Debug, Clone)]
pub struct StartTimer {
    /// The board being worked on — a team project, or the caller's own personal
    /// one. Checked for visibility here, at the moment the person says they are
    /// starting.
    pub project_id: ProjectId,
    /// The task inside that project, when they named one. It must live on the
    /// same project, for [`crate::time_entries`]'s reason.
    pub task_id: Option<TaskId>,
    /// Whether the hour will be chargeable. Carried from the start so the entry
    /// a stop writes is complete without a second dialog.
    pub billable: bool,
    /// What they are doing. May be empty; personal data, never logged.
    pub note: String,
}

impl StartTimer {
    /// The minimum a caller must state: a board. Billable, because a client
    /// project's hours are chargeable unless somebody says otherwise.
    #[must_use]
    pub fn on(project_id: ProjectId) -> Self {
        Self {
            project_id,
            task_id: None,
            billable: true,
            note: String::new(),
        }
    }
}

/// A clock that is currently running.
#[derive(Debug, Clone)]
pub struct RunningTimer {
    /// The board it is running against.
    pub project_id: ProjectId,
    /// The task inside it, if one was named.
    pub task_id: Option<TaskId>,
    /// When it started.
    pub started_at: OffsetDateTime,
    /// Whether the hour will be chargeable.
    pub billable: bool,
    /// What the person said they were doing. Personal data: never logged.
    pub note: String,
}

/// What a stop produced: the hour, and whether the clock had to be trimmed to
/// fit a day.
#[derive(Debug, Clone)]
pub struct StoppedTimer {
    /// The entry the stop wrote.
    pub entry: TimeEntry,
    /// The minutes actually elapsed on the wall clock, before the day ceiling —
    /// equal to `entry.minutes` unless [`Self::capped`].
    pub elapsed_minutes: i64,
    /// Whether the clock ran past a full day and the entry was written at
    /// [`MINUTES_MAX`]. Somebody went home without stopping it, and the honest
    /// answer is a full day plus a flag rather than a 22-hour invoice line.
    pub capped: bool,
}

/// Turns an elapsed wall-clock span into minutes, **uncapped**.
///
/// Pure, so the two decisions in it are unit-tested directly rather than through
/// a clock:
///
/// - **Rounds up.** A 30-second stint is one minute, never zero: an entry of
///   zero minutes cannot exist ([`MINUTES_MIN`]), and silently discarding a
///   stint somebody started is worse than rounding thirty seconds up.
/// - **A backwards clock is one minute.** `now` before `started_at` means the
///   host's clock moved, not that work took negative time; refusing the stop
///   would strand the timer forever, and the person still worked.
#[must_use]
pub fn elapsed_minutes(started_at: OffsetDateTime, now: OffsetDateTime) -> i64 {
    let seconds = (now - started_at).whole_seconds().max(0);
    // Ceiling division on non-negative integers; no float on this path, as
    // nowhere on a billing path has one.
    seconds
        .saturating_add(SECONDS_PER_MINUTE - 1)
        .saturating_div(SECONDS_PER_MINUTE)
        .max(MINUTES_MIN)
}

/// Trims an elapsed span to what one entry may hold, saying whether it had to.
///
/// A clock running past a full day means somebody went home without stopping
/// it, and the honest answer is a full day plus a flag rather than a 22-hour
/// invoice line the customer will query.
#[must_use]
pub fn capped_minutes(elapsed: i64) -> (i64, bool) {
    if elapsed > MINUTES_MAX {
        (MINUTES_MAX, true)
    } else {
        (elapsed, false)
    }
}

impl AccountStore {
    /// The caller's own running timer, or `None`.
    ///
    /// A colleague's clock is not addressable through this door, so there is no
    /// id to pass and nothing to deny.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn running_timer(&self) -> Result<Option<RunningTimer>> {
        let row = sqlx::query_as::<_, TimerRow>(
            "SELECT project_id, task_id, started_at, billable, note FROM time_timers \
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        Ok(row.map(TimerRow::into_timer))
    }

    /// Starts the caller's clock on a project.
    ///
    /// The board's visibility is checked here — this is the moment the person
    /// says they are starting work, and an hour is logged against a board
    /// somebody can open. A board they cannot see reads as absent, never as a
    /// refusal that would confirm it exists.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when a timer is already running — the caller
    /// stops it or leaves it, and this never decides for them;
    /// [`StoreError::NotFound`] when the project or the task is not one the
    /// caller can see; [`StoreError::Validation`] when the task is on another
    /// project or the note is too long; [`StoreError::Db`] on failure.
    pub async fn start_timer(&self, start: &StartTimer) -> Result<RunningTimer> {
        self.writable_project(&start.project_id).await?;
        self.require_task_on_project(start.task_id.as_ref(), &start.project_id)
            .await?;
        let note = bounded("note", &start.note, NOTE_MAX)?;
        // ON CONFLICT DO NOTHING rather than a read-then-write: the primary key
        // is the rule, so the row that is not inserted is the race being lost,
        // and the answer to it is read back below rather than guessed at.
        let row = sqlx::query_as::<_, TimerRow>(
            "INSERT INTO time_timers (tenant_id, user_id, project_id, task_id, billable, note) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (tenant_id, user_id) DO NOTHING \
             RETURNING project_id, task_id, started_at, billable, note",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .bind(start.project_id.as_str())
        .bind(start.task_id.as_ref().map(TaskId::as_str))
        .bind(start.billable)
        .bind(note)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::Db)?;
        match row {
            Some(row) => Ok(row.into_timer()),
            None => Err(StoreError::Conflict(
                "a timer is already running; stop it before starting another".to_owned(),
            )),
        }
    }

    /// Stops the caller's clock and writes the hour it counted.
    ///
    /// `work_date` is the day the person says the work belongs to, stated in
    /// **their** zone — a session stopped at 00:30 in Berlin usually belongs to
    /// the previous working day, and a timesheet whose week boundary moves with
    /// the server's zone is one an employee will dispute. `None` falls back to
    /// the UTC day the clock **started**, which is the closest thing to a fact
    /// this side of the wire knows; every caller that has a user in front of it
    /// states the day.
    ///
    /// The rate is snapshotted now, from the engagement's facts, exactly as a
    /// manual entry's is — and without the visibility check, because an hour
    /// already worked is not un-worked by the board having been archived while
    /// the clock ran ([`project_rate`]).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when no timer is running, or the project has
    /// been deleted under it; [`StoreError::Validation`] when the engagement's
    /// rate cannot be expressed; [`StoreError::Db`] on failure.
    pub async fn stop_timer(&self, work_date: Option<Date>) -> Result<StoppedTimer> {
        let mut tx = self.pool.begin().await.map_err(StoreError::Db)?;
        // The delete IS the claim: two simultaneous stops, one entry.
        let row = sqlx::query_as::<_, TimerRow>(
            "DELETE FROM time_timers WHERE tenant_id = $1 AND user_id = $2 \
             RETURNING project_id, task_id, started_at, billable, note",
        )
        .bind(self.tenant.as_str())
        .bind(self.user.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
        let timer = row.ok_or(StoreError::NotFound)?.into_timer();

        let elapsed = elapsed_minutes(timer.started_at, OffsetDateTime::now_utc());
        let (minutes, capped) = capped_minutes(elapsed);

        let project = project_rate(&mut tx, self.tenant.as_str(), &timer.project_id).await?;
        let mut new = NewTimeEntry::worked(
            timer.project_id.clone(),
            work_date.unwrap_or_else(|| timer.started_at.date()),
            minutes,
        );
        new.task_id = timer.task_id.clone();
        // Provenance, never a period boundary: the day above is what the week,
        // the report and the unbilled cut-off use.
        new.started_at = Some(timer.started_at);
        new.billable = timer.billable;
        new.note = timer.note.clone();
        let entry = insert_entry(
            &mut tx,
            self.tenant.as_str(),
            self.user.as_str(),
            &new,
            &project,
        )
        .await?;
        tx.commit().await.map_err(StoreError::Db)?;
        Ok(StoppedTimer {
            entry,
            elapsed_minutes: elapsed,
            capped,
        })
    }
}

// ---- row type ---------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TimerRow {
    project_id: String,
    task_id: Option<String>,
    started_at: OffsetDateTime,
    billable: bool,
    note: String,
}

impl TimerRow {
    fn into_timer(self) -> RunningTimer {
        RunningTimer {
            project_id: ProjectId::new(self.project_id),
            task_id: self.task_id.map(TaskId::new),
            started_at: self.started_at,
            billable: self.billable,
            note: self.note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    fn start() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }

    fn after(seconds: i64) -> i64 {
        elapsed_minutes(start(), start() + Duration::seconds(seconds))
    }

    #[test]
    fn a_stint_shorter_than_a_minute_is_one_minute() {
        for seconds in [0, 1, 30, 59, 60] {
            assert_eq!(after(seconds), 1, "{seconds}s");
        }
    }

    #[test]
    fn a_part_minute_rounds_up() {
        assert_eq!(after(61), 2);
        assert_eq!(after(119), 2);
        assert_eq!(after(120), 2);
        assert_eq!(after(121), 3);
    }

    #[test]
    fn a_clock_that_went_backwards_is_one_minute_not_a_refusal() {
        assert_eq!(
            elapsed_minutes(start(), start() - Duration::hours(3)),
            MINUTES_MIN
        );
    }

    #[test]
    fn a_day_is_the_ceiling_and_it_says_so() {
        assert_eq!(
            capped_minutes(MINUTES_MAX),
            (MINUTES_MAX, false),
            "exactly a day is not yet over the ceiling"
        );
        assert_eq!(capped_minutes(MINUTES_MAX + 1), (MINUTES_MAX, true));
        assert_eq!(capped_minutes(30 * 60), (MINUTES_MAX, true));
        assert_eq!(capped_minutes(120), (120, false));
    }

    #[test]
    fn a_clock_left_running_over_a_weekend_still_reports_what_it_ran() {
        // The entry is a day; the answer says the clock ran three, which is what
        // tells the person their Friday timer was never stopped.
        let elapsed = elapsed_minutes(start(), start() + Duration::days(3));
        assert_eq!(elapsed, 3 * 24 * 60);
        assert_eq!(capped_minutes(elapsed), (MINUTES_MAX, true));
    }
}
