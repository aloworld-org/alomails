//! Executing the **Insights** agent's tools (ADR 0034, queue item A2.4) — the
//! vocabulary, the figures, what moved between two periods, and the one write
//! that pins a board of questions.
//!
//! Everything here runs on the asker's own account door, so the agent reads
//! exactly the rows the person who asked could open. A [`ChartSpec`] has no
//! field that could name a tenant (ADR 0037), which is what makes handing a
//! model-written specification to the query engine safe at all: the worst it can
//! produce is a chart of the caller's own records, or a `422`.
//!
//! Four decisions are worth reading before changing anything here:
//!
//! - **The catalog is answered, not remembered.** [`execute_insight_catalog`]
//!   renders the closed vocabulary *from the catalog enums themselves*
//!   ([`insight_catalog::DATASETS`]), never from a list typed out here, so a
//!   measure added to the product is a measure the agent is offered on the next
//!   build. It is a second **shape** of the menu
//!   ([`alo_store::insight_prompt::catalog_prompt`] is the ask path's prose one)
//!   and deliberately not a second **source**: both iterate the same catalog.
//!   The shape differs because the consumers do — the prose menu is a system
//!   prompt of several thousand characters, and a tool result is cut to
//!   `agent_turn::MAX_RESULT_CHARS` before the model ever sees it, so what
//!   crosses here is compact JSON with a size test on it.
//! - **The spec is validated by the same gate a hand-built one meets**
//!   ([`ChartSpec::from_value`]), and its refusal is handed back verbatim: those
//!   messages name the offending field and the rule it broke, which is exactly
//!   what a model needs to correct itself on its next call.
//! - **A change is compared over aligned buckets only.** Two periods broken down
//!   by date have different bucket keys and nothing to compare, so
//!   [`execute_insight_change`] refuses a date breakdown rather than diffing
//!   January against July and calling the result a movement.
//! - **A report is built only from charts that answer.** Every one of its specs
//!   is validated *and evaluated* before the board is created, so an approved
//!   proposal cannot leave somebody looking at a board of broken tiles.

use axum::Json;
use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use alo_store::insight_catalog::{
    self, DATASETS, DimensionKind, Unit, ValueKind, Viz, dataset as catalog_entry,
};
use alo_store::insight_dashboards::NewDashboard;
use alo_store::insight_spec::{
    ChartSpec, MAX_CATEGORIES, MAX_FILTER_VALUES, MAX_FILTERS, MAX_TIME_BUCKETS,
};
use alo_store::insight_tiles::NewTile;
use alo_store::{InsightDashboardId, Series, SeriesGroup};

use crate::agent_args::{string_arg, unprocessable};
use crate::billing::map_store_err;
use crate::error::Problem;
use crate::insights_ask::span_for;
use crate::state::Account;

/// The most buckets one answer reports per series group. A chart question in a
/// room is "how much did we bill last quarter" or "revenue by month this year",
/// not four hundred daily points — and a result the turn has to cut mid-JSON is
/// a result the model cannot read at all.
const MAX_POINTS: usize = 40;

/// The most movements one change report names. Past this, the tail is noise: a
/// person asked what moved, and the answer is the few things that did.
const MAX_MOVERS: usize = 20;

/// The most charts one proposed report may carry. A board holds forty
/// ([`alo_store::insight_tiles::TILES_PER_DASHBOARD_MAX`]); a report somebody
/// asked for in a sentence is a handful, and a model that wanted thirty of them
/// has misread the question.
const MAX_REPORT_CHARTS: usize = 8;

