//! CRM stages HTTP surface (alo CRM, ADR 0035, wave B2) — the columns of a
//! board, on top of [`alo_store::crm_stages`].
//!
//! Two of this module's routes exist because of one rule from the design note
//! (`docs/design/crm.md` § Seeding): **a board drag must not be able to rename a
//! column, and saving an edit form must not be able to reorder the board.** So
//! `PATCH` writes the name and the win/loss flags and cannot touch `position`,
//! and `POST …/move` writes the position and can touch nothing else.
//!
//! Columns are addressed at the top level (`/crm/stages/{id}`) once they exist,
//! and created under their board (`/crm/pipelines/{id}/stages`) — the same shape
//! `/billing/invoices/{id}/payments` uses, and for the same reason: the parent
//! is where a create needs its context, and after that the id is enough.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::crm_stages::{NewStage, Stage};
use alo_store::{CrmPipelineId, CrmStageId};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::crm_pipelines::archive_intent;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A stage as JSON. `position` is a fractional ordering, never a quantity —
/// it is the one non-integer number the CRM surface carries.
fn stage_json(s: &Stage) -> Value {
    json!({
        "id": s.id.as_str(),
        "pipelineId": s.pipeline_id.as_str(),
        "name": s.name,
        "position": s.position,
        "isWon": s.is_won,
        "isLost": s.is_lost,
        "closed": s.is_closed(),
        "archived": s.is_archived(),
        "archivedAt": s.archived_at.map(iso),
        "createdAt": iso(s.created_at),
        "updatedAt": iso(s.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto.
fn editable(s: &Stage) -> NewStage {
    NewStage {
        name: s.name.clone(),
        is_won: s.is_won,
        is_lost: s.is_lost,
    }
}

/// The writable fields of a stage, every one optional.
///
/// The same body serves `POST` (merged onto [`NewStage::default`] — an unnamed
/// open column the store then refuses for its blank name) and `PATCH` (merged
/// onto the stored record). `position` is **not** here: see the module note.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StageBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_won: Option<bool>,
    #[serde(default)]
    is_lost: Option<bool>,
}

impl StageBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewStage) -> NewStage {
        NewStage {
            name: self.name.unwrap_or(base.name),
            is_won: self.is_won.unwrap_or(base.is_won),
            is_lost: self.is_lost.unwrap_or(base.is_lost),
        }
    }
}

/// Loads one of the tenant's stages, or fails with the `404` an id from another
/// tenant gets.
async fn load(acc: &AccountStore, id: &CrmStageId) -> Result<Stage, Problem> {
    acc.crm_stage(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such stage"))
}

/// Query string of the list route.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `includeArchived=1` also returns archived columns, in among the others
    /// by position — an archived column keeps its place, because that is where
    /// the deals that closed in it sat.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /crm/pipelines/{id}/stages[?includeArchived=1]` → `{"stages":[…]}` —
/// one board's columns, left to right. A board that is not this tenant's is the
/// same `404` an id that never existed gets, never an empty list.
pub async fn list_stages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pipeline): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let stages = account
        .acc
        .crm_stages(
            &CrmPipelineId::new(pipeline),
            flag(q.include_archived.as_deref()),
        )
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "stages": stages.iter().map(stage_json).collect::<Vec<_>>(),
    })))
}

/// `POST /crm/pipelines/{id}/stages` `{name, isWon?, isLost?}` →
/// `{"stage":{…}}` — append a column to the right-hand end of the board.
///
/// A board may carry at most one winning and one losing column; a second is a
/// `422` naming which flag is already taken, and a column that claims both is a
/// `422` too. The board's meaning lives in these two flags and not in the
/// names, which is what makes renaming a column a rename.
pub async fn create_stage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pipeline): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: StageBody = parse_body(&body)?;
    let input = req.apply(NewStage::default());
    let id = account
        .acc
        .create_crm_stage(&CrmPipelineId::new(pipeline), &input)
        .await
        .map_err(map_store_err)?;
    let stage = load(&account.acc, &id).await?;
    Ok(Json(json!({ "stage": stage_json(&stage) })))
}

/// `GET /crm/stages/{id}` → `{"stage":{…}}`. Archived columns are readable by
/// id, so a deal that closed in one can still say where.
pub async fn get_stage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let stage = load(&account.acc, &CrmStageId::new(id)).await?;
    Ok(Json(json!({ "stage": stage_json(&stage) })))
}

