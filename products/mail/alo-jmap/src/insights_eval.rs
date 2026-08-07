//! Evaluating an alo Insights chart (ADR 0037, wave BI1.04) — the two routes
//! that turn a question into figures, over [`alo_store::insight_query`].
//!
//! Kept apart from [`crate::insights`] because they are two responsibilities:
//! that module stores and returns *questions*, this one answers them by reading
//! the tenant's documents. Nothing is stored here, and nothing is computed here
//! either — the arithmetic belongs to the store, in the same functions the
//! printed invoice and the VAT return use, so a tile and a tax return cannot
//! disagree about a cent.
//!
//! The series crosses the wire in the shape the store defines
//! (`docs/design/insights.md` § The series that comes back): integer values, a
//! declared unit, ISO bucket keys, and labels that are either catalog ids the
//! client translates or the tenant's own words. It is serialised straight from
//! [`alo_store::Series`] rather than rebuilt here — one definition of the
//! contract, in the one place that can keep it true.
//!
//! **A spec is not a capability.** The tenant comes from the account door and a
//! ChartSpec has no field that could name one, so an ad-hoc spec — including
//! one a model proposed (BI1.07) — can only ever read the caller's own rows.
//! A filter naming another tenant's customer is a `422`, never a chart that is
//! quietly empty.

use std::time::Instant;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use alo_store::insight_spec::ChartSpec;
use alo_store::insight_tiles::TileSpec;
use alo_store::{AccountStore, InsightTileId, Series};

use crate::billing::{map_store_err, parse_body};
use crate::error::Problem;
use crate::insights::load_tile;
use crate::state::{AppState, authenticate};

/// The body of `POST /insights/eval`: the question, wrapped.
///
/// Wrapped rather than the bare envelope so the request can grow additively —
/// the ask (BI1.07) answers a spec *and* its preview, and a later `asOf` for
/// re-reading a period as it stood is a field here, not a second route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalBody {
    #[serde(default)]
    spec: Option<Value>,
}

/// Parses a wire spec, or the `422` naming the field it broke.
///
/// [`alo_store::insight_spec::SpecError`] messages are written for exactly this
/// purpose: they name the offending field and the rule — an unknown measure, an
/// incompatible pairing, a bound and its maximum — so the builder UI, or a
/// model on its one repair attempt, can act on the refusal. They quote the
/// caller's own input at most and never stored data.
fn read_spec(raw: Value) -> Result<ChartSpec, Problem> {
    ChartSpec::from_value(raw)
        .map_err(|error| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))
}

/// Evaluates a spec against the tenant's rows and answers the series, as the
/// JSON body its callers send — the two routes here, and the ask (BI1.07),
/// which previews a proposed spec through exactly this path.
///
/// The one place either route computes anything, so the instrumentation lives
/// here too — and carries **catalog ids and integers only**: which dataset,
/// which measure, how many buckets came back, how long it took. Filter values
/// (customer ids, the tenant's own words) and every figure are deliberately
/// absent: our logs are held to the promise we sell.
pub(crate) async fn evaluate(acc: &AccountStore, spec: &ChartSpec) -> Result<Value, Problem> {
    let started = Instant::now();
    let series = acc.insight_evaluate(spec).await.map_err(map_store_err)?;
    tracing::debug!(
        dataset = wire(&spec.dataset),
        measure = wire(&spec.measure.id),
        dimension = spec.dimension.map(|d| wire(&d.id)),
        buckets = buckets(&series),
        truncated = series.truncated,
        ms = started.elapsed().as_millis(),
        "insights eval"
    );
    serde_json::to_value(&series).map_err(|_| Problem::server_error())
}

/// A catalog enum's wire word, read back through serde so a log line can never
/// drift from the vocabulary the spec speaks.
fn wire(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

/// How many points the answer holds, across every group.
fn buckets(series: &Series) -> usize {
    series.groups.iter().map(|g| g.points.len()).sum()
}

/// `POST /insights/eval` `{spec}` → the series — the builder's live preview.
///
/// Stores nothing: a question is only a tile once somebody pins it
/// (`POST /insights/dashboards/{id}/tiles`). That separation is what makes the
/// ask (BI1.07) propose-then-approve rather than a model writing to a board.
pub async fn eval(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: EvalBody = parse_body(&body)?;
    let raw = req.spec.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "spec is required: there is nothing to evaluate without a question",
        )
    })?;
    let spec = read_spec(raw)?;
    Ok(Json(evaluate(&account.acc, &spec).await?))
}

