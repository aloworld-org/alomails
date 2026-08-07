//! Rows into a **series** — the answer a chart is drawn from (ADR 0037, wave
//! BI-1).
//!
//! [`crate::insight_query`] reads the rows a [`crate::insight_spec::ChartSpec`]
//! asks for; this module turns them into buckets, and it is the **only file in
//! the wave that adds money up**. Everything it produces is an integer: cents
//! for money, counts for counts, basis points for the one declared ratio. No
//! float ever carries a figure a person reads, and the client never computes
//! money — it formats cents and draws a bar.
//!
//! Four rules the shape of the answer encodes, each of them a way a chart could
//! otherwise quietly lie:
//!
//! - **A bucket key is machine-readable; a label is not English from us.**
//!   Time buckets are ISO strings (`2026-01`, `2026-Q1`, `2026-W03`) the client
//!   formats per locale and carry no label at all. A category bucket carries
//!   one: the tenant's own words ([`Label::Raw`] — a customer, a stage, a
//!   payment method), a catalog id the client translates ([`Label::Catalog`] —
//!   a status, an age band, "other"), or a rate in basis points
//!   ([`Label::RateBp`]), which is a number rather than anybody's language.
//! - **A tail is folded, never dropped.** A category breakdown past its limit
//!   keeps the largest buckets and adds the rest into one `other` bucket, with
//!   [`Series::truncated`] set. A chart that silently omitted rows would read
//!   as a business that does not have them.
//! - **An empty time bucket is a zero, and an unanswered ratio is absent.** A
//!   quiet month is worth nothing and says so; a month in which nothing closed
//!   has no win rate at all, and reporting 0 % would be an invented fact
//!   ([`crate::crm_report::PipelineCurrency::win_rate_bp`] makes the same
//!   distinction).
//! - **Currencies that were not restated are separate series.** Money the
//!   engine could convert at each document's own frozen rate arrives as one
//!   group in the accounting currency; money it holds no honest rate for
//!   (a deal, a payment) arrives as one group per currency, never added
//!   together behind a single bar.
//!
//! The whole module is pure: no clock, no database, no tenant. That is what
//! makes a golden test able to state a hand-computed series.

use std::collections::BTreeMap;

use serde::Serialize;
use time::{Date, Duration, Month};

use crate::insight_catalog::{Aggregate, Grain, Unit};
use crate::insight_spec::{Sort, SortBy, SortDir};

/// The bucket key of a chart with no breakdown: one figure, over the period.
pub const TOTAL_BUCKET: &str = "total";
/// The bucket key the folded tail of a category breakdown lands in.
pub const OTHER_BUCKET: &str = "other";
/// The group key of a series that is not split by currency (a count, a ratio).
pub const ALL_GROUP: &str = "all";

/// Basis points in the whole: 10 000 = 100 %.
const BP_SCALE: i64 = 10_000;

/// What a bucket is called on screen.
///
/// Never English from the server: a catalog id crosses for anything from our
/// own closed vocabulary and the client translates it, the tenant's own words
/// cross verbatim, and a VAT rate crosses as the number it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Label {
    /// An id from our catalog (`status.issued`, `age.31_60`, `bucket.other`)
    /// that the client renders in the reader's language.
    Catalog {
        /// The id, from the closed set in [`crate::insight_query`].
        id: &'static str,
    },
    /// The tenant's own words — a customer name, a stage header, a payment
    /// method. Passed through untranslated, because it was never ours.
    Raw {
        /// The stored text.
        text: String,
    },
    /// A VAT rate in basis points (2100 = 21 %), formatted per locale by the
    /// client.
    RateBp {
        /// The rate.
        bp: i32,
    },
}

/// One bucket and what was measured in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Point {
    /// The bucket key: an ISO period for a time breakdown, a record id or a
    /// stored value for a category one, [`TOTAL_BUCKET`] for a chart with no
    /// breakdown, [`OTHER_BUCKET`] for the folded tail.
    pub bucket: String,
    /// What to call it. Absent for a time bucket, whose key already says
    /// everything and is formatted by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<Label>,
    /// The figure: cents, a count, or basis points, per [`SeriesUnit`].
    pub value: i64,
}

/// One drawable line/bar set. There is more than one only when money could not
/// honestly be restated into a single currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesGroup {
    /// The currency code this group is in, or [`ALL_GROUP`] when the measure
    /// is not money.
    pub key: String,
    /// What to call it.
    pub label: Label,
    /// Its buckets, in the order the chart draws them.
    pub points: Vec<Point>,
}

