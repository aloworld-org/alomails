//! CRM pipelines HTTP surface (alo CRM, ADR 0035, wave B2) — the boards a
//! tenant's deals move across, on top of [`alo_store::crm_pipelines`].
//!
//! Billing's conventions verbatim ([`crate::billing`]): authenticated and
//! tenant-scoped through the account door, no validation duplicated from the
//! store, every write answered with the stored record, `PATCH` as a merge onto
//! it, and archiving as its own `POST` so an ordinary rename can never make a
//! board disappear from the tabs.
//!
//! One rule is this module's own. **The list route seeds a tenant's first
//! board**: a new tenant opens CRM onto a working funnel rather than a setup
//! form (`docs/design/crm.md` § Seeding). The words come from `?lang=` at this
//! edge ([`crate::crm::seed_for`]) and are ordinary user data from the moment
//! they are written — the store never invents a name.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::AccountStore;
use alo_store::CrmPipelineId;
use alo_store::crm_pipelines::{NewPipeline, Pipeline};

use crate::billing::{flag, iso, map_store_err, parse_body};
use crate::crm::seed_for;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// A pipeline as JSON. Its columns are not inlined: a board's stages are their
/// own read (`GET /crm/pipelines/{id}/stages`), so the tab strip is one small
/// response and the board is the one that pays for the columns.
fn pipeline_json(p: &Pipeline) -> Value {
    json!({
        "id": p.id.as_str(),
        "name": p.name,
        "description": p.description,
        "archived": p.is_archived(),
        "archivedAt": p.archived_at.map(iso),
        "createdBy": p.created_by,
        "createdAt": iso(p.created_at),
        "updatedAt": iso(p.updated_at),
    })
}

/// The stored record as writable input — the base a `PATCH` merges onto.
fn editable(p: &Pipeline) -> NewPipeline {
    NewPipeline {
        name: p.name.clone(),
        description: p.description.clone(),
    }
}

/// The writable fields of a pipeline, both optional.
///
/// The same body serves `POST` (merged onto [`NewPipeline::default`] — a
/// nameless, undescribed board the store then refuses) and `PATCH` (merged onto
/// the stored record). Unknown fields are ignored so the contract can grow
/// additively; `archived` is deliberately among them, because archiving is its
/// own route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl PipelineBody {
    /// Merges the stated fields onto `base`, leaving the rest as they were.
    fn apply(self, base: NewPipeline) -> NewPipeline {
        NewPipeline {
            name: self.name.unwrap_or(base.name),
            description: self.description.unwrap_or(base.description),
        }
    }
}

/// Loads one of the tenant's pipelines, or fails with the `404` an id from
/// another tenant gets.
async fn load(acc: &AccountStore, id: &CrmPipelineId) -> Result<Pipeline, Problem> {
    acc.crm_pipeline(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such pipeline"))
}

/// Query string of the list route: the seed language, plus the archive switch.
#[derive(Deserialize)]
pub struct ListQuery {
    /// `lang=fr` names the seeded board and its columns in French. Only ever
    /// read on a tenant's **first** read of the module; after that the names
    /// are stored user data and this parameter does nothing.
    #[serde(default)]
    lang: Option<String>,
    /// `includeArchived=1` also returns archived boards, sorted after the
    /// active ones. Read through [`flag`], so an unparseable value is simply
    /// off rather than a rejected request.
    #[serde(default, rename = "includeArchived")]
    include_archived: Option<String>,
}

/// `GET /crm/pipelines[?lang=fr][&includeArchived=1]` → `{"pipelines":[…]}` —
/// the tenant's boards in name order, active ones first.
///
/// **This is the route that seeds.** A tenant with no board at all is given one
/// ("Sales", five columns) in the caller's language, in a single transaction;
/// two colleagues opening the module in the same instant still get exactly one
/// board. Seeding is a first-use rule, not an every-read one — a tenant that
/// archived its only board is not handed a new one the next morning, which is
/// why `includeArchived=1` re-reads rather than short-circuiting the seed: it
/// asks a different question, not a wider one.
pub async fn list_pipelines(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let seed = seed_for(q.lang.as_deref().unwrap_or_default());
    let active = account
        .acc
        .crm_pipelines_or_seed(&seed)
        .await
        .map_err(map_store_err)?;
    let pipelines = if flag(q.include_archived.as_deref()) {
        account
            .acc
            .crm_pipelines(true)
            .await
            .map_err(map_store_err)?
    } else {
        active
    };
    Ok(Json(json!({
        "pipelines": pipelines.iter().map(pipeline_json).collect::<Vec<_>>(),
    })))
}

/// `POST /crm/pipelines` `{name, description?}` → `{"pipeline":{…}}` — create a
/// second board (Renewals, one per team). It starts with **no columns**: a
/// board built by hand is built by hand, and only the first-use seed hands one
/// over ready-made.
pub async fn create_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PipelineBody = parse_body(&body)?;
    let input = req.apply(NewPipeline::default());
    let id = account
        .acc
        .create_crm_pipeline(&input)
        .await
        .map_err(map_store_err)?;
    let pipeline = load(&account.acc, &id).await?;
    Ok(Json(json!({ "pipeline": pipeline_json(&pipeline) })))
}

