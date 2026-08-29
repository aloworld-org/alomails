//! The alo Insights HTTP surface (ADR 0037, wave BI1.04) — the boards a tenant
//! reads its numbers from and the questions pinned to them, on top of
//! [`alo_store::insight_dashboards`] and [`alo_store::insight_tiles`].
//!
//! Billing's and CRM's conventions verbatim ([`crate::billing`]): authenticated
//! and tenant-scoped through the account door, no validation duplicated from
//! the store, every write answered with the stored record, `PATCH` as a merge
//! onto it. Evaluating a spec — the part that reads the tenant's documents — is
//! [`crate::insights_eval`]; nothing here computes a figure.
//!
//! Three rules give this module its shape.
//!
//! - **Moving a tile is its own `POST`.** The design note's route table sketched
//!   the move as a field on the tile `PATCH`; the surface CRM settled on is the
//!   one shipped (`docs/design/crm.md` § the stage routes): a board drag must
//!   not be able to retitle a chart, and saving an edit form must not be able to
//!   rearrange the board. `PATCH` writes title, spec and span and cannot touch
//!   `position`; `POST …/move` writes `position` and can touch nothing else.
//! - **A tile from the future is shown, not hidden.** A stored spec this build
//!   cannot parse comes back with `readable: false`, its raw envelope and the
//!   reason — the whole board still renders (`docs/design/insights.md`
//!   § Errors). It is only when such a tile is *evaluated* or *edited without a
//!   replacement spec* that the request is refused, because neither can be done
//!   honestly with a question we cannot read.
//! - **Listing boards is the one route that writes.** A tenant's first read is
//!   handed the zero-setup Business overview (BI1.06) — one working board of
//!   prebuilt questions, in the language of `?lang=`, written in a single
//!   transaction. It is a first-use rule and not an every-read one: the seed
//!   asks whether it has ever run for this tenant, so a board somebody threw
//!   away is not handed back the next morning.
//!
//! Insights is deliberately **not** in the business audit trail
//! ([`crate::audit_action`]): a dashboard is a view of records, never a record
//! of anything, and a paper trail of who rearranged a chart is noise in a log
//! whose value is that everything in it matters.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::insight_dashboards::{Dashboard, NewDashboard};
use alo_store::insight_tiles::{NewTile, Tile, TileSpec};
use alo_store::{AccountStore, InsightDashboardId, InsightTileId};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::insights_gallery::overview_seed_for;
use crate::state::{AppState, authenticate};

/// A dashboard as JSON. Its tiles are not inlined in the list: the tab strip is
/// one small response, and the board that is open is the one that pays for its
/// tiles (`GET /insights/dashboards/{id}`). `pub(crate)` because the Insights
/// agent's board read ([`crate::insights_intents`]) reports a board in exactly
/// this shape, so the two views cannot drift.
pub(crate) fn dashboard_json(d: &Dashboard) -> Value {
    json!({
        "id": d.id.as_str(),
        "name": d.name,
        "systemKey": d.system_key,
        "seeded": d.is_seeded(),
        "createdBy": d.created_by,
        "createdAt": iso(d.created_at),
        "updatedAt": iso(d.updated_at),
    })
}

/// A tile as JSON.
///
/// `spec` is always the stored envelope — canonical when this build can read
/// it, the raw JSON untouched when it cannot — and `readable` says which. An
/// unreadable tile additionally carries `specError`, so a client can say *why*
/// a placeholder is standing where a chart should be instead of showing a blank
/// card with no explanation.
pub(crate) fn tile_json(t: &Tile) -> Value {
    let (spec, readable, error) = match &t.spec {
        TileSpec::Readable(spec) => (spec.to_value().ok(), true, None),
        TileSpec::Unreadable { raw, reason } => (Some(raw.clone()), false, Some(reason.clone())),
    };
    json!({
        "id": t.id.as_str(),
        "dashboardId": t.dashboard_id.as_str(),
        "title": t.title,
        "spec": spec,
        "readable": readable,
        "specError": error,
        "viz": t.viz,
        "position": t.position,
        "span": t.span,
        "createdAt": iso(t.created_at),
        "updatedAt": iso(t.updated_at),
    })
}

