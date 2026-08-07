//! The pipeline report (alo CRM, ADR 0035, wave B2.08) — what stands on a
//! board right now, and what was won and lost over a stated period.
//!
//! It answers **two questions of one board**, and keeping them apart is the
//! whole design:
//!
//! - **Value by stage is a snapshot of the open board**, unfiltered by the
//!   period. A column has no history to date it by: the stage events say when a
//!   deal moved, but reconstructing "what stood in Proposal on 31 March" is a
//!   different report over a different table, and pretending the period applies
//!   to these rows would put a figure under a heading it does not belong to.
//! - **Won and lost are the deals that closed inside the period**, judged on
//!   the `closed_at` snapshot frozen on each of them — so re-flagging a column
//!   next year never rewrites last year's win rate, exactly as
//!   [`crate::crm_deals`] intends.
//!
//! **Currencies are never converted** (`docs/design/crm.md` § The pipeline
//! report never converts currencies). A mixed-currency board answers one group
//! per currency; converting a *forecast* would mean picking today's rate for
//! money that may arrive next quarter, which reconciles against nothing.
//!
//! Every sum is computed here, in integer cents, by the database. The only
//! ratio is the win rate, and it is basis points — an integer — for the same
//! reason: no float ever carries a figure a person reads off a report.

use std::collections::{BTreeMap, BTreeSet};

use time::{Date, OffsetDateTime, Time, UtcOffset};

use crate::account::AccountStore;
use crate::crm_deals::DealState;
use crate::error::{Result, StoreError};
use crate::id::{CrmPipelineId, CrmStageId};

/// A count of deals and what they are worth, in one currency.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipelineTally {
    /// How many deals.
    pub deal_count: i64,
    /// What they add up to, in integer cents of the group's currency.
    pub value_cents: i64,
}

/// One column's row of the report: the open deals standing in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineStageRow {
    /// The column.
    pub stage_id: CrmStageId,
    /// Its header, as it reads today.
    pub name: String,
    /// Whether landing here means the deal was won.
    pub is_won: bool,
    /// Whether landing here means the deal was lost.
    pub is_lost: bool,
    /// The open deals standing in it, in this group's currency.
    pub open: PipelineTally,
}

/// One currency's whole answer: the open board by column, and the period's
/// outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineCurrency {
    /// ISO 4217 code, uppercase.
    pub currency: String,
    /// Every column of the board in board order, whether or not it holds
    /// anything in this currency — a report that silently omitted an empty
    /// column would read as a board that does not have one.
    pub stages: Vec<PipelineStageRow>,
    /// Everything still open on the board, in this currency.
    pub open: PipelineTally,
    /// What was won inside the period.
    pub won: PipelineTally,
    /// What was lost inside the period.
    pub lost: PipelineTally,
}

impl PipelineCurrency {
    /// The share of the period's closed deals that were won, in basis points
    /// (5000 = half), or `None` when nothing closed in the period — a win rate
    /// over no deals is not zero, it is unanswered.
    ///
    /// Counted by **deals, not value**: "we win one in three" is the sentence a
    /// sales team acts on, and a value-weighted rate would swing on one large
    /// deal. The value is right there beside it for anybody who wants the other
    /// reading.
    pub fn win_rate_bp(&self) -> Option<i32> {
        let closed = self.won.deal_count.checked_add(self.lost.deal_count)?;
        if closed <= 0 {
            return None;
        }
        // Integer arithmetic throughout: the counts are bounded by the row
        // count of one board, so the multiply cannot overflow an i64, and the
        // result is at most 10 000.
        let bp = self.won.deal_count.saturating_mul(10_000) / closed;
        i32::try_from(bp).ok()
    }
}

/// The whole report: one board, one period, one group per currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineReport {
    /// The board reported on.
    pub pipeline_id: CrmPipelineId,
    /// Its name, as it reads today — so a saved file says which board it is.
    pub pipeline_name: String,
    /// First day of the period the outcomes are counted over, inclusive.
    pub from: Date,
    /// Last day of that period, inclusive.
    pub to: Date,
    /// One group per currency that appears anywhere in the answer, sorted by
    /// code so two reads of an unchanged board are byte-identical.
    pub currencies: Vec<PipelineCurrency>,
}

