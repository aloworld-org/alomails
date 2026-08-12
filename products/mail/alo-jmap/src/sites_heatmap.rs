//! The owner's read of a page heatmap (ADR 0036, S2.09a): the authenticated
//! `GET /sites/{id}/heatmap` route.
//!
//! A separate module from [`crate::sites`] and from the traffic report in its
//! `get_analytics` for a separate reason to change: every other analytics
//! number is a ranking over a period, while this one is a grid over a single
//! page, and it carries the grid's own dimensions so nothing downstream has to
//! assume them.
//!
//! What the route answers is deliberately narrow, because the collection door
//! is (`alo_store::site_public_heatmap`): counts per grid cell per class of
//! screen, and a depth curve in tenths. There is no visitor, no session and no
//! time of day to report, because none was ever stored.
//!
//! Error contract, identical to the rest of the `/sites/{id}` surface: `401`
//! unauthenticated, `422` for a period or path outside its bounds, and `404`
//! for a site that does not resolve in the caller's tenant — another tenant's
//! site is indistinguishable from one that never existed.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};

use alo_store::{SiteHeatmapReport, SiteId};

use crate::error::Problem;
use crate::sites::map_store_err;
use crate::state::{AppState, authenticate};

/// The longest page path the route will look up, matching what collection
/// accepts. A longer one cannot have been stored, so it is a rule violation
/// rather than an empty grid.
const PATH_MAX_LEN: usize = 2048;

#[derive(Deserialize)]
pub struct HeatmapQuery {
    days: Option<u16>,
    path: Option<String>,
}

/// `GET /sites/:id/heatmap?days=30[&path=/prices]` -> the pages this site has
/// heatmap data for and, when a path is named, that page's grid.
///
/// The path list is always answered so the interface can offer a menu rather
/// than ask the owner to remember a URL; without `path` the `page` member is
/// `null`, which is not the same as a page with an empty grid.
pub async fn get_heatmap(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HeatmapQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let days = query.days.unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "heatmap period must be between 1 and 365 days",
        ));
    }
    let path = match query.path.as_deref().map(str::trim) {
        None => None,
        Some(path) if path.starts_with('/') && path.len() <= PATH_MAX_LEN => Some(path.to_owned()),
        Some(_) => {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "heatmap path must be an absolute page path of at most 2048 bytes",
            ));
        }
    };

    let to = OffsetDateTime::now_utc().date();
    let from = to - Duration::days(i64::from(days - 1));
    let site = SiteId::new(id);
    let paths = account
        .acc
        .site_heatmap_paths(&site, from, to)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such site"))?;

    let page = match path {
        None => None,
        Some(path) => account
            .acc
            .site_heatmap(&site, &path, from, to)
            .await
            .map_err(map_store_err)?
            .map(|report| report_json(&report)),
    };

    Ok(Json(json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "paths": paths.into_iter().map(|row| json!({
            "path": row.path,
            "events": row.events,
        })).collect::<Vec<_>>(),
        "page": page,
    })))
}

/// One page's grid. Cells are sparse — a cell nobody clicked is absent rather
/// than zero — while the depth curve keeps all ten tenths, because a curve
/// with its quiet tenths dropped is a different claim about the same page.
fn report_json(report: &SiteHeatmapReport) -> Value {
    json!({
        "path": report.path,
        "grid": { "columns": report.columns, "rows": report.rows },
        "viewports": report.viewports.iter().map(|viewport| json!({
            "viewport": viewport.viewport,
            "clickTotal": viewport.click_total,
            "clicks": viewport.clicks.iter().map(|cell| json!({
                "column": cell.column,
                "row": cell.row,
                "hits": cell.hits,
            })).collect::<Vec<_>>(),
            "scrollTotal": viewport.scroll_total,
            "scrollDepth": viewport.scroll_depth.iter().map(|bucket| json!({
                "bucket": bucket.bucket,
                "hits": bucket.hits,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
