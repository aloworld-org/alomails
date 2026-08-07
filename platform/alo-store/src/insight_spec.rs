//! The **ChartSpec** — the whole contract between a person (or a model) and
//! alo Insights (ADR 0037, wave BI-1).
//!
//! A ChartSpec is a typed envelope over the closed catalog in
//! [`crate::insight_catalog`]: which dataset, which measure, which breakdown,
//! over which period, under which filters, drawn how. It is what a tile
//! stores, what `POST /insights/eval` accepts, and what the ask-to-chart path
//! (BI1.07) makes a model *propose*. It is deliberately **not** a query:
//!
//! 1. **Nothing in a spec is an identifier.** Dataset, measure, dimension,
//!    filter field, operator, grain and viz are all enum variants; the SQL
//!    fragments they map to are written by us at compile time (BI1.03). The
//!    only caller-controlled values are bound parameters.
//! 2. **Unknown is a refusal, not a default.** Every type is
//!    `deny_unknown_fields`, so an invented measure is a named error rather
//!    than an empty chart.
//! 3. **Pairings are declared, not assumed.** The catalog's matrix decides
//!    what may be combined with what; this module enforces it.
//! 4. **Bounds are part of the type.** Categories, buckets, the period
//!    window, filter counts, filter values and the stored size of the
//!    envelope itself all have ceilings, and going over one is an error that
//!    says which ceiling and what its maximum is.
//!
//! The module is pure types + validation: no persistence and no SQL. The tile
//! store validates through it on every write (the write gate), and the query
//! engine consumes the same types, so the two cannot drift.

use serde::{Deserialize, Serialize};
use time::Date;
use time::format_description::well_known::Iso8601;

use crate::insight_catalog::{
    self, Aggregate, Dataset, Dimension, DimensionKind, FilterField, FilterOp, Grain, Measure,
    ValueKind, Viz,
};

/// The current ChartSpec schema version. A bump ships an explicit pure upgrade
/// applied on read, with the stored JSON rewritten lazily on the next save —
/// the site-sections pattern, for the same reason: a tile saved by a newer
/// client must not break an older reader mid-deploy.
pub const CHART_SPEC_SCHEMA_VERSION: u64 = 1;

/// Most categories a spec may ask for (the tail folds into one `other`
/// bucket at evaluation, never a silently omitted row).
pub const MAX_CATEGORIES: u32 = 50;
/// Most time buckets a spec may produce.
pub const MAX_TIME_BUCKETS: i64 = 400;
/// Widest period a spec may span, in days (five years).
pub const MAX_PERIOD_DAYS: i64 = 5 * 366;
/// Most filters one spec may carry.
pub const MAX_FILTERS: usize = 8;
/// Most values one filter may list.
pub const MAX_FILTER_VALUES: usize = 25;
/// Largest stored/accepted envelope, in bytes of JSON.
pub const MAX_SPEC_BYTES: usize = 8 * 1024;
/// Longest opaque id a filter value may be.
const MAX_ID_CHARS: usize = 64;
/// Longest free-text filter value (a payment method).
const MAX_TEXT_VALUE_CHARS: usize = 120;
/// The ceiling a VAT-rate filter value is checked against, in basis points.
const MAX_RATE_BP: i64 = 10_000;

/// Why a ChartSpec was rejected. Every message names the offending field and
/// the rule it broke, so the route edge can answer `422` with something the
/// builder UI — or a model on its one repair attempt — can act on. Messages
/// carry the caller's own input at most, never stored data.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The envelope declares a schema version this build does not speak.
    #[error("unsupported chart schema_version {0} (this build speaks {CHART_SPEC_SCHEMA_VERSION})")]
    UnsupportedVersion(u64),
    /// The JSON does not fit the typed schema: an unknown dataset, measure,
    /// dimension, filter, operator, grain or viz; an unknown field; a missing
    /// or mistyped value.
    #[error("chart spec does not match schema v{CHART_SPEC_SCHEMA_VERSION}: {0}")]
    Shape(#[from] serde_json::Error),
    /// The envelope is larger than a chart question has any reason to be.
    #[error("chart spec is {bytes} bytes; the maximum is {max}")]
    TooLarge {
        /// The offending size.
        bytes: usize,
        /// The ceiling.
        max: usize,
    },
    /// The spec is well-typed but breaks a catalog rule or a bound.
    #[error("{field}: {detail}")]
    Invalid {
        /// The field the rule belongs to (`measure`, `period`, `filters`, …).
        field: &'static str,
        /// The violated rule, named for the UI.
        detail: String,
    },
}

fn invalid(field: &'static str, detail: impl Into<String>) -> SpecError {
    SpecError::Invalid {
        field,
        detail: detail.into(),
    }
}