/// A catalog enum's wire word, read back through serde so the menu can never
/// name something by a spelling the parser would refuse — the same rule
/// [`alo_store::insight_prompt`] states, for the same reason.
fn wire(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

/// The wire words of a list.
fn wires<T: serde::Serialize>(values: impl IntoIterator<Item = T>) -> Vec<String> {
    values.into_iter().map(|value| wire(&value)).collect()
}

/// The grammar of a specification, said once and said tersely.
///
/// Terse because it shares a four-thousand-character result with the whole
/// vocabulary, and the vocabulary is the half a model cannot infer: the shape
/// below is one template it can copy, the bounds come from the validator's own
/// constants so a ceiling that moves moves here too, and everything a sentence
/// of English would have added is in the tool's own description instead.
fn grammar() -> Value {
    json!({
        "shape": format!(
            "{{\"schema_version\":{},\"dataset\":<dataset>,\"measure\":{{\"id\":<measure>,\"agg\":<agg>}},\
             \"dimension\":{{\"id\":<breakdown>,\"grain\":<grain, dates only>}} (omit for one figure),\
             \"period\":{{\"kind\":\"last_n\",\"n\":1-{MAX_TIME_BUCKETS},\"grain\":<grain>}} | \
             {{\"kind\":\"range\",\"from\":\"YYYY-MM-DD\",\"to\":\"YYYY-MM-DD\"}} | {{\"kind\":\"all\"}},\
             \"period_on\":<a date breakdown> (optional),\
             \"filters\":[{{\"id\":<filter>,\"op\":\"in\"|\"not_in\",\"values\":[<string>]}}] (optional, \
             at most {MAX_FILTERS} of {MAX_FILTER_VALUES} values),\
             \"sort\":{{\"by\":\"dimension\"|\"value\",\"dir\":\"asc\"|\"desc\"}} (optional),\
             \"limit\":1-{MAX_CATEGORIES} (optional, categories kept),\"viz\":<viz>}}",
            alo_store::insight_spec::CHART_SPEC_SCHEMA_VERSION,
        ),
        "viz": wires([Viz::Number, Viz::Bar, Viz::Line, Viz::Pie, Viz::Table]),
        "drawing": "number takes NO dimension; line needs a date breakdown; pie needs a category one; bar and table take either",
        "reading": format!(
            "a measure's by lists the breakdowns it allows ({ALL_BREAKDOWNS} = every one of that \
             dataset's); a breakdown with grains is a date and needs one, a breakdown without is a \
             category and must not have one"
        ),
    })
}

/// What a measure's `by` says when it allows every breakdown its dataset has —
/// which most of them do. Written out for each measure it cost six hundred
/// characters of a four-thousand-character result, so it is said once here and
/// explained once in the grammar.
const ALL_BREAKDOWNS: &str = "\"all\"";

/// The comparisons every filter allows unless it says otherwise — stated once
/// in the menu rather than repeated on each of the dozen filters, which is
/// three hundred characters of a result that has four thousand.
const USUAL_OPS: &[&str] = &["in", "not_in"];

/// One dataset's whole vocabulary, generated from its catalog entry.
fn dataset_json(dataset: insight_catalog::Dataset) -> Value {
    let entry = catalog_entry(dataset);
    let every: Vec<String> = wires(entry.dimensions.iter().map(|d| d.dimension));
    let measures: Vec<Value> = entry
        .measures
        .iter()
        .map(|measure| {
            let by = wires(measure.dimensions.iter().copied());
            json!({
                "id": wire(&measure.measure),
                "unit": wire(&measure.unit),
                "aggs": wires(measure.aggregates.iter().copied()),
                "by": if by == every { json!("all") } else { json!(by) },
            })
        })
        .collect();
    // A date breakdown is the one that carries grains; a category one is
    // recognised by having none, which the grammar says in a sentence rather
    // than this list saying it fifteen times.
    let breakdowns: Vec<Value> = entry
        .dimensions
        .iter()
        .map(|dimension| match dimension.kind {
            DimensionKind::Time(grains) => json!({
                "id": wire(&dimension.dimension),
                "grains": wires(grains.iter().copied()),
            }),
            DimensionKind::Category => json!({ "id": wire(&dimension.dimension) }),
        })
        .collect();
    let filters: Vec<Value> = entry
        .filters
        .iter()
        .map(|filter| {
            let mut row = Map::new();
            row.insert("id".to_owned(), json!(wire(&filter.field)));
            // Only when this filter is not on the usual terms: a list repeated
            // a dozen times is a list nobody reads.
            let ops = wires(filter.operators.iter().copied());
            if ops != USUAL_OPS {
                row.insert("ops".to_owned(), json!(ops));
            }
            match filter.value {
                // Ids are listed as unusable rather than hidden: a model that
                // does not know they exist invents a filter name instead, and a
                // guessed id is a refusal the user has to read.
                ValueKind::Id => {
                    row.insert(
                        "values".to_owned(),
                        json!("ids — DO NOT USE, you know none"),
                    );
                }
                ValueKind::Currency => {
                    row.insert("values".to_owned(), json!("ISO 4217, e.g. EUR"));
                }
                ValueKind::Enum(allowed) => {
                    row.insert("values".to_owned(), json!(allowed));
                }
                ValueKind::RateBp => {
                    row.insert(
                        "values".to_owned(),
                        json!("VAT basis points, e.g. \"2100\""),
                    );
                }
                ValueKind::Text => {
                    row.insert("values".to_owned(), json!("the workspace's own words"));
                }
            }
            Value::Object(row)
        })
        .collect();
    json!({
        "id": wire(&dataset),
        "period_on": wire(&entry.period),
        "measures": measures,
        "breakdowns": breakdowns,
        "filters": filters,
    })
}

/// The whole menu, in catalog order. Deterministic: the same value on every
/// call, from the same enums the validator enforces.
fn catalog_json() -> Value {
    json!({
        "kind": "insightCatalog",
        "spec": grammar(),
        "filterOps": USUAL_OPS,
        "datasets": DATASETS.iter().map(|&d| dataset_json(d)).collect::<Vec<_>>(),
    })
}

/// `insight_catalog` — what this workspace can measure.
///
/// Reads no rows at all: the vocabulary is the product's, not the tenant's, and
/// it is the same list for everyone who can open Insights. It still runs behind
/// the same authenticated boundary and the same product scope as every other
/// tool, because a catalog is only useful to an agent that may go on to ask a
/// question with it.
///
/// # Errors
/// Never — the signature matches every other executor's so the dispatcher stays
/// one shape.
#[allow(clippy::unused_async)]
pub async fn execute_insight_catalog(
    _account: &Account,
    _args: &Value,
) -> Result<Json<Value>, Problem> {
    Ok(Json(json!({ "ok": true, "result": catalog_json() })))
}

/// Reads a specification argument through the same write gate a hand-built one
/// meets, keeping the raw JSON so a caller can restate it over another period.
///
/// The refusal is [`alo_store::insight_spec::SpecError`]'s own sentence: it
/// names the offending field and the rule it broke ("the billing.documents
/// dataset does not offer the value measure"), quotes the caller's own input at
/// most, and never stored data — which is exactly what may be handed back to a
/// model.
fn read_spec(raw: Value) -> Result<ChartSpec, Problem> {
    ChartSpec::from_value(raw)
        .map_err(|error| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))
}