/// What the figures are measured in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesUnit {
    /// Money, a count, or basis points.
    pub kind: Unit,
    /// The currency every value is expressed in, when the whole series shares
    /// one (money restated into the tenant's accounting currency). `None` when
    /// each group carries its own code in [`SeriesGroup::key`], or when the
    /// measure is not money at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// Something true about the answer that the numbers alone do not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Note {
    /// A closed-set code the client translates (`unconverted_documents`).
    pub code: &'static str,
    /// How many rows it concerns.
    pub count: i64,
}

/// The code on the note a period carries when some of its documents could not
/// be restated into the accounting currency — the same honesty rule the VAT
/// summary applies: a figure is never part-invented, and the tile says when
/// part of the period is missing from it.
pub const UNCONVERTED_DOCUMENTS: &str = "unconverted_documents";

/// A whole chart answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Series {
    /// How to read every value below.
    pub unit: SeriesUnit,
    /// The drawable groups.
    #[serde(rename = "series")]
    pub groups: Vec<SeriesGroup>,
    /// What the numbers do not say on their own.
    pub notes: Vec<Note>,
    /// Whether a category tail was folded into [`OTHER_BUCKET`].
    pub truncated: bool,
}

/// One row's contribution, before the fold.
///
/// The three figures are kept apart rather than pre-reduced because which one a
/// chart wants is the aggregate's business, not the reader's: `value` is what a
/// sum adds, `rows` is what a count counts *and* what a ratio divides by, and
/// `won` is the ratio's numerator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Datum {
    /// Which currency group the contribution belongs to.
    pub group: String,
    /// Which bucket it falls in.
    pub bucket: String,
    /// The amount, in cents, for a summed measure.
    pub value: i64,
    /// How many rows it stands for.
    pub rows: i64,
    /// How many of those rows are in a ratio's numerator.
    pub won: i64,
}

/// Everything the fold needs that the data itself does not carry.
#[derive(Debug, Clone)]
pub(crate) struct FoldPlan {
    /// How a bucket is reduced.
    pub agg: Aggregate,
    /// What the result is measured in.
    pub unit: Unit,
    /// The one currency the whole series is expressed in, when there is one.
    pub unit_currency: Option<String>,
    /// Group keys that must appear even when nothing was measured — so an
    /// empty quarter answers "nothing, in euro" rather than nothing at all.
    pub always: Vec<String>,
    /// Every bucket of a bounded time window, so a quiet month is a zero.
    pub window: Option<Vec<String>>,
    /// How many category buckets to keep before folding the tail.
    pub limit: usize,
    /// How the buckets are ordered.
    pub sort: Sort,
    /// Whether the breakdown is a time breakdown.
    pub is_time: bool,
    /// Whether there is a breakdown at all.
    pub has_dimension: bool,
    /// Notes the reader must see beside the figures.
    pub notes: Vec<Note>,
}

/// A bucket's running figures during the fold.
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    value: i64,
    rows: i64,
    won: i64,
}

impl Tally {
    /// The figure this bucket reports under `agg`, or `None` when the bucket
    /// has no answer (a ratio over nothing closed is unanswered, not zero).
    fn reduce(self, agg: Aggregate) -> Option<i64> {
        match agg {
            Aggregate::Sum => Some(self.value),
            Aggregate::Count => Some(self.rows),
            Aggregate::Ratio => {
                if self.rows <= 0 {
                    return None;
                }
                Some(self.won.saturating_mul(BP_SCALE) / self.rows)
            }
        }
    }
}