/// The wire name of a catalog enum, for error messages — read back through
/// serde so a message can never drift from the vocabulary it describes.
fn wire(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

/// The measure asked for, and how it is reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasureRef {
    /// Which measure.
    pub id: Measure,
    /// How it is aggregated over a bucket.
    pub agg: Aggregate,
}

/// The breakdown asked for. A time dimension carries the grain it buckets by;
/// a category dimension carries none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionRef {
    /// Which dimension.
    pub id: Dimension,
    /// The bucket size, for a time dimension only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain: Option<Grain>,
}

/// Which slice of time the chart covers.
///
/// The variants are internally tagged on `kind`, matching the wire shape in
/// `docs/design/insights.md`. Unknown *fields* inside a variant are rejected
/// by [`Period::check_shape`] rather than by `deny_unknown_fields`, which
/// serde cannot apply to an internally tagged enum without rejecting the tag
/// itself (serde #1600) — the rule is the same, it is simply enforced here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Period {
    /// The last `n` buckets of `grain`, up to and including today's.
    LastN {
        /// How many buckets.
        n: u32,
        /// How big each one is.
        grain: Grain,
    },
    /// A closed range of ISO `YYYY-MM-DD` dates, both ends included.
    Range {
        /// First day.
        from: String,
        /// Last day.
        to: String,
    },
    /// Everything the tenant has. The period is unbounded, so the row cap at
    /// evaluation (BI1.03) is what bounds the work — a spec cannot know in
    /// advance how much history a tenant holds.
    All,
}

/// The fields each period kind may carry, checked against the raw JSON so an
/// invented one is a refusal rather than a silently ignored key.
const PERIOD_FIELDS: &[(&str, &[&str])] = &[
    ("last_n", &["kind", "n", "grain"]),
    ("range", &["kind", "from", "to"]),
    ("all", &["kind"]),
];

impl Period {
    /// Rejects unknown fields inside a period object. Called from
    /// [`ChartSpec::from_value`], where the raw JSON is still available.
    fn check_shape(value: &serde_json::Value) -> Result<(), SpecError> {
        let Some(object) = value.as_object() else {
            return Ok(()); // Shape errors are serde's to report.
        };
        let Some(kind) = object.get("kind").and_then(serde_json::Value::as_str) else {
            return Ok(());
        };
        let Some((_, allowed)) = PERIOD_FIELDS.iter().find(|(name, _)| *name == kind) else {
            return Ok(());
        };
        for key in object.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(invalid(
                    "period",
                    format!("a {kind} period has no field {key:?}"),
                ));
            }
        }
        Ok(())
    }

    /// The widest the period can be, in days — `None` for [`Period::All`],
    /// which is bounded at evaluation instead.
    fn span_days(&self) -> Result<Option<i64>, SpecError> {
        match self {
            Period::LastN { n, grain } => {
                if *n == 0 {
                    return Err(invalid("period", "n must be at least 1"));
                }
                Ok(Some(i64::from(*n) * grain_max_days(*grain)))
            }
            Period::Range { from, to } => {
                let from = parse_day("from", from)?;
                let to = parse_day("to", to)?;
                if to < from {
                    return Err(invalid("period", "from must not be after to"));
                }
                Ok(Some((to - from).whole_days() + 1))
            }
            Period::All => Ok(None),
        }
    }
}

/// The longest a bucket of `grain` can be, in days. Used for the *widest
/// possible* window, so it never underestimates.
fn grain_max_days(grain: Grain) -> i64 {
    match grain {
        Grain::Day => 1,
        Grain::Week => 7,
        Grain::Month => 31,
        Grain::Quarter => 92,
        Grain::Year => 366,
    }
}

/// The most buckets `days` can produce at `grain`. Uses the *shortest*
/// possible bucket, so it never underestimates either.
fn max_buckets(days: i64, grain: Grain) -> i64 {
    let shortest = match grain {
        Grain::Day => 1,
        Grain::Week => 7,
        Grain::Month => 28,
        Grain::Quarter => 90,
        Grain::Year => 365,
    };
    days.div_euclid(shortest) + 1
}

fn parse_day(field: &str, raw: &str) -> Result<Date, SpecError> {
    Date::parse(raw, &Iso8601::DATE).map_err(|_| {
        invalid(
            "period",
            format!("{field} must be an ISO date (YYYY-MM-DD)"),
        )
    })
}

/// One filter: a field, a comparison, and the values it compares against.
/// Values are always **data** — they are bound as parameters, never spliced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    /// Which field.
    pub id: FilterField,
    /// How it compares.
    pub op: FilterOp,
    /// What it compares against; at least one, at most
    /// [`MAX_FILTER_VALUES`].
    pub values: Vec<String>,
}