/// The `spec` argument, raw and parsed. Shared with `pin_chart`
/// ([`crate::insights_intents`]), so a pinned chart meets exactly the gate a
/// reported one does.
pub(crate) fn spec_arg(args: &Value) -> Result<(Value, ChartSpec), Problem> {
    let raw = args
        .get("spec")
        .filter(|spec| spec.is_object())
        .cloned()
        .ok_or_else(|| unprocessable("spec is required: there is no question without one"))?;
    let spec = read_spec(raw.clone())?;
    Ok((raw, spec))
}

/// What a spec is asking, in the words the catalog uses — repeated on every
/// answer so a figure never travels without its question. Shared with
/// `dashboard_tiles` ([`crate::insights_intents`]), so a tile's question reads
/// the same way an answer's does.
pub(crate) fn asked(spec: &ChartSpec) -> Value {
    json!({
        "dataset": wire(&spec.dataset),
        "measure": wire(&spec.measure.id),
        "agg": wire(&spec.measure.agg),
        "breakdown": spec.dimension.map(|d| wire(&d.id)),
        "grain": spec.dimension.and_then(|d| d.grain).map(|g| wire(&g)),
        "period": spec.period,
        "filters": spec.filters.len(),
    })
}

/// One group's points, cut to [`MAX_POINTS`] and saying how many were left out.
///
/// A time series keeps its **last** points and a category one its **first**: a
/// date breakdown arrives oldest-first and the recent end is what a question is
/// about, while a category breakdown arrives biggest-first and the tail is
/// already the part that matters least.
fn points_json(group: &SeriesGroup, over_time: bool) -> Value {
    let total = group.points.len();
    let kept: Vec<&alo_store::Point> = if total <= MAX_POINTS {
        group.points.iter().collect()
    } else if over_time {
        group.points.iter().skip(total - MAX_POINTS).collect()
    } else {
        group.points.iter().take(MAX_POINTS).collect()
    };
    json!({
        "key": group.key,
        "label": group.label,
        "points": kept
            .iter()
            .map(|point| json!({
                "bucket": point.bucket,
                "label": point.label,
                "value": point.value,
            }))
            .collect::<Vec<_>>(),
        "omitted": total - kept.len(),
    })
}

/// Whether this spec breaks its measure down by a date.
fn over_time(spec: &ChartSpec) -> bool {
    spec.dimension.is_some_and(|dimension| {
        catalog_entry(spec.dataset)
            .dimension(dimension.id)
            .is_some_and(|entry| matches!(entry.kind, DimensionKind::Time(_)))
    })
}