/// `PATCH /crm/stages/{id}` `{name?, isWon?, isLost?}` → `{"stage":{…}}` —
/// merge the stated fields onto the stored record.
///
/// Re-flagging a column changes what *future* moves mean and never rewrites
/// history: a deal's outcome is snapshotted on the deal at the moment it
/// closed, so last year's win rate is not a function of this year's board.
pub async fn update_stage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: StageBody = parse_body(&body)?;
    let id = CrmStageId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored));
    account
        .acc
        .update_crm_stage(&id, &input)
        .await
        .map_err(map_store_err)?;
    let stage = load(&account.acc, &id).await?;
    Ok(Json(json!({ "stage": stage_json(&stage) })))
}

/// The body of the reorder route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveBody {
    /// Where on the board the column now sits. Fractional, so a column can be
    /// dropped between two others without renumbering the rest.
    #[serde(default)]
    position: Option<f64>,
}

/// `POST /crm/stages/{id}/move` `{position}` → `{"stage":{…}}` — the one
/// operation a board drag performs.
///
/// Its own route rather than a field on the `PATCH`, and the mirror of
/// `POST /crm/deals/{id}/stage`: the surface says what a request *does*, so an
/// edit form and a drag can never be confused for one another. An absent
/// `position` is a `422` — a move that does not say where is not a move.
pub async fn move_stage(
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
            "position is required to move a stage",
        )
    })?;
    let id = CrmStageId::new(id);
    account
        .acc
        .move_crm_stage(&id, position)
        .await
        .map_err(map_store_err)?;
    let stage = load(&account.acc, &id).await?;
    Ok(Json(json!({ "stage": stage_json(&stage) })))
}

/// `POST /crm/stages/{id}/archive` `{"archived":true}` → `{"stage":{…}}` —
/// stop new cards landing in a column, or let them again.
///
/// Refused with `409` while the column still holds **open** deals: hiding a
/// column that work is standing in would hide the work with it. Closed deals do
/// not block — archiving says "no new work lands here", not "this never
/// happened". Idempotent otherwise.
pub async fn archive_stage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let archived = archive_intent(&body)?;
    let id = CrmStageId::new(id);
    account
        .acc
        .set_crm_stage_archived(&id, archived)
        .await
        .map_err(map_store_err)?;
    let stage = load(&account.acc, &id).await?;
    Ok(Json(json!({ "stage": stage_json(&stage) })))
}

/// `DELETE /crm/stages/{id}` → `{"deleted":true}` — the escape hatch for a
/// column created by mistake.
///
/// Refused with `409` for a column any deal stands in or any history row has
/// ever named (archive it instead — the past named it), and for a board's last
/// remaining column. Every other retirement is an archive.
pub async fn delete_stage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_crm_stage(&CrmStageId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> StageBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewStage {
        NewStage {
            name: "Qualified".to_owned(),
            is_won: false,
            is_lost: false,
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        assert_eq!(merged.name, "Qualified");
        assert!(!merged.is_won && !merged.is_lost);
    }

    #[test]
    fn a_rename_leaves_the_flags_alone() {
        let merged = body(json!({ "name": "Gekwalificeerd" })).apply(NewStage {
            name: "Won".to_owned(),
            is_won: true,
            is_lost: false,
        });
        assert_eq!(merged.name, "Gekwalificeerd");
        assert!(merged.is_won, "a rename is a rename, not a demotion");
    }

    #[test]
    fn false_is_a_stated_flag_not_an_absent_one() {
        let merged = body(json!({ "isWon": false })).apply(NewStage {
            name: "Won".to_owned(),
            is_won: true,
            is_lost: false,
        });
        assert!(!merged.is_won, "clearing a flag must reach the store");
    }

    #[test]
    fn a_patch_cannot_reorder_the_board() {
        // `position` is not a writable field; like any unknown field it is
        // ignored, so saving an edit form can never move a column.
        let merged = body(json!({ "position": 9.5, "name": "Qualified" })).apply(stored());
        assert_eq!(merged.name, "Qualified");
    }

    #[test]
    fn a_move_states_a_position_or_it_is_not_a_move() {
        let stated: MoveBody =
            serde_json::from_value(json!({ "position": 1.5 })).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(stated.position, Some(1.5));
        let absent: MoveBody = serde_json::from_value(json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(absent.position, None);
        // A position is an ordering, so a decimal is right here — unlike money,
        // which is only ever an integer number of cents.
        assert!(serde_json::from_value::<MoveBody>(json!({"position": "1.5"})).is_err());
    }
}