/// `GET /insights/tiles/{id}/data` → the series for a stored tile.
///
/// The figures only — the tile itself came with the board it is pinned to, so
/// repeating it here would be a second copy of a record the caller already
/// holds, and the two could disagree. A tile of another tenant is the same
/// `404` an id that never existed gets.
///
/// A tile whose stored spec this build cannot read is a `422` here, and that is
/// not a contradiction of the rule that such a tile still *renders*: a board
/// shows it as a placeholder with the reason, and asking for its numbers is the
/// one thing that cannot be answered honestly.
pub async fn tile_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let tile = load_tile(&account.acc, &InsightTileId::new(id)).await?;
    match &tile.spec {
        TileSpec::Readable(spec) => Ok(Json(evaluate(&account.acc, spec).await?)),
        TileSpec::Unreadable { reason, .. } => Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("this tile's stored chart spec cannot be read by this version: {reason}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::insight_series::{Label, Point, SeriesGroup, SeriesUnit};
    use alo_store::{Aggregate, Dataset, Measure, Unit};
    use serde_json::json;

    fn body(json: Value) -> EvalBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    /// Revenue by month — the spec the eval tests bend.
    fn revenue() -> Value {
        json!({
            "schema_version": 1,
            "dataset": "billing.documents",
            "measure": { "id": "net", "agg": "sum" },
            "dimension": { "id": "issue_date", "grain": "month" },
            "period": { "kind": "last_n", "n": 12, "grain": "month" },
            "viz": "bar"
        })
    }

    #[test]
    fn a_wire_spec_is_read_through_the_same_gate_a_tile_is() {
        let spec = read_spec(revenue()).unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(spec.dataset, Dataset::BillingDocuments);
        assert_eq!(spec.measure.id, Measure::Net);
        assert_eq!(spec.measure.agg, Aggregate::Sum);
    }

    /// The refusal a caller has to be able to act on, in both its shapes.
    fn refusal(spec: Value) -> String {
        let refused = read_spec(spec).err().unwrap_or_else(|| {
            panic!("a spec outside the catalog must be refused");
        });
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        refused.detail.unwrap_or_default()
    }

    #[test]
    fn an_invented_measure_is_refused_with_the_vocabulary_it_should_have_used() {
        // A word from nowhere is caught by the *shape*, and the message hands
        // back the closed set — which is what a builder UI, or a model on its
        // one repair attempt, needs to try again.
        let mut invented = revenue();
        invented["measure"] = json!({ "id": "profit", "agg": "sum" });
        let detail = refusal(invented);
        assert!(detail.contains("profit"), "{detail}");
        assert!(
            detail.contains("net") && detail.contains("win_rate"),
            "{detail}"
        );

        // A word that exists but is not this dataset's is caught by the
        // *catalog*, and that message names the field and both sides of the
        // pairing it refused.
        let mut misplaced = revenue();
        misplaced["measure"] = json!({ "id": "win_rate", "agg": "ratio" });
        let detail = refusal(misplaced);
        assert!(detail.starts_with("measure:"), "{detail}");
        assert!(detail.contains("billing.documents"), "{detail}");
    }

    #[test]
    fn an_incompatible_pairing_is_a_422_and_never_an_empty_chart() {
        let mut crossed = revenue();
        crossed["dataset"] = json!("crm.deals");
        crossed["measure"] = json!({ "id": "value", "agg": "sum" });
        crossed["dimension"] = json!({ "id": "vat_rate" });
        let refused = read_spec(crossed).err().unwrap_or_else(|| {
            panic!("deal value by VAT rate is not an odd chart, it is a refusal");
        });
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn a_body_without_a_question_states_what_is_missing() {
        assert!(body(json!({})).spec.is_none());
        assert!(body(json!({ "spec": revenue() })).spec.is_some());
    }

    #[test]
    fn the_series_crosses_in_the_shape_the_design_note_publishes() {
        let series = Series {
            unit: SeriesUnit {
                kind: Unit::Money,
                currency: Some("EUR".to_owned()),
            },
            groups: vec![SeriesGroup {
                key: "EUR".to_owned(),
                label: Label::Raw {
                    text: "EUR".to_owned(),
                },
                points: vec![Point {
                    bucket: "2026-01".to_owned(),
                    label: None,
                    value: 1_234_567,
                }],
            }],
            notes: vec![],
            truncated: false,
        };
        let wire = serde_json::to_value(&series).unwrap_or(Value::Null);
        assert_eq!(wire["unit"], json!({ "kind": "money", "currency": "EUR" }));
        assert_eq!(wire["series"][0]["key"], json!("EUR"));
        assert_eq!(
            wire["series"][0]["points"][0],
            json!({ "bucket": "2026-01", "value": 1_234_567 }),
            "a time bucket carries no label: its key already says everything"
        );
        assert_eq!(wire["truncated"], json!(false));
        assert_eq!(buckets(&series), 1);
    }
}