/// `insight_answer` — the figures one specification asks for.
///
/// # Errors
/// 422 when there is no spec, when the spec breaks a catalog rule or a bound, or
/// when the evaluation is refused (an id that is not this tenant's, a period
/// scanning more rows than the caps allow).
pub async fn execute_insight_answer(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (_, spec) = spec_arg(args)?;
    // Through the same store function `POST /insights/eval` reads, so a figure
    // the agent says and a figure the board draws cannot disagree: the
    // arithmetic is the store's, in the same code the printed invoice and the
    // VAT return use.
    let series = account
        .acc
        .insight_evaluate(&spec)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": answer_json(&spec, &series),
    })))
}

/// One evaluated series, rendered for the model.
fn answer_json(spec: &ChartSpec, series: &Series) -> Value {
    let over_time = over_time(spec);
    json!({
        "kind": "insightAnswer",
        "asked": asked(spec),
        "unit": series.unit,
        "series": series
            .groups
            .iter()
            .map(|group| points_json(group, over_time))
            .collect::<Vec<_>>(),
        "notes": series.notes,
        // The store's own word: a category tail was folded into one `other`
        // bucket rather than dropped.
        "truncated": series.truncated,
    })
}

/// The period argument of a change, put in the spec's place so the earlier read
/// meets **the whole validator** — its bounds included — rather than being
/// spliced into a struct behind its back.
fn against_spec(raw: &Value, args: &Value) -> Result<ChartSpec, Problem> {
    let against = args
        .get("against")
        .filter(|period| period.is_object())
        .cloned()
        .ok_or_else(|| {
            unprocessable("against is required: a change is a comparison with an earlier period")
        })?;
    let mut earlier = raw.clone();
    let Some(object) = earlier.as_object_mut() else {
        return Err(unprocessable("spec must be a chart specification"));
    };
    object.insert("period".to_owned(), against);
    read_spec(earlier)
}

/// `insight_change` — the same question over two periods, and what moved.
///
/// # Errors
/// 422 when either specification is refused, when `against` is missing, or when
/// the breakdown is by date — two periods bucketed by date share no bucket keys,
/// so there is nothing to compare and a diff of them would be arithmetic
/// dressed as an explanation.
pub async fn execute_insight_change(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let (raw, spec) = spec_arg(args)?;
    if over_time(&spec) {
        return Err(unprocessable(
            "a change is compared over the same buckets, so the breakdown must be a category or \
             none at all — ask for the two periods with a category breakdown instead of a date one",
        ));
    }
    let earlier = against_spec(&raw, args)?;
    let now = account
        .acc
        .insight_evaluate(&spec)
        .await
        .map_err(map_store_err)?;
    let before = account
        .acc
        .insight_evaluate(&earlier)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "ok": true,
        "result": change_json(&spec, &earlier, &now, &before),
    })))
}

/// One bucket of one group, in both periods.
struct Movement {
    group: String,
    bucket: String,
    label: Value,
    now: i64,
    before: i64,
}

impl Movement {
    /// How much it moved. `now - before`, so a fall is negative — the sign is
    /// the answer, and a magnitude on its own would need a word for direction
    /// that the model would have to supply.
    const fn change(&self) -> i64 {
        self.now - self.before
    }
}

/// Both series, aligned bucket by bucket.
///
/// A bucket present in only one period is kept, with `0` for the period it is
/// missing from: a customer who bought nothing last quarter and a great deal
/// this one is precisely the movement being asked about, and dropping the row
/// would answer that nothing happened.
fn movements(now: &Series, before: &Series) -> Vec<Movement> {
    let mut rows: Vec<Movement> = Vec::new();
    for group in &now.groups {
        for point in &group.points {
            rows.push(Movement {
                group: group.key.clone(),
                bucket: point.bucket.clone(),
                label: json!(point.label),
                now: point.value,
                before: 0,
            });
        }
    }
    for group in &before.groups {
        for point in &group.points {
            if let Some(row) = rows
                .iter_mut()
                .find(|row| row.group == group.key && row.bucket == point.bucket)
            {
                row.before = point.value;
            } else {
                rows.push(Movement {
                    group: group.key.clone(),
                    bucket: point.bucket.clone(),
                    label: json!(point.label),
                    now: 0,
                    before: point.value,
                });
            }
        }
    }
    // Biggest movement first, either way: a fall of a thousand is as much of an
    // answer as a rise of one. Ties keep a stable order so the same two periods
    // always read the same way.
    rows.sort_by(|a, b| {
        b.change()
            .abs()
            .cmp(&a.change().abs())
            .then_with(|| a.group.cmp(&b.group))
            .then_with(|| a.bucket.cmp(&b.bucket))
    });
    rows
}

