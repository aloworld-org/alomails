//! How much a tenant may send today, and why that is the number (alo
//! Campaigns, ADR 0044, queue item C2.3).
//!
//! Design: `docs/design/sending-reputation-warm-up.md`. Schema: migration 0801.
//!
//! A new sending identity has no reputation, and a receiver's first impression
//! of one is formed by volume before it is formed by content. Sending ten
//! thousand messages on day one from an address nobody has seen is the single
//! most reliable way to be filtered — not because the mail is bad, but because
//! that is what a compromised host looks like. So volume climbs on a schedule,
//! and the schedule is enforced here rather than remembered by an operator.
//!
//! ## The ceiling is computed, never stored
//!
//! The table holds a **start date** and nothing else that matters. The obvious
//! alternative — a `daily_cap` column somebody may raise — is the wrong shape:
//! that number would be edited by whoever is impatient on the day a campaign is
//! ready, and it would carry no record of what it was derived from. A date is a
//! fact about the past. Raising the limit means back-dating a warm-up, which is
//! a thing somebody has to lie about deliberately rather than nudge.
//!
//! ## Every figure is a ceiling, never a target
//!
//! A day with nothing worth sending sends nothing. Skipping a day costs far
//! less than sending filler to hit a number — filler is what a complaint rate
//! is made of, and the complaint rate is the thing the whole schedule exists to
//! protect.

use time::{Date, OffsetDateTime};

use crate::account::AccountStore;
use crate::error::{Result, StoreError};

/// The published schedule, as `(through day, ceiling)` — the table in
/// `docs/design/sending-reputation-warm-up.md`, in code so the two cannot
/// drift.
///
/// Day 1 is the start date itself.
const SCHEDULE: [(i64, i64); 5] = [(3, 5), (7, 20), (14, 100), (21, 500), (28, 2_000)];

/// What the last banded day allows, and the base the doubling starts from.
const SETTLED_CEILING: i64 = 2_000;

/// The last day the bands cover; past it the ceiling doubles weekly.
const SETTLED_DAY: i64 = 28;

/// A ceiling past which the schedule stops meaning anything.
///
/// The design says the doubling runs "indefinitely, until the volume matches
/// the actual audience", which is true of the intent and useless as
/// arithmetic — doubling weekly overflows an `i64` in under two years. This is
/// the point at which a warm-up has plainly finished: a sender doing a million
/// a day is not warming up, and anything beyond it is the audience's size
/// rather than the schedule's business.
pub const WARM_UP_COMPLETE_AT: i64 = 1_000_000;

/// How much may go out today, and the sentence that explains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendAllowance {
    /// Which day of the warm-up today is. Day 1 is the start date.
    pub day: i64,
    /// The most that may be sent today.
    pub ceiling: i64,
    /// How many have already gone out today, across every send this tenant is
    /// running — the ceiling is the identity's, not one campaign's.
    pub sent_today: i64,
    /// What is left. Never negative: a ceiling lowered under a day's existing
    /// traffic leaves nothing rather than a debt.
    pub remaining: i64,
}

impl SendAllowance {
    /// Whether anything at all may go out.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.remaining == 0
    }

    /// Whether the schedule has stopped constraining this identity.
    #[must_use]
    pub fn warm_up_finished(&self) -> bool {
        self.ceiling >= WARM_UP_COMPLETE_AT
    }
}

/// The ceiling for one day of a warm-up. Day 1 is the first day.
///
/// Pure, and the reason this module is testable without a clock or a database:
/// every judgement about volume reduces to this function.
///
/// Day 0 or earlier is zero rather than the first band. A warm-up that has not
/// begun is not a warm-up at the tightest setting — it is an identity with no
/// start date, and sending from one is the thing the schedule forbids.
#[must_use]
pub fn ceiling_for_day(day: i64) -> i64 {
    if day <= 0 {
        return 0;
    }
    for (through, ceiling) in SCHEDULE {
        if day <= through {
            return ceiling;
        }
    }
    // Past the bands: double each further week, saturating rather than
    // overflowing. `(day - 29) / 7` is whole weeks completed since the doubling
    // began, so day 29 is one doubling of the settled ceiling.
    let weeks = (day - SETTLED_DAY - 1) / 7 + 1;
    let doubled = SETTLED_CEILING
        .saturating_mul(2_i64.saturating_pow(u32::try_from(weeks).unwrap_or(u32::MAX).min(32)));
    doubled.min(WARM_UP_COMPLETE_AT)
}