/// `GET /crm/pipelines/{id}` → `{"pipeline":{…}}`. Archived boards are readable
/// by id — a deal won last year must always be able to name where it was won.
pub async fn get_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let pipeline = load(&account.acc, &CrmPipelineId::new(id)).await?;
    Ok(Json(json!({ "pipeline": pipeline_json(&pipeline) })))
}

/// `PATCH /crm/pipelines/{id}` `{name?, description?}` → `{"pipeline":{…}}` —
/// merge the stated fields onto the stored record. Renaming a board is a
/// rename: every deal on it, open or closed, keeps pointing at it.
pub async fn update_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: PipelineBody = parse_body(&body)?;
    let id = CrmPipelineId::new(id);
    let stored = load(&account.acc, &id).await?;
    let input = req.apply(editable(&stored));
    account
        .acc
        .update_crm_pipeline(&id, &input)
        .await
        .map_err(map_store_err)?;
    let pipeline = load(&account.acc, &id).await?;
    Ok(Json(json!({ "pipeline": pipeline_json(&pipeline) })))
}

/// The body of every archive route in this module: `false` restores.
#[derive(Deserialize)]
pub(crate) struct ArchiveBody {
    /// Required when a body is sent; an **empty** body archives, because the
    /// route's name is already the intent.
    pub(crate) archived: bool,
}

/// Reads an archive body, defaulting an empty one to "archive".
pub(crate) fn archive_intent(body: &axum::body::Bytes) -> Result<bool, Problem> {
    let req: ArchiveBody = parse_body(if body.is_empty() {
        br#"{"archived":true}"#
    } else {
        body
    })?;
    Ok(req.archived)
}

/// `POST /crm/pipelines/{id}/archive` `{"archived":true}` →
/// `{"pipeline":{…}}` — retire a board, or bring it back.
///
/// Never a delete: a closed deal must always be able to name the board it
/// closed on. Refused with `409` while the board still holds **open** deals —
/// a board that vanishes from the tabs with live work on it takes the work with
/// it — and refused with `409` when restoring would collide with the name of
/// another active board. Idempotent otherwise.
pub async fn archive_pipeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let archived = archive_intent(&body)?;
    let id = CrmPipelineId::new(id);
    account
        .acc
        .set_crm_pipeline_archived(&id, archived)
        .await
        .map_err(map_store_err)?;
    let pipeline = load(&account.acc, &id).await?;
    Ok(Json(json!({ "pipeline": pipeline_json(&pipeline) })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: Value) -> PipelineBody {
        serde_json::from_value(json).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored() -> NewPipeline {
        NewPipeline {
            name: "Renewals".to_owned(),
            description: "Contracts up for renewal".to_owned(),
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = body(json!({})).apply(stored());
        assert_eq!(merged.name, "Renewals");
        assert_eq!(merged.description, "Contracts up for renewal");
    }

    #[test]
    fn a_rename_leaves_the_description_alone() {
        let merged = body(json!({ "name": "Renewals 2027" })).apply(stored());
        assert_eq!(merged.name, "Renewals 2027");
        assert_eq!(merged.description, "Contracts up for renewal");
    }

    #[test]
    fn a_description_can_be_cleared_with_an_empty_string() {
        // There is no `null` case here: the column is NOT NULL and empty is
        // exactly what "no description" means in the store.
        let merged = body(json!({ "description": "" })).apply(stored());
        assert_eq!(merged.description, "");
        assert_eq!(merged.name, "Renewals");
    }

    #[test]
    fn a_patch_cannot_archive_a_board() {
        // `archived` is not a writable field; like any unknown field it is
        // ignored, so a stale edit form can never retire a board.
        let merged = body(json!({ "archived": true, "name": "Renewals" })).apply(stored());
        assert_eq!(merged.name, "Renewals");
    }

    #[test]
    fn an_empty_archive_body_means_archive_and_a_stated_one_wins() {
        assert_eq!(archive_intent(&axum::body::Bytes::new()).ok(), Some(true));
        let restore = axum::body::Bytes::from_static(br#"{"archived":false}"#);
        assert_eq!(archive_intent(&restore).ok(), Some(false));
        let malformed = axum::body::Bytes::from_static(br#"{"archived":"yes"}"#);
        assert!(archive_intent(&malformed).is_err());
    }
}