/// The total of each group in both periods — money and counts only.
///
/// A ratio in basis points is deliberately absent: adding win rates together is
/// not a win rate, and a total nobody can check is worse than no total.
fn totals_json(unit: Unit, rows: &[Movement]) -> Option<Value> {
    if matches!(unit, Unit::PercentBp) {
        return None;
    }
    let mut groups: Vec<(String, i64, i64)> = Vec::new();
    for row in rows {
        if let Some(entry) = groups.iter_mut().find(|(key, _, _)| *key == row.group) {
            entry.1 += row.now;
            entry.2 += row.before;
        } else {
            groups.push((row.group.clone(), row.now, row.before));
        }
    }
    Some(json!(
        groups
            .iter()
            .map(|(key, now, before)| json!({
                "key": key,
                "now": now,
                "before": before,
                "change": now - before,
            }))
            .collect::<Vec<_>>()
    ))
}

/// The comparison, rendered for the model.
fn change_json(spec: &ChartSpec, earlier: &ChartSpec, now: &Series, before: &Series) -> Value {
    let rows = movements(now, before);
    let totals = totals_json(now.unit.kind, &rows);
    let omitted = rows.len().saturating_sub(MAX_MOVERS);
    json!({
        "kind": "insightChange",
        "asked": asked(spec),
        "against": earlier.period,
        "unit": now.unit,
        "totals": totals,
        "movers": rows
            .iter()
            .take(MAX_MOVERS)
            .map(|row| json!({
                "key": row.group,
                "bucket": row.bucket,
                "label": row.label,
                "now": row.now,
                "before": row.before,
                "change": row.change(),
            }))
            .collect::<Vec<_>>(),
        "omitted": omitted,
        // Both periods' notes, kept apart: "two documents could not be
        // restated" is true of one period, and merging them would misreport
        // which figure is incomplete.
        "notes": { "now": now.notes, "before": before.notes },
    })
}

/// One chart of a proposed report, once it has been read and answered.
struct Chart {
    title: String,
    spec: Value,
    viz: Viz,
}