/// Which day of a warm-up `today` is, counting the start date as day 1.
///
/// Negative when the start is in the future, which the table's own `CHECK`
/// forbids — kept total anyway, because a function that panics on a value the
/// database merely promises not to hold is a function that panics one migration
/// later.
#[must_use]
pub fn day_of_warm_up(started_on: Date, today: Date) -> i64 {
    (today - started_on).whole_days() + 1
}

impl AccountStore {
    /// Records when this tenant's sending identity started warming.
    ///
    /// Idempotent on the date: recording the same start twice is the same
    /// fact, and the first record's timestamp stands. Recording a *different*
    /// start replaces it — back-dating a warm-up is a deliberate act and the
    /// row says who did it.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the date is in the future — the ceiling
    /// on day zero is the tightest there is, so a future start would silently
    /// mean "send nothing", a limit nobody set and nobody could explain.
    /// [`StoreError::Db`] on failure.
    pub async fn record_campaign_warm_up_start(&self, started_on: Date) -> Result<()> {
        let today = OffsetDateTime::now_utc().date();
        if started_on > today {
            return Err(StoreError::Validation(
                "a warm-up cannot start in the future — until then the identity may send nothing, \
                 which is a limit nobody set"
                    .to_owned(),
            ));
        }
        sqlx::query(UPSERT_SQL)
            .bind(self.tenant.as_str())
            .bind(started_on)
            .bind(self.user.as_str())
            .execute(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(())
    }

    /// When this tenant's warm-up started, or `None` if nobody has recorded it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_warm_up_start(&self) -> Result<Option<Date>> {
        let row: Option<(Date,)> = sqlx::query_as(SELECT_SQL)
            .bind(self.tenant.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::Db)?;
        Ok(row.map(|r| r.0))
    }

    /// How much this tenant may send today, and why.
    ///
    /// **A tenant with no recorded warm-up may send nothing**, and that is the
    /// safe direction rather than an oversight: an identity nobody has started
    /// warming is one nobody has checked the DNS of, and the failure of sending
    /// from it is measured in months of deliverability rather than in a refused
    /// request.
    ///
    /// The count is across every send this tenant is running, because the
    /// ceiling belongs to the sending identity and not to one campaign. Two
    /// campaigns going out on the same day share it.
    ///
    /// # Errors
    /// [`StoreError::Db`] on failure.
    pub async fn campaign_send_allowance(&self) -> Result<SendAllowance> {
        let today = OffsetDateTime::now_utc().date();
        let started = self.campaign_warm_up_start().await?;
        let day = started.map_or(0, |start| day_of_warm_up(start, today));
        let ceiling = ceiling_for_day(day);

        let (sent_today,): (i64,) = sqlx::query_as(SENT_TODAY_SQL)
            .bind(self.tenant.as_str())
            .fetch_one(&self.pool)
            .await
            .map_err(StoreError::Db)?;

        Ok(SendAllowance {
            day,
            ceiling,
            sent_today,
            // Saturating: a ceiling that dropped below a day's existing traffic
            // leaves nothing to send, not a negative debt to carry.
            remaining: (ceiling - sent_today).max(0),
        })
    }
}

const UPSERT_SQL: &str = "INSERT INTO campaign_warm_up (tenant_id, started_on, recorded_by) \
     VALUES ($1, $2, $3) \
     ON CONFLICT (tenant_id) DO UPDATE \
       SET started_on = EXCLUDED.started_on, \
           recorded_by = EXCLUDED.recorded_by, \
           recorded_at = now() \
     WHERE campaign_warm_up.started_on <> EXCLUDED.started_on";