/// Loads one of the tenant's dashboards, or fails with the `404` an id from
/// another tenant gets — the same answer as an id that never existed.
async fn load_dashboard(acc: &AccountStore, id: &InsightDashboardId) -> Result<Dashboard, Problem> {
    acc.insight_dashboard(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such dashboard"))
}

/// Loads one of the tenant's tiles, with the same denial.
pub(crate) async fn load_tile(acc: &AccountStore, id: &InsightTileId) -> Result<Tile, Problem> {
    acc.insight_tile(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such tile"))
}

/// The writable field of a dashboard. Unknown fields are ignored so the
/// contract can grow additively; `systemKey` is deliberately among them,
/// because a seed marker is ours and never a caller's.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DashboardBody {
    #[serde(default)]
    name: Option<String>,
}

impl DashboardBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewDashboard) -> NewDashboard {
        NewDashboard {
            name: self.name.unwrap_or(base.name),
        }
    }
}

/// Query string of the list route: the seed language.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `lang=fr` names the seeded overview and its tiles in French. Only ever
    /// read on a tenant's **first** read of the module; after that the captions
    /// are stored user data and this parameter does nothing.
    #[serde(default)]
    lang: Option<String>,
}

/// `GET /insights/dashboards[?lang=fr]` → `{"dashboards":[…]}` — the tenant's
/// boards, oldest first, so the seeded overview stays the first tab.
///
/// **This is the route that seeds.** A tenant that has never opened Insights is
/// given the Business overview — live numbers with no builder and no setup form
/// — and two colleagues opening the module in the same instant still get
/// exactly one board.
pub async fn list_dashboards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let seed = overview_seed_for(q.lang.as_deref().unwrap_or_default());
    let dashboards = account
        .acc
        .insight_dashboards_or_seed(&seed)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "dashboards": dashboards.iter().map(dashboard_json).collect::<Vec<_>>(),
    })))
}

/// `POST /insights/dashboards` `{name}` → `{"dashboard":{…}}` — a new board,
/// with no tiles on it yet.
pub async fn create_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DashboardBody = parse_body(&body)?;
    let input = req.apply(NewDashboard::default());
    let id = account
        .acc
        .create_insight_dashboard(&input)
        .await
        .map_err(map_store_err)?;
    let dashboard = load_dashboard(&account.acc, &id).await?;
    Ok(Json(json!({ "dashboard": dashboard_json(&dashboard) })))
}

/// `GET /insights/dashboards/{id}` → `{"dashboard":{…},"tiles":[…]}` — the
/// board and its tiles in layout order.
///
/// One read rather than two: opening a board and drawing its grid is a single
/// intention, and the tiles carry no figures, so this stays a small response
/// however heavy the charts on it turn out to be. Each tile's numbers are
/// fetched on its own (`GET /insights/tiles/{id}/data`), which is what lets a
/// grid render immediately and fill in as answers arrive.
pub async fn get_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let id = InsightDashboardId::new(id);
    let dashboard = load_dashboard(&account.acc, &id).await?;
    let tiles = account
        .acc
        .insight_tiles(&id)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "dashboard": dashboard_json(&dashboard),
        "tiles": tiles.iter().map(tile_json).collect::<Vec<_>>(),
    })))
}

/// `PATCH /insights/dashboards/{id}` `{name?}` → `{"dashboard":{…}}` — rename a
/// board. A seeded board renames like any other: its seed marker is untouched,
/// so the overview is still never written twice.
pub async fn update_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: DashboardBody = parse_body(&body)?;
    let id = InsightDashboardId::new(id);
    let stored = load_dashboard(&account.acc, &id).await?;
    let input = req.apply(NewDashboard {
        name: stored.name.clone(),
    });
    account
        .acc
        .rename_insight_dashboard(&id, &input)
        .await
        .map_err(map_store_err)?;
    let dashboard = load_dashboard(&account.acc, &id).await?;
    Ok(Json(json!({ "dashboard": dashboard_json(&dashboard) })))
}

/// `DELETE /insights/dashboards/{id}` → `{"deleted":true}` — remove a board and
/// its tiles.
///
/// A real delete, unlike a billing document or a CRM board, which are archived:
/// a dashboard is a *view* of records and never a record of anything, so
/// nothing is lost that the invoices and deals underneath it do not still hold.
/// Deleting the seeded overview is allowed and it does not come back — the seed
/// asks whether this tenant ever had it, not whether it has it now.
pub async fn delete_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_insight_dashboard(&InsightDashboardId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

/// The writable fields of a tile. The same body serves the pin (merged onto
/// [`NewTile::default`], whose empty spec the store then refuses) and the edit
/// (merged onto the stored record).
///
/// `span` is read as an `i64` and narrowed rather than deserialized as an
/// `i16`: a caller asking for 40 columns has broken the *grid* rule, and should
/// be told so with the `422` that names it, not handed a `400` for a malformed
/// body because the number happened not to fit the column's width.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TileBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    spec: Option<Value>,
    #[serde(default)]
    span: Option<i64>,
}