/// Turns the rows into the answer.
///
/// Sums saturate rather than wrapping, for the reason
/// [`crate::billing_totals`] gives: an absurd input produces an absurd figure,
/// never a plausible wrong one and never a panic.
pub(crate) fn fold(data: Vec<Datum>, plan: &FoldPlan) -> Series {
    // (group, bucket) → tally. Sorted maps, so two evaluations of one unchanged
    // tenant answer byte-identically.
    let mut tallies: BTreeMap<String, BTreeMap<String, Tally>> = BTreeMap::new();
    for key in &plan.always {
        tallies.entry(key.clone()).or_default();
    }
    for datum in data {
        let bucket = tallies
            .entry(datum.group)
            .or_default()
            .entry(datum.bucket)
            .or_default();
        bucket.value = bucket.value.saturating_add(datum.value);
        bucket.rows = bucket.rows.saturating_add(datum.rows);
        bucket.won = bucket.won.saturating_add(datum.won);
    }

    let mut truncated = false;
    let mut groups = Vec::with_capacity(tallies.len());
    for (key, buckets) in tallies {
        let mut points: Vec<Point> = buckets
            .into_iter()
            .filter_map(|(bucket, tally)| {
                tally.reduce(plan.agg).map(|value| Point {
                    bucket,
                    label: None,
                    value,
                })
            })
            .collect();

        if plan.has_dimension && plan.is_time {
            zero_fill(&mut points, plan);
        } else if plan.has_dimension {
            truncated |= fold_tail(&mut points, plan.limit);
        } else if points.is_empty() && plan.agg != Aggregate::Ratio {
            // A single-figure tile over a period that holds nothing answers
            // zero, in a stated currency. A ratio does not: nothing closed has
            // no win rate, and 0 % would be a fact nobody stated.
            points.push(Point {
                bucket: TOTAL_BUCKET.to_owned(),
                label: None,
                value: 0,
            });
        }
        order(&mut points, plan.sort);
        groups.push(SeriesGroup {
            label: group_label(&key),
            key,
            points,
        });
    }

    Series {
        unit: SeriesUnit {
            kind: plan.unit,
            currency: plan.unit_currency.clone(),
        },
        groups,
        notes: plan.notes.clone(),
        truncated,
    }
}

/// The label of a currency group: its own code, in the tenant's data, or the
/// catalog word for "everything" when the measure is not money.
fn group_label(key: &str) -> Label {
    if key == ALL_GROUP {
        Label::Catalog { id: "series.all" }
    } else {
        Label::Raw {
            text: key.to_owned(),
        }
    }
}

/// Adds the window's missing buckets as zeros, so a quiet month reads as
/// nothing earned rather than as a month that did not happen.
///
/// Never for a ratio: a month in which nothing closed has no win rate, and a
/// zero there would be a fact nobody stated.
fn zero_fill(points: &mut Vec<Point>, plan: &FoldPlan) {
    let (Some(window), true) = (plan.window.as_ref(), plan.agg != Aggregate::Ratio) else {
        return;
    };
    for bucket in window {
        if !points.iter().any(|p| &p.bucket == bucket) {
            points.push(Point {
                bucket: bucket.clone(),
                label: None,
                value: 0,
            });
        }
    }
}

/// Keeps the `limit` largest buckets and adds the rest into one `other`.
/// Returns whether anything was folded.
fn fold_tail(points: &mut Vec<Point>, limit: usize) -> bool {
    if points.len() <= limit {
        return false;
    }
    // Largest first, ties broken by key so the choice of which buckets survive
    // is a fact about the data rather than about the order rows arrived in.
    points.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.bucket.cmp(&b.bucket)));
    let tail: i64 = points
        .split_off(limit)
        .into_iter()
        .fold(0i64, |sum, p| sum.saturating_add(p.value));
    points.push(Point {
        bucket: OTHER_BUCKET.to_owned(),
        label: Some(Label::Catalog { id: "bucket.other" }),
        value: tail,
    });
    true
}

/// Orders the buckets as the spec asked, keeping the folded tail last: `other`
/// is not a bucket that competes for a position, it is what is left.
fn order(points: &mut [Point], sort: Sort) {
    let split = points
        .iter()
        .position(|p| p.bucket == OTHER_BUCKET)
        .map_or(points.len(), |at| {
            points.swap(at, points.len() - 1);
            points.len() - 1
        });
    let (ranked, _) = points.split_at_mut(split);
    ranked.sort_by(|a, b| {
        let by = match sort.by {
            SortBy::Dimension => a.bucket.cmp(&b.bucket),
            SortBy::Value => a.value.cmp(&b.value).then_with(|| a.bucket.cmp(&b.bucket)),
        };
        match sort.dir {
            SortDir::Asc => by,
            SortDir::Desc => by.reverse(),
        }
    });
}

// ---- time buckets -----------------------------------------------------------