/// What the buckets are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    /// By the bucket key — chronological for a time dimension.
    Dimension,
    /// By the measured value.
    Value,
}

/// Which way an ordering runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// How the buckets are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sort {
    /// What to order by.
    pub by: SortBy,
    /// Which way.
    pub dir: SortDir,
}

/// A whole chart question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSpec {
    /// Envelope version; this build speaks [`CHART_SPEC_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// Which dataset the question is asked of.
    pub dataset: Dataset,
    /// What is measured.
    pub measure: MeasureRef,
    /// How it is broken down; `None` is a single figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<DimensionRef>,
    /// Over which slice of time.
    pub period: Period,
    /// Which of the dataset's dates the period narrows on. Omitted means the
    /// chart's own time breakdown when it has one, and otherwise the dataset's
    /// declared default ([`insight_catalog::DatasetEntry::period`]) — see
    /// [`ChartSpec::period_dimension`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_on: Option<Dimension>,
    /// Which rows are considered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// How the buckets are ordered; the engine's default is by dimension
    /// ascending for a time breakdown, by value descending otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    /// At most how many category buckets to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// How the answer is drawn.
    pub viz: Viz,
}

impl ChartSpec {
    /// Parses and fully validates a stored or wire spec. **This is the write
    /// gate**: everything persisted as a tile goes through here first, and so
    /// does everything evaluated.
    ///
    /// # Errors
    /// [`SpecError::TooLarge`] before anything is parsed;
    /// [`SpecError::UnsupportedVersion`] before shape, so a v2 payload gets
    /// the version error rather than a confusing shape one;
    /// [`SpecError::Shape`] on unknown or mistyped fields; and the
    /// catalog/bound rules from [`Self::validate`].
    pub fn from_value(value: serde_json::Value) -> Result<Self, SpecError> {
        let bytes = serde_json::to_vec(&value).map(|v| v.len()).unwrap_or(0);
        if bytes > MAX_SPEC_BYTES {
            return Err(SpecError::TooLarge {
                bytes,
                max: MAX_SPEC_BYTES,
            });
        }
        if let Some(version) = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            && version != CHART_SPEC_SCHEMA_VERSION
        {
            return Err(SpecError::UnsupportedVersion(version));
        }
        if let Some(period) = value.get("period") {
            Period::check_shape(period)?;
        }
        let spec: Self = serde_json::from_value(value)?;
        spec.validate()?;
        Ok(spec)
    }

    /// Serialises back to the stored JSON shape.
    ///
    /// # Errors
    /// [`SpecError::Shape`] — unreachable for values built from these types,
    /// but serialisation is fallible by signature.
    pub fn to_value(&self) -> Result<serde_json::Value, SpecError> {
        Ok(serde_json::to_value(self)?)
    }