impl TileBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewTile) -> NewTile {
        NewTile {
            title: self.title.unwrap_or(base.title),
            spec: self.spec.unwrap_or(base.spec),
            span: self
                .span
                .map_or(base.span, |span| i16::try_from(span).unwrap_or(i16::MAX)),
        }
    }
}

/// The stored tile as writable input — the base an edit merges onto.
///
/// A tile whose stored spec this build cannot read has no base to merge a
/// partial edit onto: re-writing the raw envelope would fail the write gate
/// with a message about a schema this caller never sent. So the refusal is
/// made here, and it names the way out — send a spec, and the tile is rewritten
/// as one this build understands.
fn editable(tile: &Tile, body_has_spec: bool) -> Result<NewTile, Problem> {
    let spec = match &tile.spec {
        TileSpec::Readable(spec) => spec.to_value().map_err(|_| Problem::server_error())?,
        TileSpec::Unreadable { raw, .. } if body_has_spec => raw.clone(),
        TileSpec::Unreadable { .. } => {
            return Err(Problem::with(
                StatusCode::UNPROCESSABLE_ENTITY,
                "this tile's stored chart spec cannot be read by this version; \
                 send a spec to replace it",
            ));
        }
    };
    Ok(NewTile {
        title: tile.title.clone(),
        spec,
        span: tile.span,
    })
}

/// `POST /insights/dashboards/{id}/tiles` `{title, spec, span?}` →
/// `{"tile":{…}}` — pin a question to a board, at the end of the layout.
///
/// The spec goes through the typed model before anything is stored, so an
/// invented measure or an incompatible pairing is a `422` naming the field
/// rather than a tile that draws nothing. A board that is not this tenant's is
/// the same `404` an id that never existed gets — the composite foreign key is
/// what refuses it, so the denial is structural rather than a check somebody
/// can forget.
pub async fn create_tile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(dashboard): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: TileBody = parse_body(&body)?;
    let input = req.apply(NewTile::default());
    let id = account
        .acc
        .create_insight_tile(&InsightDashboardId::new(dashboard), &input)
        .await
        .map_err(map_store_err)?;
    let tile = load_tile(&account.acc, &id).await?;
    Ok(Json(json!({ "tile": tile_json(&tile) })))
}

/// `PATCH /insights/tiles/{id}` `{title?, spec?, span?}` → `{"tile":{…}}` —
/// merge the stated fields onto the stored tile.
///
/// Its board and its place on that board are not writable here: a chart that
/// jumped across the layout because somebody fixed a typo in its caption would
/// be a surprise nobody asked for. Replacing the spec re-runs the whole write
/// gate.
pub async fn update_tile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: TileBody = parse_body(&body)?;
    let id = InsightTileId::new(id);
    let stored = load_tile(&account.acc, &id).await?;
    let input = req.apply(editable(&stored, req_has_spec(&body)?)?);
    account
        .acc
        .update_insight_tile(&id, &input)
        .await
        .map_err(map_store_err)?;
    let tile = load_tile(&account.acc, &id).await?;
    Ok(Json(json!({ "tile": tile_json(&tile) })))
}

/// Whether the request body states a `spec` at all — the question
/// [`editable`] needs answered before the merge, and one a merged value can no
/// longer answer.
fn req_has_spec(body: &axum::body::Bytes) -> Result<bool, Problem> {
    let raw: Value = parse_body(body)?;
    Ok(raw.get("spec").is_some())
}

/// The body of the reorder route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveBody {
    /// Where on the board the tile now sits. Fractional, so a tile can be
    /// dropped between two others without renumbering the rest.
    #[serde(default)]
    position: Option<f64>,
}

/// `POST /insights/tiles/{id}/move` `{position}` → `{"tile":{…}}` — the one
/// operation a grid drag performs.
///
/// Its own route rather than a field on the `PATCH`, for the reason the CRM
/// board settled: the surface says what a request *does*. An absent `position`
/// is a `422` — a move that does not say where is not a move.
pub async fn move_tile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: MoveBody = parse_body(&body)?;
    let position = req.position.ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "position is required to move a tile",
        )
    })?;
    let id = InsightTileId::new(id);
    account
        .acc
        .move_insight_tile(&id, position)
        .await
        .map_err(map_store_err)?;
    let tile = load_tile(&account.acc, &id).await?;
    Ok(Json(json!({ "tile": tile_json(&tile) })))
}