const SELECT_SQL: &str = "SELECT started_on FROM campaign_warm_up WHERE tenant_id = $1";

/// Everything this tenant has actually put on the wire today, whichever send it
/// belonged to. `settled_at` is when the dispatcher moved the row, which is the
/// moment the message was handed to submission.
const SENT_TODAY_SQL: &str = "SELECT count(*) FROM campaign_send_recipients \
     WHERE tenant_id = $1 AND state = 'sent' \
       AND settled_at >= (now() AT TIME ZONE 'UTC')::date";

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use time::Month;

    #[test]
    fn the_bands_are_the_ones_the_design_published() {
        // `docs/design/sending-reputation-warm-up.md`, in code so the two
        // cannot drift. Each band is checked at both ends.
        assert_eq!(ceiling_for_day(1), 5);
        assert_eq!(ceiling_for_day(3), 5);
        assert_eq!(ceiling_for_day(4), 20);
        assert_eq!(ceiling_for_day(7), 20);
        assert_eq!(ceiling_for_day(8), 100);
        assert_eq!(ceiling_for_day(14), 100);
        assert_eq!(ceiling_for_day(15), 500);
        assert_eq!(ceiling_for_day(21), 500);
        assert_eq!(ceiling_for_day(22), 2_000);
        assert_eq!(ceiling_for_day(28), 2_000);
    }

    #[test]
    fn past_the_bands_it_doubles_weekly() {
        assert_eq!(ceiling_for_day(29), 4_000, "the first doubling");
        assert_eq!(ceiling_for_day(35), 4_000, "and it holds for the week");
        assert_eq!(ceiling_for_day(36), 8_000);
        assert_eq!(ceiling_for_day(42), 8_000);
        assert_eq!(ceiling_for_day(43), 16_000);
    }

    #[test]
    fn the_doubling_saturates_rather_than_overflowing() {
        // "Indefinitely" is true of the intent and useless as arithmetic:
        // doubling weekly overflows an i64 in under two years, and a panic in
        // the function that decides whether mail may go out is not a thing to
        // leave resting on nobody sending for long enough.
        assert_eq!(ceiling_for_day(10_000), WARM_UP_COMPLETE_AT);
        assert_eq!(ceiling_for_day(i64::MAX), WARM_UP_COMPLETE_AT);
    }

    #[test]
    fn a_warm_up_that_has_not_begun_allows_nothing() {
        // Not "the tightest band": an identity with no start date is one nobody
        // has checked, and sending from it is the thing the schedule forbids.
        assert_eq!(ceiling_for_day(0), 0);
        assert_eq!(ceiling_for_day(-1), 0);
        assert_eq!(ceiling_for_day(i64::MIN), 0);
    }

    #[test]
    fn the_start_date_is_day_one_rather_than_day_zero() {
        let start = Date::from_calendar_date(2026, Month::August, 18).unwrap();
        assert_eq!(day_of_warm_up(start, start), 1, "the first day counts");
        let next = Date::from_calendar_date(2026, Month::August, 19).unwrap();
        assert_eq!(day_of_warm_up(start, next), 2);
        // Across a month boundary, which is where a naive day-of-month
        // subtraction would go wrong.
        let september = Date::from_calendar_date(2026, Month::September, 1).unwrap();
        assert_eq!(day_of_warm_up(start, september), 15);
    }

    #[test]
    fn an_allowance_says_when_it_is_spent_and_when_the_schedule_is_over() {
        let spent = SendAllowance {
            day: 1,
            ceiling: 5,
            sent_today: 5,
            remaining: 0,
        };
        assert!(spent.is_exhausted());
        assert!(!spent.warm_up_finished());

        let grown = SendAllowance {
            day: 400,
            ceiling: WARM_UP_COMPLETE_AT,
            sent_today: 0,
            remaining: WARM_UP_COMPLETE_AT,
        };
        assert!(!grown.is_exhausted());
        assert!(grown.warm_up_finished());
    }
}