/// The ISO key of the bucket `date` falls in.
///
/// These strings are the chart's x-axis and the client's formatting input, so
/// they are stated once here and reproduced by the SQL of the grouped datasets
/// — a divergence between the two would put one dataset's January beside
/// another's.
pub fn bucket_key(date: Date, grain: Grain) -> String {
    match grain {
        Grain::Day => format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        ),
        Grain::Week => {
            // The ISO week-numbering year, not the calendar year: 1 January
            // 2027 is in week 53 of 2026, and saying "2027-W53" would invent a
            // week nobody had.
            let (year, week, _) = date.to_iso_week_date();
            format!("{year:04}-W{week:02}")
        }
        Grain::Month => format!("{:04}-{:02}", date.year(), u8::from(date.month())),
        Grain::Quarter => format!("{:04}-Q{}", date.year(), quarter(date)),
        Grain::Year => format!("{:04}", date.year()),
    }
}

/// Which quarter of its year `date` is in, 1–4.
fn quarter(date: Date) -> u8 {
    (u8::from(date.month()) - 1) / 3 + 1
}

/// The first day of the bucket `date` falls in.
pub fn bucket_start(date: Date, grain: Grain) -> Option<Date> {
    match grain {
        Grain::Day => Some(date),
        Grain::Week => {
            let back = i64::from(date.weekday().number_days_from_monday());
            date.checked_sub(Duration::days(back))
        }
        Grain::Month => Date::from_calendar_date(date.year(), date.month(), 1).ok(),
        Grain::Quarter => {
            let first = Month::try_from((quarter(date) - 1) * 3 + 1).ok()?;
            Date::from_calendar_date(date.year(), first, 1).ok()
        }
        Grain::Year => Date::from_calendar_date(date.year(), Month::January, 1).ok(),
    }
}

/// The last day of the bucket `date` falls in.
pub fn bucket_end(date: Date, grain: Grain) -> Option<Date> {
    let start = bucket_start(date, grain)?;
    let next = next_bucket(start, grain)?;
    next.previous_day()
}

/// The first day of the bucket after the one starting at `start`.
fn next_bucket(start: Date, grain: Grain) -> Option<Date> {
    match grain {
        Grain::Day => start.checked_add(Duration::days(1)),
        Grain::Week => start.checked_add(Duration::days(7)),
        Grain::Month => add_months(start, 1),
        Grain::Quarter => add_months(start, 3),
        Grain::Year => {
            Date::from_calendar_date(start.year().checked_add(1)?, Month::January, 1).ok()
        }
    }
}

/// The first day of the bucket `n` buckets before the one starting at `start`.
pub fn buckets_before(start: Date, grain: Grain, n: u32) -> Option<Date> {
    let n = i64::from(n);
    match grain {
        Grain::Day => start.checked_sub(Duration::days(n)),
        Grain::Week => start.checked_sub(Duration::days(n.checked_mul(7)?)),
        Grain::Month => add_months(start, -n),
        Grain::Quarter => add_months(start, -n.checked_mul(3)?),
        Grain::Year => Date::from_calendar_date(
            i32::try_from(i64::from(start.year()).checked_sub(n)?).ok()?,
            Month::January,
            1,
        )
        .ok(),
    }
}

/// `start` moved by whole months, keeping day 1 (every caller is a bucket
/// start, so there is no end-of-month clamping question to get wrong).
fn add_months(start: Date, months: i64) -> Option<Date> {
    let index = i64::from(start.year())
        .checked_mul(12)?
        .checked_add(i64::from(u8::from(start.month())) - 1)?
        .checked_add(months)?;
    let year = i32::try_from(index.div_euclid(12)).ok()?;
    let month = Month::try_from(u8::try_from(index.rem_euclid(12)).ok()?.checked_add(1)?).ok()?;
    Date::from_calendar_date(year, month, 1).ok()
}