/// `insight_report` — a named board of saved charts.
///
/// The only write in this product's set, and it writes **questions**: a board
/// and its tiles, each carrying a spec the caller could have built by hand. It
/// changes no figure and no record any figure is read from.
///
/// Every chart is validated *and evaluated* before the board is created, so an
/// approved proposal cannot leave somebody looking at a board of broken tiles —
/// and the refusal names the chart by its own title.
///
/// # Errors
/// 422 when the report has no name, no charts, more than
/// [`MAX_REPORT_CHARTS`], or a chart the validator or the query engine refuses;
/// the store's own error when the tenant is at its board or tile ceiling.
pub async fn execute_insight_report(
    account: &Account,
    args: &Value,
) -> Result<Json<Value>, Problem> {
    let name = string_arg(args, "name")
        .ok_or_else(|| unprocessable("name is required: a report is something a person opens"))?;
    let charts = args
        .get("charts")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            unprocessable("charts is required: a report with no chart is not a report")
        })?;
    if charts.is_empty() {
        return Err(unprocessable("a report needs at least one chart"));
    }
    if charts.len() > MAX_REPORT_CHARTS {
        return Err(unprocessable(format!(
            "a proposed report carries at most {MAX_REPORT_CHARTS} charts; this one has {}",
            charts.len()
        )));
    }

    // Everything checked BEFORE a single row is written: a half-built board is
    // worse than none, and the refusal has to be able to name the chart.
    let mut prepared: Vec<Chart> = Vec::new();
    for (index, chart) in charts.iter().enumerate() {
        let position = index + 1;
        let title = string_arg(chart, "title")
            .ok_or_else(|| unprocessable(format!("chart {position} has no title")))?;
        let raw = chart
            .get("spec")
            .filter(|spec| spec.is_object())
            .cloned()
            .ok_or_else(|| unprocessable(format!("chart {position} ({title}) has no spec")))?;
        let spec = read_spec(raw.clone()).map_err(|problem| {
            unprocessable(format!(
                "chart {position} ({title}): {}",
                problem.detail.as_deref().unwrap_or("the spec was refused")
            ))
        })?;
        // A tile that cannot be answered is a tile nobody wants pinned. This is
        // the same evaluation the board itself will run when it is opened.
        account
            .acc
            .insight_evaluate(&spec)
            .await
            .map_err(|error| unprocessable(format!("chart {position} ({title}): {error}")))?;
        prepared.push(Chart {
            title,
            spec: raw,
            viz: spec.viz,
        });
    }

    let dashboard: InsightDashboardId = account
        .acc
        .create_insight_dashboard(&NewDashboard { name: name.clone() })
        .await
        .map_err(map_store_err)?;
    let mut pinned: Vec<Value> = Vec::new();
    for chart in prepared {
        let tile = account
            .acc
            .create_insight_tile(
                &dashboard,
                &NewTile {
                    title: chart.title.clone(),
                    spec: chart.spec,
                    span: span_for(chart.viz),
                },
            )
            .await
            .map_err(map_store_err)?;
        pinned.push(json!({
            "id": tile.as_str(),
            "title": chart.title,
            "viz": wire(&chart.viz),
        }));
    }
    Ok(Json(json!({
        "ok": true,
        "result": {
            "kind": "insightReport",
            "report": { "id": dashboard.as_str(), "name": name },
            "charts": pinned,
        }
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alo_store::insight_catalog::{Dataset, Dimension, Grain, Measure};
    use alo_store::insight_series::Label;
    use alo_store::{Point, SeriesUnit};

    /// The menu is generated from the catalog, so every word the validator
    /// accepts is a word the agent was offered. A measure the catalog gained
    /// and this rendering never mentioned would be a feature nobody could ask
    /// for — the same property [`alo_store::insight_prompt`] holds its prose
    /// menu to.
    #[test]
    fn the_menu_offers_the_whole_catalog() {
        let menu = catalog_json().to_string();
        for &dataset in DATASETS {
            let entry = catalog_entry(dataset);
            assert!(menu.contains(&wire(&dataset)), "{dataset:?} is not offered");
            for measure in entry.measures {
                assert!(
                    menu.contains(&wire(&measure.measure)),
                    "{:?} is not offered",
                    measure.measure
                );
                for agg in measure.aggregates {
                    assert!(menu.contains(&wire(agg)), "{agg:?} is not offered");
                }
            }
            for dimension in entry.dimensions {
                assert!(
                    menu.contains(&wire(&dimension.dimension)),
                    "{:?} is not offered",
                    dimension.dimension
                );
                if let DimensionKind::Time(grains) = dimension.kind {
                    for grain in grains {
                        assert!(menu.contains(&wire(grain)), "{grain:?} is not offered");
                    }
                }
            }
            for filter in entry.filters {
                assert!(
                    menu.contains(&wire(&filter.field)),
                    "{:?} is not offered",
                    filter.field
                );
                if let ValueKind::Enum(allowed) = filter.value {
                    for value in allowed {
                        assert!(menu.contains(value), "{value} is not offered");
                    }
                }
            }
        }
    }

    /// **The menu fits in a tool result**, checked against the turn's own bound
    /// rather than a copy of it. A catalog cut in half is a catalog with
    /// invented spellings at the end of it, and the model would have no way to
    /// know. If a wave pushes this over, the fix is a narrower menu — one
    /// dataset at a time, on request — not a bigger prompt nobody reads.
    #[test]
    fn the_menu_fits_in_one_tool_result() {
        let menu = catalog_json().to_string();
        assert!(
            menu.chars().count() < crate::agent_turn::MAX_RESULT_CHARS,
            "the catalog is {} characters and would be truncated",
            menu.chars().count()
        );
    }

    /// A model does not know this workspace's record ids, so the filters that
    /// take one are offered as unusable rather than silently listed.
    #[test]
    fn no_filter_invites_an_identifier() {
        let menu = catalog_json().to_string();
        assert!(menu.contains("DO NOT USE"));
        assert!(!menu.contains("SELECT"), "no SQL is ever named");
    }

    fn spec_value(dimension: Option<Value>, viz: &str) -> Value {
        let mut spec = json!({
            "schema_version": 1,
            "dataset": "billing.documents",
            "measure": { "id": "net", "agg": "sum" },
            "period": { "kind": "last_n", "n": 3, "grain": "month" },
            "viz": viz,
        });
        if let Some(dimension) = dimension {
            spec["dimension"] = dimension;
        }
        spec
    }

    /// The spec argument meets the same gate a hand-built one does, and a
    /// refusal keeps the validator's own sentence — which is what lets a model
    /// correct itself rather than guess again.
    #[test]
    fn a_spec_is_read_through_the_write_gate() {
        let (raw, spec) = spec_arg(&json!({ "spec": spec_value(None, "number") })).unwrap();
        assert_eq!(spec.dataset, Dataset::BillingDocuments);
        assert_eq!(spec.measure.id, Measure::Net);
        assert!(raw.is_object());

        let missing = spec_arg(&json!({})).unwrap_err();
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(missing.detail.unwrap().contains("spec is required"));
        // A spec that is not an object at all is the same refusal, not a panic.
        assert!(spec_arg(&json!({ "spec": "revenue" })).is_err());

        // The validator's words survive the trip.
        let mut bad = spec_value(None, "number");
        bad["measure"]["id"] = json!("profit");
        let refused = spec_arg(&json!({ "spec": bad })).unwrap_err();
        assert!(refused.detail.unwrap().contains("profit"));
    }

    /// A date breakdown and a category breakdown are told apart from the
    /// catalog, not from the field's name — which is what
    /// [`execute_insight_change`] refuses on.
    #[test]
    fn a_date_breakdown_is_recognised_from_the_catalog() {
        let (_, over_dates) = spec_arg(&json!({
            "spec": spec_value(Some(json!({ "id": "issue_date", "grain": "month" })), "line")
        }))
        .unwrap();
        assert!(over_time(&over_dates));
        let (_, by_customer) = spec_arg(&json!({
            "spec": spec_value(Some(json!({ "id": "customer" })), "bar")
        }))
        .unwrap();
        assert!(!over_time(&by_customer));
        let (_, one_figure) = spec_arg(&json!({ "spec": spec_value(None, "number") })).unwrap();
        assert!(!over_time(&one_figure));
    }

    /// The earlier period is put **in the spec** and revalidated, so a
    /// comparison cannot reach further back than a chart may.
    #[test]
    fn the_earlier_period_meets_the_validator_too() {
        let raw = spec_value(Some(json!({ "id": "customer" })), "bar");
        let earlier = against_spec(
            &raw,
            &json!({ "against": { "kind": "range", "from": "2026-01-01", "to": "2026-03-31" } }),
        )
        .unwrap();
        assert_eq!(
            earlier.period,
            alo_store::insight_spec::Period::Range {
                from: "2026-01-01".to_owned(),
                to: "2026-03-31".to_owned(),
            }
        );
        // Everything else is the question that was asked.
        assert_eq!(earlier.dataset, Dataset::BillingDocuments);
        assert_eq!(earlier.dimension.map(|d| d.id), Some(Dimension::Customer));

        // A period wider than the validator allows is refused here rather than
        // scanned — the whole reason the period is revalidated instead of
        // being written into the struct.
        let too_wide = against_spec(
            &raw,
            &json!({ "against": { "kind": "range", "from": "2000-01-01", "to": "2026-01-01" } }),
        )
        .unwrap_err();
        assert_eq!(too_wide.status, StatusCode::UNPROCESSABLE_ENTITY);
        // …and a missing one says what a change actually is.
        let absent = against_spec(&raw, &json!({})).unwrap_err();
        assert!(absent.detail.unwrap().contains("earlier period"));
    }

    fn point(bucket: &str, value: i64) -> Point {
        Point {
            bucket: bucket.to_owned(),
            label: Some(Label::Raw {
                text: format!("{bucket} Ltd"),
            }),
            value,
        }
    }

    fn series(unit: Unit, currency: Option<&str>, points: Vec<Point>) -> Series {
        Series {
            unit: SeriesUnit {
                kind: unit,
                currency: currency.map(str::to_owned),
            },
            groups: vec![SeriesGroup {
                key: "EUR".to_owned(),
                label: Label::Raw {
                    text: "EUR".to_owned(),
                },
                points,
            }],
            notes: Vec::new(),
            truncated: false,
        }
    }

    /// The comparison itself: aligned buckets, biggest movement first, a bucket
    /// missing from one period counted as zero rather than dropped.
    #[test]
    fn what_moved_is_aligned_ordered_and_never_silently_dropped() {
        let now = series(
            Unit::Money,
            Some("EUR"),
            vec![
                point("acme", 120_000),
                point("brio", 30_000),
                point("new", 90_000),
            ],
        );
        let before = series(
            Unit::Money,
            Some("EUR"),
            vec![
                point("acme", 100_000),
                point("brio", 95_000),
                point("gone", 40_000),
            ],
        );
        let rows = movements(&now, &before);
        let seen: Vec<(&str, i64, i64, i64)> = rows
            .iter()
            .map(|row| (row.bucket.as_str(), row.before, row.now, row.change()))
            .collect();
        assert_eq!(
            seen,
            [
                ("new", 0, 90_000, 90_000),
                ("brio", 95_000, 30_000, -65_000),
                ("gone", 40_000, 0, -40_000),
                ("acme", 100_000, 120_000, 20_000),
            ],
            "biggest movement first, either direction, nothing dropped"
        );

        // The totals are the two periods' own sums, and their difference.
        let totals = totals_json(Unit::Money, &rows).unwrap();
        assert_eq!(totals[0]["key"], "EUR");
        assert_eq!(totals[0]["now"], 240_000);
        assert_eq!(totals[0]["before"], 235_000);
        assert_eq!(totals[0]["change"], 5_000);
    }

    /// Basis points are never added up: a total of win rates is not a win rate.
    #[test]
    fn a_ratio_has_no_total() {
        let rows = movements(
            &series(Unit::PercentBp, None, vec![point("q1", 4_000)]),
            &series(Unit::PercentBp, None, vec![point("q1", 5_000)]),
        );
        assert_eq!(rows[0].change(), -1_000);
        assert!(totals_json(Unit::PercentBp, &rows).is_none());
        assert!(totals_json(Unit::Count, &rows).is_some());
    }

    /// A long answer is cut at the end that matters least, and says how much it
    /// left out — never silently, because a series that stops early reads as a
    /// business that stopped trading.
    #[test]
    fn a_long_answer_is_cut_where_it_matters_least_and_says_so() {
        let many: Vec<Point> = (0..MAX_POINTS + 5)
            .map(|i| point(&format!("b{i:03}"), i64::try_from(i).unwrap()))
            .collect();
        let group = &series(Unit::Count, None, many).groups[0];

        let over_time = points_json(group, true);
        assert_eq!(over_time["omitted"], 5);
        assert_eq!(over_time["points"].as_array().unwrap().len(), MAX_POINTS);
        // The most recent buckets survive.
        assert_eq!(over_time["points"][MAX_POINTS - 1]["bucket"], "b044");
        assert_eq!(over_time["points"][0]["bucket"], "b005");

        let by_category = points_json(group, false);
        assert_eq!(by_category["omitted"], 5);
        // The biggest categories survive, which is the order they arrive in.
        assert_eq!(by_category["points"][0]["bucket"], "b000");

        // Nothing is cut when nothing needs to be, and the label travels with
        // the bucket so a customer can be named.
        let short = points_json(
            &series(Unit::Count, None, vec![point("acme", 1)]).groups[0],
            false,
        );
        assert_eq!(short["omitted"], 0);
        assert_eq!(short["points"][0]["label"]["text"], "acme Ltd");
    }

    /// Every answer repeats the question it answers: the measure, the dataset
    /// and the period a figure came from travel with the figure.
    #[test]
    fn a_figure_never_travels_without_its_question() {
        let (_, spec) = spec_arg(&json!({
            "spec": spec_value(Some(json!({ "id": "issue_date", "grain": "month" })), "line")
        }))
        .unwrap();
        let answer = answer_json(
            &spec,
            &series(Unit::Money, Some("EUR"), vec![point("2026-07", 1)]),
        );
        assert_eq!(answer["kind"], "insightAnswer");
        assert_eq!(answer["asked"]["dataset"], "billing.documents");
        assert_eq!(answer["asked"]["measure"], "net");
        assert_eq!(answer["asked"]["breakdown"], "issue_date");
        assert_eq!(answer["asked"]["grain"], "month");
        assert_eq!(answer["asked"]["period"]["kind"], "last_n");
        assert_eq!(answer["unit"]["kind"], "money");
        assert_eq!(answer["unit"]["currency"], "EUR");
        assert_eq!(answer["series"][0]["points"][0]["value"], 1);
    }

    /// The grain a chart is drawn at reaches the tile's width, so a proposed
    /// report lays out like one a person built by hand.
    #[test]
    fn a_single_figure_is_a_small_tile_and_a_chart_is_a_wide_one() {
        assert_eq!(span_for(Viz::Number), 1);
        assert_eq!(span_for(Viz::Bar), 2);
        assert_eq!(span_for(Viz::Line), 2);
    }

    /// The catalog answers without touching a row, so the same value comes back
    /// every time — a prompt that changed between two calls would make a
    /// fixture test meaningless.
    #[test]
    fn the_catalog_is_deterministic() {
        assert_eq!(catalog_json(), catalog_json());
        assert_eq!(catalog_json()["kind"], "insightCatalog");
        assert_eq!(
            catalog_json()["datasets"].as_array().unwrap().len(),
            DATASETS.len()
        );
    }

    /// The grammar names the bounds the validator actually enforces, read from
    /// its constants rather than typed out beside them.
    #[test]
    fn the_grammar_carries_the_validators_own_bounds() {
        let grammar = grammar().to_string();
        assert!(grammar.contains(&MAX_TIME_BUCKETS.to_string()));
        assert!(grammar.contains(&MAX_CATEGORIES.to_string()));
        assert!(grammar.contains(&MAX_FILTERS.to_string()));
        assert!(grammar.contains(&MAX_FILTER_VALUES.to_string()));
        assert!(grammar.contains(&wire(&Grain::Month)) || grammar.contains("grain"));
    }
}
