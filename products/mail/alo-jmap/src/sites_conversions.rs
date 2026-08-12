//! The owner's read of a site's conversion funnel (ADR 0036, S2.10a): the
//! authenticated `GET /sites/{id}/conversions` route.
//!
//! A separate module from [`crate::sites`]' traffic report and from
//! [`crate::sites_heatmap`] for a separate reason to change: traffic ranks
//! pages over a period, the heatmap draws one page, and this answers one
//! number per stage per conversion point.
//!
//! What it answers is deliberately narrow, because the collection doors are
//! (`alo_store::site_public_conversions`): three independent counters per
//! source. There is no per-visitor funnel to report, because no journey was
//! ever stored — the ratio between the stages is a property of the totals and
//! of nobody in particular.
//!
//! Error contract, identical to the rest of the `/sites/{id}` surface: `401`
//! unauthenticated, `422` for a period outside its bounds, and `404` for a
//! site that does not resolve in the caller's tenant — another tenant's site
//! is indistinguishable from one that never existed.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};

use alo_store::{SiteConversionReport, SiteId};

use crate::error::Problem;
use crate::sites::map_store_err;
use crate::state::{AppState, authenticate};

#[derive(Deserialize)]
pub struct ConversionsQuery {
    days: Option<u16>,
}

/// `GET /sites/:id/conversions?days=30` -> every conversion point of the site
/// with its view, start, and submit totals over the period, plus the site-wide
/// totals.
///
/// Conversion points with nothing on them are listed too: "no one has reached
/// this form yet" is a finding, and hiding it would leave an owner wondering
/// whether the form or the counting is broken.
pub async fn get_conversions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ConversionsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let days = query.days.unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "conversion period must be between 1 and 365 days",
        ));
    }

    let to = OffsetDateTime::now_utc().date();
    let from = to - Duration::days(i64::from(days - 1));
    let report = account
        .acc
        .site_conversions(&SiteId::new(id), from, to)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;

    Ok(Json(report_json(from, to, &report)))
}

/// The funnel as JSON. Stage counts are named rather than nested, because they
/// were counted independently: a `starts` above `views` is possible (a visitor
/// whose view report never arrived) and the shape must be able to say so
/// instead of implying a subset.
fn report_json(from: time::Date, to: time::Date, report: &SiteConversionReport) -> Value {
    json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "totals": {
            "views": report.views,
            "starts": report.starts,
            "submits": report.submits,
        },
        "sources": report.sources.iter().map(|source| json!({
            "kind": source.kind,
            "id": source.id,
            "name": source.name,
            "views": source.views,
            "starts": source.starts,
            "submits": source.submits,
        })).collect::<Vec<_>>(),
    })
}