/// Every bucket key the window `from..=to` covers, in order.
///
/// Walks days rather than buckets: a window is at most five years by
/// construction ([`crate::insight_spec::MAX_PERIOD_DAYS`]), and stepping days
/// makes the month, quarter, week and ISO-week-year edges the calendar's
/// problem rather than ours.
pub fn window_buckets(from: Date, to: Date, grain: Grain) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut day = from;
    while day <= to {
        let key = bucket_key(day, grain);
        if keys.last() != Some(&key) {
            keys.push(key);
        }
        let Some(next) = day.next_day() else { break };
        day = next;
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn day(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).unwrap_or_else(|e| panic!("{e}"))
    }

    fn datum(group: &str, bucket: &str, value: i64) -> Datum {
        Datum {
            group: group.to_owned(),
            bucket: bucket.to_owned(),
            value,
            rows: 1,
            won: 0,
        }
    }

    fn money_plan() -> FoldPlan {
        FoldPlan {
            agg: Aggregate::Sum,
            unit: Unit::Money,
            unit_currency: Some("EUR".to_owned()),
            always: vec!["EUR".to_owned()],
            window: None,
            limit: 50,
            sort: Sort {
                by: SortBy::Dimension,
                dir: SortDir::Asc,
            },
            is_time: true,
            has_dimension: true,
            notes: Vec::new(),
        }
    }

    fn values(series: &Series, group: &str) -> Vec<(String, i64)> {
        series
            .groups
            .iter()
            .find(|g| g.key == group)
            .map(|g| {
                g.points
                    .iter()
                    .map(|p| (p.bucket.clone(), p.value))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_time_breakdown_adds_up_per_bucket_and_stays_in_order() {
        let data = vec![
            datum("EUR", "2026-02", 25_000),
            datum("EUR", "2026-01", 100_000),
            datum("EUR", "2026-02", 5_000),
        ];
        let series = fold(data, &money_plan());
        assert_eq!(series.unit.kind, Unit::Money);
        assert_eq!(series.unit.currency.as_deref(), Some("EUR"));
        assert_eq!(
            values(&series, "EUR"),
            vec![
                ("2026-01".to_owned(), 100_000),
                ("2026-02".to_owned(), 30_000),
            ]
        );
        assert!(!series.truncated);
    }

    #[test]
    fn a_quiet_bucket_inside_the_window_is_a_zero_and_not_a_gap() {
        let mut plan = money_plan();
        plan.window = Some(vec![
            "2026-01".to_owned(),
            "2026-02".to_owned(),
            "2026-03".to_owned(),
        ]);
        let series = fold(vec![datum("EUR", "2026-02", 4_200)], &plan);
        assert_eq!(
            values(&series, "EUR"),
            vec![
                ("2026-01".to_owned(), 0),
                ("2026-02".to_owned(), 4_200),
                ("2026-03".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn a_period_with_nothing_in_it_still_answers_in_a_currency() {
        // "Nothing, in euro" is an answer; nothing at all is a question.
        let mut plan = money_plan();
        plan.window = Some(vec!["2026-01".to_owned()]);
        let series = fold(Vec::new(), &plan);
        assert_eq!(series.groups.len(), 1);
        assert_eq!(series.groups[0].key, "EUR");
        assert_eq!(values(&series, "EUR"), vec![("2026-01".to_owned(), 0)]);
    }

    #[test]
    fn currencies_that_were_not_restated_are_separate_groups() {
        let mut plan = money_plan();
        plan.unit_currency = None;
        plan.always = Vec::new();
        plan.is_time = false;
        plan.sort = Sort {
            by: SortBy::Value,
            dir: SortDir::Desc,
        };
        let series = fold(
            vec![
                datum("EUR", "won", 100_000),
                datum("USD", "won", 900_000),
                datum("EUR", "open", 50_000),
            ],
            &plan,
        );
        assert_eq!(
            series
                .groups
                .iter()
                .map(|g| g.key.as_str())
                .collect::<Vec<_>>(),
            ["EUR", "USD"],
            "one group per currency, never added together"
        );
        assert!(series.unit.currency.is_none(), "each group carries its own");
        assert_eq!(
            values(&series, "EUR"),
            vec![("won".to_owned(), 100_000), ("open".to_owned(), 50_000)]
        );
    }

    #[test]
    fn a_category_tail_is_folded_into_other_and_never_dropped() {
        let mut plan = money_plan();
        plan.is_time = false;
        plan.limit = 2;
        plan.sort = Sort {
            by: SortBy::Value,
            dir: SortDir::Desc,
        };
        let series = fold(
            vec![
                datum("EUR", "a", 10),
                datum("EUR", "b", 100),
                datum("EUR", "c", 1_000),
                datum("EUR", "d", 7),
            ],
            &plan,
        );
        assert!(series.truncated);
        assert_eq!(
            values(&series, "EUR"),
            vec![
                ("c".to_owned(), 1_000),
                ("b".to_owned(), 100),
                (OTHER_BUCKET.to_owned(), 17),
            ],
            "the two largest, then everything else — and the total is intact"
        );
        let other = series.groups[0]
            .points
            .last()
            .unwrap_or_else(|| panic!("no tail"));
        assert_eq!(other.label, Some(Label::Catalog { id: "bucket.other" }));
    }

    #[test]
    fn the_folded_tail_stays_last_whichever_way_the_chart_is_sorted() {
        let mut plan = money_plan();
        plan.is_time = false;
        plan.limit = 1;
        plan.sort = Sort {
            by: SortBy::Value,
            dir: SortDir::Asc,
        };
        let series = fold(vec![datum("EUR", "a", 10), datum("EUR", "b", 100)], &plan);
        assert_eq!(
            values(&series, "EUR"),
            vec![("b".to_owned(), 100), (OTHER_BUCKET.to_owned(), 10)],
            "'other' is what is left over, not a bucket competing for a place"
        );
    }

    #[test]
    fn a_count_counts_rows_and_a_sum_adds_values() {
        let data = vec![
            Datum {
                rows: 3,
                value: 999,
                ..datum("all", "2026-01", 0)
            },
            Datum {
                rows: 2,
                value: 1,
                ..datum("all", "2026-01", 0)
            },
        ];
        let mut plan = money_plan();
        plan.agg = Aggregate::Count;
        plan.unit = Unit::Count;
        plan.unit_currency = None;
        plan.always = vec![ALL_GROUP.to_owned()];
        let counted = fold(data.clone(), &plan);
        assert_eq!(values(&counted, ALL_GROUP), vec![("2026-01".to_owned(), 5)]);
        assert_eq!(counted.groups[0].label, Label::Catalog { id: "series.all" });

        plan.agg = Aggregate::Sum;
        let summed = fold(data, &plan);
        assert_eq!(
            values(&summed, ALL_GROUP),
            vec![("2026-01".to_owned(), 1_000)]
        );
    }

    #[test]
    fn a_ratio_is_basis_points_and_a_bucket_that_closed_nothing_is_absent() {
        let mut plan = money_plan();
        plan.agg = Aggregate::Ratio;
        plan.unit = Unit::PercentBp;
        plan.unit_currency = None;
        plan.always = vec![ALL_GROUP.to_owned()];
        plan.window = Some(vec![
            "2026-01".to_owned(),
            "2026-02".to_owned(),
            "2026-03".to_owned(),
        ]);
        let data = vec![
            Datum {
                rows: 1,
                won: 1,
                ..datum(ALL_GROUP, "2026-01", 0)
            },
            Datum {
                rows: 1,
                won: 0,
                ..datum(ALL_GROUP, "2026-01", 0)
            },
            Datum {
                rows: 1,
                won: 0,
                ..datum(ALL_GROUP, "2026-01", 0)
            },
            Datum {
                rows: 1,
                won: 1,
                ..datum(ALL_GROUP, "2026-03", 0)
            },
        ];
        let series = fold(data, &plan);
        assert_eq!(
            values(&series, ALL_GROUP),
            vec![
                ("2026-01".to_owned(), 3_333),
                ("2026-03".to_owned(), 10_000)
            ],
            "one in three, and February is unanswered rather than 0 %"
        );
    }

    #[test]
    fn a_chart_with_no_breakdown_is_one_bucket() {
        let mut plan = money_plan();
        plan.has_dimension = false;
        plan.is_time = false;
        plan.limit = 1;
        let series = fold(
            vec![
                datum("EUR", TOTAL_BUCKET, 700),
                datum("EUR", TOTAL_BUCKET, 300),
            ],
            &plan,
        );
        assert_eq!(
            values(&series, "EUR"),
            vec![(TOTAL_BUCKET.to_owned(), 1_000)],
            "a limit of one never folds the only figure into 'other'"
        );
        assert!(!series.truncated);
    }

    #[test]
    fn a_single_figure_over_an_empty_period_is_zero_and_a_ratio_is_absent() {
        let mut plan = money_plan();
        plan.has_dimension = false;
        plan.is_time = false;
        let empty = fold(Vec::new(), &plan);
        assert_eq!(values(&empty, "EUR"), vec![(TOTAL_BUCKET.to_owned(), 0)]);

        plan.agg = Aggregate::Ratio;
        plan.unit = Unit::PercentBp;
        let unanswered = fold(Vec::new(), &plan);
        assert!(
            values(&unanswered, "EUR").is_empty(),
            "a win rate over nothing closed is unanswered, never 0 %"
        );
    }

    #[test]
    fn an_absurd_series_saturates_rather_than_wrapping() {
        let plan = money_plan();
        let data = vec![datum("EUR", "2026-01", i64::MAX); 4];
        let series = fold(data, &plan);
        assert_eq!(
            values(&series, "EUR"),
            vec![("2026-01".to_owned(), i64::MAX)]
        );
    }

    #[test]
    fn bucket_keys_are_iso_and_sort_chronologically_as_text() {
        let d = day(2026, Month::January, 15);
        assert_eq!(bucket_key(d, Grain::Day), "2026-01-15");
        assert_eq!(bucket_key(d, Grain::Month), "2026-01");
        assert_eq!(bucket_key(d, Grain::Quarter), "2026-Q1");
        assert_eq!(bucket_key(d, Grain::Year), "2026");
        assert_eq!(
            bucket_key(day(2026, Month::December, 31), Grain::Quarter),
            "2026-Q4"
        );
        // A key is a sort key: text order is calendar order, which is why the
        // month and week are zero-padded.
        let mut keys = vec![
            bucket_key(day(2026, Month::October, 1), Grain::Month),
            bucket_key(day(2026, Month::February, 1), Grain::Month),
        ];
        keys.sort();
        assert_eq!(keys, ["2026-02", "2026-10"]);
    }

    #[test]
    fn a_week_key_carries_the_iso_week_year_not_the_calendar_one() {
        // 1 January 2027 is a Friday in ISO week 53 of 2026: naming it
        // "2027-W53" would invent a week that year never had.
        assert_eq!(
            bucket_key(day(2027, Month::January, 1), Grain::Week),
            "2026-W53"
        );
        assert_eq!(
            bucket_key(day(2026, Month::January, 15), Grain::Week),
            "2026-W03"
        );
    }

    #[test]
    fn a_bucket_runs_from_its_first_day_to_its_last() {
        let mid = day(2026, Month::February, 17);
        assert_eq!(
            bucket_start(mid, Grain::Month),
            Some(day(2026, Month::February, 1))
        );
        assert_eq!(
            bucket_end(mid, Grain::Month),
            Some(day(2026, Month::February, 28))
        );
        // A leap February ends on the 29th; the calendar decides, not us.
        let leap = day(2028, Month::February, 17);
        assert_eq!(
            bucket_end(leap, Grain::Month),
            Some(day(2028, Month::February, 29))
        );
        // A week starts on Monday, wherever in it the day falls.
        let thursday = day(2026, Month::January, 15);
        assert_eq!(
            bucket_start(thursday, Grain::Week),
            Some(day(2026, Month::January, 12))
        );
        assert_eq!(
            bucket_end(thursday, Grain::Week),
            Some(day(2026, Month::January, 18))
        );
        assert_eq!(
            bucket_end(mid, Grain::Quarter),
            Some(day(2026, Month::March, 31))
        );
        assert_eq!(
            bucket_end(mid, Grain::Year),
            Some(day(2026, Month::December, 31))
        );
    }

    #[test]
    fn stepping_back_whole_buckets_crosses_years_correctly() {
        let january = day(2026, Month::January, 1);
        assert_eq!(
            buckets_before(january, Grain::Month, 1),
            Some(day(2025, Month::December, 1))
        );
        assert_eq!(
            buckets_before(january, Grain::Month, 13),
            Some(day(2024, Month::December, 1))
        );
        assert_eq!(
            buckets_before(january, Grain::Quarter, 1),
            Some(day(2025, Month::October, 1))
        );
        assert_eq!(
            buckets_before(january, Grain::Year, 2),
            Some(day(2024, Month::January, 1))
        );
        assert_eq!(buckets_before(january, Grain::Month, 0), Some(january));
    }

    #[test]
    fn a_window_lists_every_bucket_it_covers_once_and_in_order() {
        let keys = window_buckets(
            day(2025, Month::November, 20),
            day(2026, Month::February, 3),
            Grain::Month,
        );
        assert_eq!(keys, ["2025-11", "2025-12", "2026-01", "2026-02"]);

        let quarters = window_buckets(
            day(2026, Month::January, 1),
            day(2026, Month::December, 31),
            Grain::Quarter,
        );
        assert_eq!(quarters, ["2026-Q1", "2026-Q2", "2026-Q3", "2026-Q4"]);

        let one_day = day(2026, Month::March, 3);
        assert_eq!(window_buckets(one_day, one_day, Grain::Day), ["2026-03-03"]);
    }
}