/// The half-open instant range a pair of inclusive days covers, in UTC.
///
/// `closed_at` is an instant, the period is two days, and the join between them
/// has to be stated once rather than guessed at each call site: the period runs
/// from midnight starting `from` up to — but not including — midnight starting
/// the day after `to`. **UTC**, like every other stored instant in alo; a
/// tenant reading a quarter from a distant zone sees a boundary a few hours off
/// their local midnight, which is the same rule the rest of the product already
/// keeps and is honest about.
fn period_bounds(from: Date, to: Date) -> Result<(OffsetDateTime, OffsetDateTime)> {
    if to < from {
        return Err(StoreError::Validation(
            "the period ends before it starts".to_owned(),
        ));
    }
    let day_after = to.next_day().ok_or_else(|| {
        StoreError::Validation("the period ends beyond the last day there is".to_owned())
    })?;
    Ok((
        from.with_time(Time::MIDNIGHT).assume_offset(UtcOffset::UTC),
        day_after
            .with_time(Time::MIDNIGHT)
            .assume_offset(UtcOffset::UTC),
    ))
}

impl AccountStore {
    /// The report for one of the tenant's boards.
    ///
    /// Three reads, all tenant-scoped: the board's columns, the open deals
    /// grouped by column and currency, and the deals that closed inside the
    /// period grouped by outcome and currency. Nothing is summed in Rust that
    /// the database can sum, and nothing is converted between currencies at
    /// all.
    ///
    /// # Errors
    /// [`StoreError::Validation`] when the period ends before it starts;
    /// [`StoreError::NotFound`] when the board is not this tenant's (never an
    /// empty report, which would be an existence oracle);
    /// [`StoreError::Db`] on failure.
    pub async fn crm_pipeline_report(
        &self,
        pipeline: &CrmPipelineId,
        from: Date,
        to: Date,
    ) -> Result<PipelineReport> {
        let (start, end) = period_bounds(from, to)?;
        let board = self
            .crm_pipeline(pipeline)
            .await?
            .ok_or(StoreError::NotFound)?;
        // Archived columns included: a closed deal keeps pointing at the column
        // it closed in, and a column that still holds open work is one the
        // report must show even though nothing new can land there.
        let stages = self.crm_stages(pipeline, true).await?;

        let open = sqlx::query_as::<_, (String, String, i64, i64)>(
            "SELECT currency, stage_id, count(*), COALESCE(SUM(value_cents), 0)::bigint \
             FROM crm_deals \
             WHERE tenant_id = $1 AND pipeline_id = $2 AND outcome IS NULL \
             GROUP BY currency, stage_id",
        )
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        let closed = sqlx::query_as::<_, (String, String, i64, i64)>(
            "SELECT currency, outcome, count(*), COALESCE(SUM(value_cents), 0)::bigint \
             FROM crm_deals \
             WHERE tenant_id = $1 AND pipeline_id = $2 AND outcome IS NOT NULL \
               AND closed_at >= $3 AND closed_at < $4 \
             GROUP BY currency, outcome",
        )
        .bind(self.tenant.as_str())
        .bind(pipeline.as_str())
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::Db)?;

        Ok(PipelineReport {
            pipeline_id: board.id,
            pipeline_name: board.name,
            from,
            to,
            currencies: assemble(&stages, &open, &closed)?,
        })
    }
}

/// Turns the two grouped reads into one group per currency.
///
/// Split out from the query so the shape of the answer — which columns appear,
/// in what order, and what an absent group means — is decided in code that can
/// be tested without a database.
fn assemble(
    stages: &[crate::crm_stages::Stage],
    open: &[(String, String, i64, i64)],
    closed: &[(String, String, i64, i64)],
) -> Result<Vec<PipelineCurrency>> {
    // Every currency that appears anywhere, in code order: a board billed in
    // two currencies answers two groups, and one billed in none answers none.
    let mut codes: BTreeSet<&str> = BTreeSet::new();
    for (currency, ..) in open.iter().chain(closed.iter()) {
        codes.insert(currency.as_str());
    }
    // (currency, stage) → the open tally standing in it.
    let mut by_stage: BTreeMap<(&str, &str), PipelineTally> = BTreeMap::new();
    for (currency, stage, count, value) in open {
        by_stage.insert(
            (currency.as_str(), stage.as_str()),
            PipelineTally {
                deal_count: *count,
                value_cents: *value,
            },
        );
    }

    let mut groups = Vec::with_capacity(codes.len());
    for currency in codes {
        // A column is shown when it is still part of the board, or when it
        // holds open deals in this currency anyway — the second case cannot
        // arise today (a column with open work cannot be archived) and is here
        // so a report can never quietly drop a deal it counted in a total.
        let rows: Vec<PipelineStageRow> = stages
            .iter()
            .filter_map(|s| {
                let open = by_stage
                    .get(&(currency, s.id.as_str()))
                    .copied()
                    .unwrap_or_default();
                (s.archived_at.is_none() || open.deal_count > 0).then(|| PipelineStageRow {
                    stage_id: s.id.clone(),
                    name: s.name.clone(),
                    is_won: s.is_won,
                    is_lost: s.is_lost,
                    open,
                })
            })
            .collect();
        let mut group = PipelineCurrency {
            currency: currency.to_owned(),
            open: total(rows.iter().map(|r| r.open)),
            stages: rows,
            won: PipelineTally::default(),
            lost: PipelineTally::default(),
        };
        for (code, outcome, count, value) in closed {
            if code != currency {
                continue;
            }
            let tally = PipelineTally {
                deal_count: *count,
                value_cents: *value,
            };
            match DealState::parse(outcome) {
                Some(DealState::Won) => group.won = tally,
                Some(DealState::Lost) => group.lost = tally,
                // `open` is the absence of an outcome, so a row that stores it
                // is corrupt — and so is anything else. Never guessed at: a
                // guess here reports a lost deal as won.
                _ => {
                    return Err(StoreError::Db(sqlx::Error::Decode(
                        "crm_deals.outcome is not a known outcome".into(),
                    )));
                }
            }
        }
        groups.push(group);
    }
    Ok(groups)
}