/// `DELETE /insights/tiles/{id}` → `{"deleted":true}` — unpin a tile. A tile is
/// a question, so nothing is lost that the documents underneath do not hold.
pub async fn delete_tile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_insight_tile(&InsightTileId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alo_store::insight_tiles::TILE_SPAN_MAX;

    fn tile_body(json: Value) -> TileBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn dashboard_body(json: Value) -> DashboardBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    /// The stored tile a partial edit merges onto.
    fn stored() -> NewTile {
        NewTile {
            title: "Outstanding".to_owned(),
            spec: json!({
                "schema_version": 1,
                "dataset": "billing.receivables",
                "measure": { "id": "outstanding", "agg": "sum" },
                "period": { "kind": "all" },
                "viz": "number"
            }),
            span: 2,
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = tile_body(json!({})).apply(stored());
        assert_eq!(merged.title, "Outstanding");
        assert_eq!(merged.span, 2);
        assert_eq!(merged.spec, stored().spec);
    }

    #[test]
    fn a_retitle_leaves_the_question_alone() {
        let merged = tile_body(json!({ "title": "Owed to us" })).apply(stored());
        assert_eq!(merged.title, "Owed to us");
        assert_eq!(merged.spec, stored().spec, "the spec is untouched");
        assert_eq!(merged.span, 2);
    }

    #[test]
    fn a_patch_cannot_move_a_tile() {
        // `position` is not a writable field here; like any unknown field it is
        // ignored, so saving an edit form can never rearrange the board.
        let merged = tile_body(json!({ "position": 99.0, "title": "Owed" })).apply(stored());
        assert_eq!(merged.title, "Owed");
        assert_eq!(merged.span, 2);
    }

    #[test]
    fn an_impossible_span_stays_a_grid_rule_and_never_a_malformed_body() {
        // Both of these are refused by the store's span rule with a `422`. What
        // matters here is that a number far outside the grid still *arrives*
        // as a span rather than failing the body parse with a `400`.
        for absurd in [i64::from(TILE_SPAN_MAX) + 1, 40, i64::from(i32::MAX)] {
            let merged = tile_body(json!({ "span": absurd })).apply(stored());
            assert!(
                !(1..=TILE_SPAN_MAX).contains(&merged.span),
                "a span outside the grid must stay outside it: {absurd}"
            );
        }
        let merged = tile_body(json!({ "span": 4 })).apply(stored());
        assert_eq!(merged.span, 4);
    }

    #[test]
    fn a_dashboard_patch_merges_onto_the_stored_name() {
        let base = NewDashboard {
            name: "Cash".to_owned(),
        };
        assert_eq!(dashboard_body(json!({})).apply(base.clone()).name, "Cash");
        assert_eq!(
            dashboard_body(json!({ "name": "Cash 2027" }))
                .apply(base)
                .name,
            "Cash 2027"
        );
    }

    #[test]
    fn a_seed_marker_is_never_a_callers_field() {
        // `systemKey` is ignored like any unknown field: a client cannot mint a
        // board that claims to be the one we seed.
        let merged = dashboard_body(json!({ "systemKey": "business_overview", "name": "Mine" }))
            .apply(NewDashboard::default());
        assert_eq!(merged.name, "Mine");
    }

    #[test]
    fn a_body_that_states_a_spec_is_told_apart_from_one_that_does_not() {
        let with = axum::body::Bytes::from_static(br#"{"title":"x","spec":null}"#);
        assert_eq!(req_has_spec(&with).ok(), Some(true), "null is still stated");
        let without = axum::body::Bytes::from_static(br#"{"title":"x"}"#);
        assert_eq!(req_has_spec(&without).ok(), Some(false));
    }

    #[test]
    fn a_tile_from_the_future_can_only_be_edited_by_replacing_its_question() {
        let unreadable = Tile {
            id: InsightTileId::new("t1".to_owned()),
            dashboard_id: InsightDashboardId::new("d1".to_owned()),
            title: "Later".to_owned(),
            spec: TileSpec::Unreadable {
                raw: json!({ "schema_version": 2 }),
                reason: "unsupported chart schema_version 2".to_owned(),
            },
            viz: None,
            position: 1.0,
            span: 1,
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
        };
        let refused = editable(&unreadable, false).err().unwrap_or_else(|| {
            panic!("a partial edit of an unreadable tile must be refused");
        });
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            refused.detail.unwrap_or_default().contains("send a spec"),
            "the refusal names the way out"
        );
        // With a replacement spec in the body there is a base to merge onto,
        // and the store's write gate decides on the new spec alone.
        assert!(editable(&unreadable, true).is_ok());

        // And the read of such a tile is never an error: the board renders.
        let json = tile_json(&unreadable);
        assert_eq!(json["readable"], json!(false));
        assert_eq!(json["spec"], json!({ "schema_version": 2 }));
        assert!(json["specError"].is_string());
        assert_eq!(json["viz"], Value::Null);
    }
}