    /// Every rule that is not shape: the version, the catalog's compatibility
    /// matrix, the bounds, and the viz/breakdown agreement.
    ///
    /// # Errors
    /// The [`SpecError`] variant naming the offending field.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.schema_version != CHART_SPEC_SCHEMA_VERSION {
            return Err(SpecError::UnsupportedVersion(self.schema_version));
        }
        let entry = insight_catalog::dataset(self.dataset);

        let measure = entry.measure(self.measure.id).ok_or_else(|| {
            invalid(
                "measure",
                format!(
                    "the {} dataset does not offer the {} measure",
                    wire(&self.dataset),
                    wire(&self.measure.id)
                ),
            )
        })?;
        if !measure.aggregates.contains(&self.measure.agg) {
            return Err(invalid(
                "measure",
                format!(
                    "{} cannot be aggregated with {}",
                    wire(&self.measure.id),
                    wire(&self.measure.agg)
                ),
            ));
        }

        if let Some(dimension) = &self.dimension {
            let dim = entry.dimension(dimension.id).ok_or_else(|| {
                invalid(
                    "dimension",
                    format!(
                        "the {} dataset cannot be broken down by {}",
                        wire(&self.dataset),
                        wire(&dimension.id)
                    ),
                )
            })?;
            if !measure.dimensions.contains(&dimension.id) {
                return Err(invalid(
                    "dimension",
                    format!(
                        "{} cannot be broken down by {}",
                        wire(&self.measure.id),
                        wire(&dimension.id)
                    ),
                ));
            }
            match (dim.kind, dimension.grain) {
                (DimensionKind::Time(grains), Some(grain)) if grains.contains(&grain) => {}
                (DimensionKind::Time(_), Some(grain)) => {
                    return Err(invalid(
                        "dimension",
                        format!(
                            "{} cannot be bucketed by {}",
                            wire(&dimension.id),
                            wire(&grain)
                        ),
                    ));
                }
                (DimensionKind::Time(_), None) => {
                    return Err(invalid(
                        "dimension",
                        format!("{} needs a grain", wire(&dimension.id)),
                    ));
                }
                (DimensionKind::Category, Some(_)) => {
                    return Err(invalid(
                        "dimension",
                        format!("{} takes no grain", wire(&dimension.id)),
                    ));
                }
                (DimensionKind::Category, None) => {}
            }
        }

        if let Some(on) = self.period_on {
            let dim = entry.dimension(on).ok_or_else(|| {
                invalid(
                    "period_on",
                    format!(
                        "the {} dataset has no {} date",
                        wire(&self.dataset),
                        wire(&on)
                    ),
                )
            })?;
            if !matches!(dim.kind, DimensionKind::Time(_)) {
                return Err(invalid(
                    "period_on",
                    format!(
                        "{} is not a date, so a period cannot narrow on it",
                        wire(&on)
                    ),
                ));
            }
        }

        self.check_period()?;
        self.check_filters()?;

        if let Some(limit) = self.limit
            && (limit == 0 || limit > MAX_CATEGORIES)
        {
            return Err(invalid(
                "limit",
                format!("limit must be between 1 and {MAX_CATEGORIES}"),
            ));
        }

        if self.sort.is_some_and(|s| s.by == SortBy::Dimension) && self.dimension.is_none() {
            return Err(invalid(
                "sort",
                "a chart with no breakdown cannot be sorted by dimension",
            ));
        }

        self.check_viz()
    }

    /// Which of the dataset's dates [`Self::period`] narrows on.
    ///
    /// Three rules, in order, so a chart's period is never a surprise:
    /// what `period_on` says; failing that the chart's **own** time breakdown
    /// (revenue by month over the last year narrows on the month it draws);
    /// and failing that the dataset's declared default — a document is dated
    /// by its issue date, a receivable by its due date, a payment by the day
    /// the money arrived, a deal by the day it was created.
    ///
    /// A chart that means another of the dataset's dates — "won this month" is
    /// about the day a deal *closed*, not the day it was raised — says so, and
    /// the gallery specs (BI1.06) do.
    pub fn period_dimension(&self) -> Dimension {
        if let Some(on) = self.period_on {
            return on;
        }
        if let Some(DimensionRef { id, grain: Some(_) }) = self.dimension {
            return id;
        }
        insight_catalog::dataset(self.dataset).period
    }

    /// The period's own rules, plus the bucket ceiling it implies together
    /// with a time breakdown.
    fn check_period(&self) -> Result<(), SpecError> {
        let Some(days) = self.period.span_days()? else {
            return Ok(());
        };
        if days > MAX_PERIOD_DAYS {
            return Err(invalid(
                "period",
                format!("a period may span at most {MAX_PERIOD_DAYS} days"),
            ));
        }
        if let Period::LastN { n, .. } = self.period
            && i64::from(n) > MAX_TIME_BUCKETS
        {
            return Err(invalid(
                "period",
                format!("a period may hold at most {MAX_TIME_BUCKETS} buckets"),
            ));
        }
        if let Some(DimensionRef {
            grain: Some(grain), ..
        }) = self.dimension
            && max_buckets(days, grain) > MAX_TIME_BUCKETS
        {
            return Err(invalid(
                "period",
                format!(
                    "{} buckets over this period exceed the maximum of {MAX_TIME_BUCKETS}; \
                     narrow the period or use a coarser grain",
                    wire(&grain)
                ),
            ));
        }
        Ok(())
    }

    /// Filter count, uniqueness, membership in the dataset, allowed operator,
    /// and the shape of every value.
    fn check_filters(&self) -> Result<(), SpecError> {
        if self.filters.len() > MAX_FILTERS {
            return Err(invalid(
                "filters",
                format!("a chart may carry at most {MAX_FILTERS} filters"),
            ));
        }
        let entry = insight_catalog::dataset(self.dataset);
        for (index, filter) in self.filters.iter().enumerate() {
            if self.filters[..index].iter().any(|f| f.id == filter.id) {
                return Err(invalid(
                    "filters",
                    format!("{} is filtered twice", wire(&filter.id)),
                ));
            }
            let declared = entry.filter(filter.id).ok_or_else(|| {
                invalid(
                    "filters",
                    format!(
                        "the {} dataset cannot be filtered by {}",
                        wire(&self.dataset),
                        wire(&filter.id)
                    ),
                )
            })?;
            if !declared.operators.contains(&filter.op) {
                return Err(invalid(
                    "filters",
                    format!(
                        "{} does not accept the {} operator",
                        wire(&filter.id),
                        wire(&filter.op)
                    ),
                ));
            }
            if filter.values.is_empty() {
                return Err(invalid(
                    "filters",
                    format!("{} needs at least one value", wire(&filter.id)),
                ));
            }
            if filter.values.len() > MAX_FILTER_VALUES {
                return Err(invalid(
                    "filters",
                    format!(
                        "{} may list at most {MAX_FILTER_VALUES} values",
                        wire(&filter.id)
                    ),
                ));
            }
            for value in &filter.values {
                check_value(filter.id, declared.value, value)?;
            }
        }
        Ok(())
    }

    /// A chart form and a breakdown have to agree, or the tile renders
    /// nothing a person can read.
    fn check_viz(&self) -> Result<(), SpecError> {
        let dimension = self.dimension.as_ref();
        match self.viz {
            Viz::Number => {
                if dimension.is_some() {
                    return Err(invalid(
                        "viz",
                        "a number tile shows one figure, so it takes no breakdown",
                    ));
                }
            }
            Viz::Line => match dimension {
                Some(d) if d.grain.is_some() => {}
                Some(d) => {
                    return Err(invalid(
                        "viz",
                        format!(
                            "a line runs over time; {} is not a time breakdown",
                            wire(&d.id)
                        ),
                    ));
                }
                None => return Err(invalid("viz", "a line chart needs a time breakdown")),
            },
            Viz::Pie => match dimension {
                Some(d) if d.grain.is_none() => {}
                Some(_) => {
                    return Err(invalid(
                        "viz",
                        "a pie shows shares of a whole, so it takes a category breakdown, not time",
                    ));
                }
                None => return Err(invalid("viz", "a pie chart needs a breakdown")),
            },
            Viz::Bar | Viz::Table => {
                if dimension.is_none() {
                    return Err(invalid(
                        "viz",
                        format!("a {} needs a breakdown", wire(&self.viz)),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// One filter value, checked against the shape its field declares. Ids are
/// checked for *shape* only here — they are resolved against the tenant's own
/// records at evaluation (BI1.03), which is what turns another tenant's id
/// into a refusal instead of a join that quietly matches nothing.
fn check_value(field: FilterField, kind: ValueKind, value: &str) -> Result<(), SpecError> {
    let name = wire(&field);
    match kind {
        ValueKind::Id => {
            let ok = !value.is_empty()
                && value.len() <= MAX_ID_CHARS
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !ok {
                return Err(invalid("filters", format!("{name} takes record ids")));
            }
        }
        ValueKind::Currency => {
            let ok = value.len() == 3 && value.bytes().all(|b| b.is_ascii_alphabetic());
            if !ok {
                return Err(invalid(
                    "filters",
                    format!("{name} takes three-letter ISO 4217 codes"),
                ));
            }
        }
        ValueKind::Enum(allowed) => {
            if !allowed.contains(&value) {
                return Err(invalid(
                    "filters",
                    format!("{name} accepts only: {}", allowed.join(", ")),
                ));
            }
        }
        ValueKind::RateBp => {
            let ok = value
                .parse::<i64>()
                .is_ok_and(|bp| (0..=MAX_RATE_BP).contains(&bp));
            if !ok {
                return Err(invalid(
                    "filters",
                    format!("{name} takes rates in basis points between 0 and {MAX_RATE_BP}"),
                ));
            }
        }
        ValueKind::Text => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.chars().count() > MAX_TEXT_VALUE_CHARS {
                return Err(invalid(
                    "filters",
                    format!("{name} values must be 1 to {MAX_TEXT_VALUE_CHARS} characters"),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Revenue by month for the last year — the spec the Business overview
    /// leads with, and the fixture the rest of the tests bend.
    fn revenue_by_month() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "dataset": "billing.documents",
            "measure": { "id": "net", "agg": "sum" },
            "dimension": { "id": "issue_date", "grain": "month" },
            "period": { "kind": "last_n", "n": 12, "grain": "month" },
            "filters": [ { "id": "status", "op": "in", "values": ["issued", "paid"] } ],
            "sort": { "by": "dimension", "dir": "asc" },
            "limit": 24,
            "viz": "bar"
        })
    }

    fn detail(result: Result<ChartSpec, SpecError>) -> String {
        match result {
            Err(error) => error.to_string(),
            Ok(spec) => panic!("expected rejection, got: {spec:?}"),
        }
    }

    #[test]
    fn the_reference_spec_round_trips_through_the_types() {
        let spec =
            ChartSpec::from_value(revenue_by_month()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(spec.dataset, Dataset::BillingDocuments);
        assert_eq!(spec.measure.id, Measure::Net);
        assert_eq!(spec.viz, Viz::Bar);
        let canonical = spec.to_value().unwrap_or_default();
        let reparsed =
            ChartSpec::from_value(canonical).unwrap_or_else(|e| panic!("re-rejected: {e}"));
        assert_eq!(reparsed, spec, "the canonical form must parse back equal");
    }

    #[test]
    fn a_version_this_build_does_not_speak_is_refused_before_shape() {
        let mut value = revenue_by_month();
        value["schema_version"] = json!(2);
        // Also make the shape wrong: the version error must still win, so a
        // newer client gets "upgrade me", not "your JSON is broken".
        value["measure"] = json!({ "id": "net", "agg": "sum", "future": true });
        assert!(matches!(
            ChartSpec::from_value(value),
            Err(SpecError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn an_invented_measure_is_an_error_and_never_an_empty_chart() {
        let mut value = revenue_by_month();
        value["measure"] = json!({ "id": "profit", "agg": "sum" });
        assert!(matches!(
            ChartSpec::from_value(value),
            Err(SpecError::Shape(_))
        ));
    }

    #[test]
    fn unknown_fields_are_refused_everywhere_including_inside_a_period() {
        let mut value = revenue_by_month();
        value["colour"] = json!("blue");
        assert!(matches!(
            ChartSpec::from_value(value),
            Err(SpecError::Shape(_))
        ));

        let mut value = revenue_by_month();
        value["period"] = json!({ "kind": "last_n", "n": 12, "grain": "month", "tz": "CET" });
        assert!(detail(ChartSpec::from_value(value)).contains("no field"));

        let mut value = revenue_by_month();
        value["measure"] = json!({ "id": "net", "agg": "sum", "scale": 2 });
        assert!(matches!(
            ChartSpec::from_value(value),
            Err(SpecError::Shape(_))
        ));
    }

    #[test]
    fn a_measure_must_belong_to_its_dataset() {
        let mut value = revenue_by_month();
        value["measure"] = json!({ "id": "value", "agg": "sum" });
        assert!(detail(ChartSpec::from_value(value)).contains("does not offer"));
    }

    #[test]
    fn a_deal_value_cannot_be_broken_down_by_a_vat_rate() {
        let value = json!({
            "schema_version": 1,
            "dataset": "crm.deals",
            "measure": { "id": "value", "agg": "sum" },
            "dimension": { "id": "vat_rate" },
            "period": { "kind": "all" },
            "viz": "bar"
        });
        assert!(detail(ChartSpec::from_value(value)).contains("cannot be broken down"));
    }

    #[test]
    fn documents_may_not_be_counted_per_vat_rate() {
        // A two-rate invoice is one document; counting per rate would report
        // more invoices than the tenant raised. Its money does split per rate.
        let count_by_rate = json!({
            "schema_version": 1,
            "dataset": "billing.documents",
            "measure": { "id": "count", "agg": "count" },
            "dimension": { "id": "vat_rate" },
            "period": { "kind": "all" },
            "viz": "bar"
        });
        assert!(detail(ChartSpec::from_value(count_by_rate)).contains("cannot be broken down"));

        let mut net_by_rate = revenue_by_month();
        net_by_rate["dimension"] = json!({ "id": "vat_rate" });
        net_by_rate["sort"] = json!({ "by": "value", "dir": "desc" });
        assert!(ChartSpec::from_value(net_by_rate).is_ok());
    }

    #[test]
    fn a_win_rate_is_not_broken_down_by_the_outcome_it_measures() {
        let value = json!({
            "schema_version": 1,
            "dataset": "crm.deals",
            "measure": { "id": "win_rate", "agg": "ratio" },
            "dimension": { "id": "outcome" },
            "period": { "kind": "last_n", "n": 4, "grain": "quarter" },
            "viz": "bar"
        });
        assert!(detail(ChartSpec::from_value(value)).contains("cannot be broken down"));
    }

    #[test]
    fn an_aggregate_a_measure_does_not_allow_is_refused() {
        let mut value = revenue_by_month();
        value["measure"] = json!({ "id": "net", "agg": "count" });
        assert!(detail(ChartSpec::from_value(value)).contains("cannot be aggregated"));
    }

    #[test]
    fn a_time_dimension_needs_an_allowed_grain_and_a_category_needs_none() {
        let mut missing = revenue_by_month();
        missing["dimension"] = json!({ "id": "issue_date" });
        assert!(detail(ChartSpec::from_value(missing)).contains("needs a grain"));

        let mut on_category = revenue_by_month();
        on_category["dimension"] = json!({ "id": "customer", "grain": "month" });
        assert!(detail(ChartSpec::from_value(on_category)).contains("takes no grain"));

        // Deals bucket by month at the finest: a forecast by day is noise.
        let daily_deals = json!({
            "schema_version": 1,
            "dataset": "crm.deals",
            "measure": { "id": "value", "agg": "sum" },
            "dimension": { "id": "expected_close", "grain": "day" },
            "period": { "kind": "last_n", "n": 6, "grain": "month" },
            "viz": "line"
        });
        assert!(detail(ChartSpec::from_value(daily_deals)).contains("cannot be bucketed"));
    }

    #[test]
    fn a_period_is_bounded_in_days_and_in_buckets() {
        let mut wide = revenue_by_month();
        wide["period"] = json!({ "kind": "range", "from": "2015-01-01", "to": "2026-01-01" });
        assert!(detail(ChartSpec::from_value(wide)).contains("at most"));

        let mut backwards = revenue_by_month();
        backwards["period"] = json!({ "kind": "range", "from": "2026-06-01", "to": "2026-01-01" });
        assert!(detail(ChartSpec::from_value(backwards)).contains("after"));

        let mut malformed = revenue_by_month();
        malformed["period"] = json!({ "kind": "range", "from": "01/06/2026", "to": "2026-01-01" });
        assert!(detail(ChartSpec::from_value(malformed)).contains("ISO date"));

        let mut empty = revenue_by_month();
        empty["period"] = json!({ "kind": "last_n", "n": 0, "grain": "month" });
        assert!(detail(ChartSpec::from_value(empty)).contains("at least 1"));

        // Four years of days is inside the five-year window but well past the
        // bucket ceiling — the error says which ceiling and how to get under.
        let mut too_many = revenue_by_month();
        too_many["dimension"] = json!({ "id": "issue_date", "grain": "day" });
        too_many["period"] = json!({ "kind": "range", "from": "2022-01-01", "to": "2026-01-01" });
        let message = detail(ChartSpec::from_value(too_many));
        assert!(message.contains("coarser grain"), "{message}");

        // A range that fits, at a grain that fits.
        let mut ok = revenue_by_month();
        ok["period"] = json!({ "kind": "range", "from": "2024-01-01", "to": "2026-01-01" });
        assert!(ChartSpec::from_value(ok).is_ok());
    }

    #[test]
    fn filters_are_bounded_unique_and_shape_checked() {
        let mut twice = revenue_by_month();
        twice["filters"] = json!([
            { "id": "status", "op": "in", "values": ["issued"] },
            { "id": "status", "op": "in", "values": ["paid"] },
        ]);
        assert!(detail(ChartSpec::from_value(twice)).contains("filtered twice"));

        let mut foreign = revenue_by_month();
        foreign["filters"] = json!([{ "id": "outcome", "op": "in", "values": ["won"] }]);
        assert!(detail(ChartSpec::from_value(foreign)).contains("cannot be filtered"));

        let mut empty = revenue_by_month();
        empty["filters"] = json!([{ "id": "status", "op": "in", "values": [] }]);
        assert!(detail(ChartSpec::from_value(empty)).contains("at least one value"));

        let mut many = revenue_by_month();
        many["filters"] = json!([{
            "id": "customer",
            "op": "in",
            "values": (0..=MAX_FILTER_VALUES).map(|i| format!("c{i}")).collect::<Vec<_>>(),
        }]);
        assert!(detail(ChartSpec::from_value(many)).contains("at most"));

        let mut too_many_filters = revenue_by_month();
        too_many_filters["filters"] = json!(
            (0..=MAX_FILTERS)
                .map(|i| json!({ "id": "status", "op": "in", "values": [format!("s{i}")] }))
                .collect::<Vec<_>>()
        );
        assert!(detail(ChartSpec::from_value(too_many_filters)).contains("at most"));
    }

    #[test]
    fn a_filter_value_must_have_the_shape_its_field_declares() {
        let mut bad_status = revenue_by_month();
        bad_status["filters"] = json!([{ "id": "status", "op": "in", "values": ["draft"] }]);
        assert!(detail(ChartSpec::from_value(bad_status)).contains("accepts only"));

        let mut bad_currency = revenue_by_month();
        bad_currency["filters"] = json!([{ "id": "currency", "op": "in", "values": ["EURO"] }]);
        assert!(detail(ChartSpec::from_value(bad_currency)).contains("ISO 4217"));

        let mut bad_rate = revenue_by_month();
        bad_rate["filters"] = json!([{ "id": "vat_rate", "op": "not_in", "values": ["21%"] }]);
        assert!(detail(ChartSpec::from_value(bad_rate)).contains("basis points"));

        // An id is checked for shape here; whether it is THIS tenant's is
        // settled at evaluation, against the tenant's own records.
        let mut sql_shaped_id = revenue_by_month();
        sql_shaped_id["filters"] =
            json!([{ "id": "customer", "op": "in", "values": ["1' OR '1'='1"] }]);
        assert!(detail(ChartSpec::from_value(sql_shaped_id)).contains("record ids"));

        let mut fine = revenue_by_month();
        fine["filters"] = json!([{ "id": "customer", "op": "in", "values": ["abcDEF-123_xyz"] }]);
        assert!(ChartSpec::from_value(fine).is_ok());
    }

    #[test]
    fn a_chart_form_and_its_breakdown_have_to_agree() {
        let mut number_with_breakdown = revenue_by_month();
        number_with_breakdown["viz"] = json!("number");
        assert!(detail(ChartSpec::from_value(number_with_breakdown)).contains("no breakdown"));

        let mut bar_without = revenue_by_month();
        bar_without.as_object_mut().map(|o| o.remove("dimension"));
        bar_without["sort"] = json!({ "by": "value", "dir": "desc" });
        assert!(detail(ChartSpec::from_value(bar_without)).contains("needs a breakdown"));

        let mut line_over_categories = revenue_by_month();
        line_over_categories["viz"] = json!("line");
        line_over_categories["dimension"] = json!({ "id": "customer" });
        assert!(detail(ChartSpec::from_value(line_over_categories)).contains("over time"));

        let mut pie_over_time = revenue_by_month();
        pie_over_time["viz"] = json!("pie");
        assert!(detail(ChartSpec::from_value(pie_over_time)).contains("category breakdown"));

        // The single figure a Business overview tile leads with.
        let outstanding = json!({
            "schema_version": 1,
            "dataset": "billing.receivables",
            "measure": { "id": "outstanding", "agg": "sum" },
            "period": { "kind": "all" },
            "viz": "number"
        });
        assert!(ChartSpec::from_value(outstanding).is_ok());
    }

    #[test]
    fn sorting_by_a_breakdown_that_is_not_there_is_refused() {
        let mut value = revenue_by_month();
        value.as_object_mut().map(|o| o.remove("dimension"));
        value["viz"] = json!("number");
        assert!(detail(ChartSpec::from_value(value)).contains("cannot be sorted"));
    }

    #[test]
    fn the_limit_is_bounded_to_what_a_chart_can_show() {
        for bad in [0, MAX_CATEGORIES + 1, u32::MAX] {
            let mut value = revenue_by_month();
            value["limit"] = json!(bad);
            assert!(detail(ChartSpec::from_value(value)).contains("between 1"));
        }
    }

    #[test]
    fn a_period_narrows_on_the_date_the_chart_actually_means() {
        // The chart's own time breakdown, when it has one.
        let spec =
            ChartSpec::from_value(revenue_by_month()).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(spec.period_dimension(), Dimension::IssueDate);

        // No breakdown at all: the dataset's declared default.
        let outstanding = ChartSpec::from_value(json!({
            "schema_version": 1,
            "dataset": "billing.receivables",
            "measure": { "id": "outstanding", "agg": "sum" },
            "period": { "kind": "all" },
            "viz": "number"
        }))
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(outstanding.period_dimension(), Dimension::DueDate);

        // "Won this month" is about the day a deal closed, not the day it was
        // raised — so it says so, and the default does not win.
        let won = ChartSpec::from_value(json!({
            "schema_version": 1,
            "dataset": "crm.deals",
            "measure": { "id": "value", "agg": "sum" },
            "period": { "kind": "last_n", "n": 1, "grain": "month" },
            "period_on": "closed_at",
            "filters": [ { "id": "outcome", "op": "in", "values": ["won"] } ],
            "viz": "number"
        }))
        .unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(won.period_dimension(), Dimension::ClosedAt);
        // An explicit date beats the breakdown, not only the default.
        let mut over_breakdown = revenue_by_month();
        over_breakdown["dataset"] = json!("crm.deals");
        over_breakdown["measure"] = json!({ "id": "value", "agg": "sum" });
        over_breakdown["dimension"] = json!({ "id": "expected_close", "grain": "month" });
        over_breakdown["period_on"] = json!("created_at");
        over_breakdown["filters"] = json!([]);
        let spec =
            ChartSpec::from_value(over_breakdown).unwrap_or_else(|e| panic!("rejected: {e}"));
        assert_eq!(spec.period_dimension(), Dimension::CreatedAt);
    }

    #[test]
    fn a_period_cannot_narrow_on_something_that_is_not_the_datasets_date() {
        let mut foreign = revenue_by_month();
        foreign["period_on"] = json!("paid_on");
        assert!(detail(ChartSpec::from_value(foreign)).contains("has no"));

        let mut not_a_date = revenue_by_month();
        not_a_date["period_on"] = json!("customer");
        assert!(detail(ChartSpec::from_value(not_a_date)).contains("not a date"));
    }

    #[test]
    fn an_oversized_envelope_is_refused_before_it_is_parsed() {
        let mut value = revenue_by_month();
        value["filters"] = json!([{
            "id": "customer",
            "op": "in",
            "values": ["x".repeat(MAX_SPEC_BYTES)],
        }]);
        assert!(matches!(
            ChartSpec::from_value(value),
            Err(SpecError::TooLarge { .. })
        ));
    }
}