/// Adds tallies up. Saturating, because a total that wrapped would be a wrong
/// number on a screen rather than a loud failure — and the deal-value cap makes
/// it unreachable in the first place
/// ([`crate::crm_deals::DEAL_VALUE_MAX_CENTS`]).
fn total(parts: impl Iterator<Item = PipelineTally>) -> PipelineTally {
    parts.fold(PipelineTally::default(), |acc, part| PipelineTally {
        deal_count: acc.deal_count.saturating_add(part.deal_count),
        value_cents: acc.value_cents.saturating_add(part.value_cents),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm_stages::Stage;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn stage(id: &str, name: &str, is_won: bool, is_lost: bool, archived: bool) -> Stage {
        Stage {
            id: CrmStageId::new(id),
            pipeline_id: CrmPipelineId::new("pip"),
            name: name.to_owned(),
            position: 1.0,
            is_won,
            is_lost,
            archived_at: archived.then_some(OffsetDateTime::UNIX_EPOCH),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn board() -> Vec<Stage> {
        vec![
            stage("new", "New", false, false, false),
            stage("prop", "Proposal", false, false, false),
            stage("won", "Won", true, false, false),
            stage("lost", "Lost", false, true, false),
        ]
    }

    fn row(currency: &str, key: &str, count: i64, value: i64) -> (String, String, i64, i64) {
        (currency.to_owned(), key.to_owned(), count, value)
    }

    #[test]
    fn a_period_is_two_inclusive_days_and_a_half_open_range_of_instants() {
        let (start, end) =
            period_bounds(day(2026, Month::July, 1), day(2026, Month::September, 30))
                .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(start.date(), day(2026, Month::July, 1));
        assert_eq!(start.time(), Time::MIDNIGHT);
        // The last day is included by ending at the midnight that starts the
        // day after it — the only way an instant column covers a whole day.
        assert_eq!(end.date(), day(2026, Month::October, 1));
        assert_eq!(end.time(), Time::MIDNIGHT);
        assert_eq!(start.offset(), UtcOffset::UTC);
        assert_eq!(end.offset(), UtcOffset::UTC);
    }

    #[test]
    fn one_day_is_a_period() {
        let one = day(2026, Month::March, 3);
        let (start, end) = period_bounds(one, one).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(start.date(), one);
        assert_eq!(end.date(), day(2026, Month::March, 4));
    }

    #[test]
    fn a_period_that_ends_before_it_starts_is_refused() {
        let err = period_bounds(day(2026, Month::March, 3), day(2026, Month::March, 2))
            .err()
            .unwrap_or_else(|| panic!("accepted a backwards period"));
        assert!(matches!(err, StoreError::Validation(ref m) if m.contains("ends before")));
    }

    #[test]
    fn an_empty_board_answers_no_currency_groups_at_all() {
        // Not "EUR, all zero": the report says nothing was billed rather than
        // inventing a currency the tenant never used.
        let groups = assemble(&board(), &[], &[]).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(groups.is_empty());
    }

    #[test]
    fn value_by_stage_is_the_open_board_and_the_group_total_is_its_sum() {
        let open = [row("EUR", "new", 2, 30_000), row("EUR", "prop", 1, 250_000)];
        let groups = assemble(&board(), &open, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.currency, "EUR");
        // Every column of the board, in board order, including the two that
        // hold nothing.
        assert_eq!(
            g.stages.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["New", "Proposal", "Won", "Lost"]
        );
        assert_eq!(g.stages[0].open.deal_count, 2);
        assert_eq!(g.stages[0].open.value_cents, 30_000);
        assert_eq!(g.stages[1].open.value_cents, 250_000);
        assert_eq!(g.stages[2].open, PipelineTally::default());
        assert_eq!(
            g.open,
            PipelineTally {
                deal_count: 3,
                value_cents: 280_000
            }
        );
        assert!(g.stages[2].is_won && g.stages[3].is_lost);
    }

    #[test]
    fn outcomes_are_the_periods_closed_deals_and_never_touch_the_open_rows() {
        let open = [row("EUR", "new", 1, 10_000)];
        let closed = [row("EUR", "won", 3, 900_000), row("EUR", "lost", 1, 50_000)];
        let groups = assemble(&board(), &open, &closed).unwrap_or_else(|e| panic!("{e:?}"));
        let g = &groups[0];
        assert_eq!(g.won.deal_count, 3);
        assert_eq!(g.won.value_cents, 900_000);
        assert_eq!(g.lost.deal_count, 1);
        // The open total is the open board, not the board plus what closed.
        assert_eq!(g.open.value_cents, 10_000);
        assert_eq!(g.win_rate_bp(), Some(7_500));
    }

    #[test]
    fn a_win_rate_over_nothing_is_unanswered_rather_than_zero() {
        let groups = assemble(&board(), &[row("EUR", "new", 1, 100)], &[])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(groups[0].win_rate_bp(), None);
        let all_lost = assemble(&board(), &[], &[row("EUR", "lost", 2, 100)])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(all_lost[0].win_rate_bp(), Some(0));
        let all_won = assemble(&board(), &[], &[row("EUR", "won", 2, 100)])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(all_won[0].win_rate_bp(), Some(10_000));
    }

    #[test]
    fn a_win_rate_is_a_third_not_a_rounded_percent() {
        let closed = [row("EUR", "won", 1, 1), row("EUR", "lost", 2, 1)];
        let groups = assemble(&board(), &[], &closed).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(groups[0].win_rate_bp(), Some(3_333));
    }

    #[test]
    fn two_currencies_are_two_groups_and_are_never_added_together() {
        let open = [row("EUR", "new", 1, 100_000), row("USD", "new", 1, 200_000)];
        let closed = [row("USD", "won", 1, 700_000)];
        let groups = assemble(&board(), &open, &closed).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            groups
                .iter()
                .map(|g| g.currency.as_str())
                .collect::<Vec<_>>(),
            ["EUR", "USD"],
            "sorted by code, so two reads of one board agree"
        );
        assert_eq!(groups[0].open.value_cents, 100_000);
        assert_eq!(groups[0].won, PipelineTally::default());
        assert_eq!(groups[1].open.value_cents, 200_000);
        assert_eq!(groups[1].won.value_cents, 700_000);
    }

    #[test]
    fn an_archived_column_is_dropped_unless_it_still_holds_open_work() {
        let mut stages = board();
        stages.push(stage("old", "Retired", false, false, true));
        let quiet = assemble(&stages, &[row("EUR", "new", 1, 100)], &[])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(quiet[0].stages.iter().all(|s| s.name != "Retired"));

        let holding = assemble(&stages, &[row("EUR", "old", 1, 4_200)], &[])
            .unwrap_or_else(|e| panic!("{e:?}"));
        let retired = holding[0]
            .stages
            .iter()
            .find(|s| s.name == "Retired")
            .unwrap_or_else(|| panic!("an archived column holding work must still be shown"));
        assert_eq!(retired.open.value_cents, 4_200);
        // …and it is inside the total, which is what makes showing it matter.
        assert_eq!(holding[0].open.value_cents, 4_200);
    }

    #[test]
    fn a_corrupt_outcome_fails_the_read_rather_than_being_guessed_at() {
        for corrupt in ["open", "WON", "abandoned", ""] {
            let closed = [row("EUR", corrupt, 1, 100)];
            assert!(
                matches!(assemble(&board(), &[], &closed), Err(StoreError::Db(_))),
                "expected a decode failure for {corrupt:?}"
            );
        }
    }
}
