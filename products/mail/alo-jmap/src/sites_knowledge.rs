//! The site assistant's **Public knowledge** collection on the wire
//! (ADR 0040 §1, item S3.02d): list, publish, and withdraw the documents the
//! visitor assistant may read. The store half ([`alo_store::site_knowledge`])
//! shipped with the reading list; this is the HTTP door the source-adding
//! screen talks to.
//!
//! **Every route here is the site owner's, not the site editor's.** A
//! restricted collaborator can edit pages they were invited to; they cannot
//! read the tenant's Drive — and publishing a Drive document to the assistant
//! makes it readable by anyone on the internet (that is the sentence the
//! screen shows). Letting an editor do it would hand them exactly the door
//! their role exists to close, so the guard is [`require_site_manager`], the
//! same one the domain-purchase money door uses. The listing is behind the
//! same guard: the collection names Drive documents an editor may not see.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use alo_store::{DriveNodeId, SiteId, SiteKnowledgeSource, SiteKnowledgeSourceId};

use crate::error::Problem;
use crate::sites::{map_store_err, require_site, require_site_manager};
use crate::state::{Account, AppState, authenticate};

/// The refusal a non-owner meets on every route here — one sentence, the same
/// on all three doors, naming what only the owner may do.
const OWNER_ONLY: &str = "Only this website's owner can change what its assistant reads.";

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// The site this surface is about, provided the caller administers it.
async fn require_knowledge_site(account: &Account, site: &SiteId) -> Result<(), Problem> {
    let site = require_site(account, site).await?;
    require_site_manager(account, &site)
        .map_err(|_| Problem::with(StatusCode::FORBIDDEN, OWNER_ONLY))
}

fn source_json(source: &SiteKnowledgeSource) -> Value {
    json!({
        "id": source.id.as_str(),
        "docNodeId": source.doc_node_id.as_str(),
        "title": source.title,
        "trashed": source.trashed,
        "addedBy": source.added_by,
        "addedAt": iso(source.added_at),
    })
}

/// `GET /sites/:id/chat-knowledge` → `{"sources":[...]}`, oldest first. A
/// trashed document stays listed (flagged) so the owner can see and remove
/// the binding; the grounding corpus already excludes it.
pub async fn list_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_knowledge_site(&account, &site).await?;
    let sources = account
        .acc
        .site_knowledge_sources(&site)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "sources": sources.iter().map(source_json).collect::<Vec<_>>() }),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AddKnowledgeBody {
    doc_node_id: String,
}

/// `POST /sites/:id/chat-knowledge` `{"docNodeId":…}` → publishes one of the
/// tenant's readable documents to the site's assistant and answers the stored
/// binding. The store rules on readability (an alo Doc, a PDF, an Office
/// file, or plain text), on duplicates, and on the collection's size — each
/// refusal is a `422` naming the rule.
pub async fn add_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: AddKnowledgeBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let site = SiteId::new(id);
    require_knowledge_site(&account, &site).await?;
    let source_id = account
        .acc
        .add_site_knowledge_source(&site, &DriveNodeId::new(req.doc_node_id))
        .await
        .map_err(map_store_err)?;
    // Answer the stored row (title included), not just the id: the screen
    // shows the new source without a second read.
    let sources = account
        .acc
        .site_knowledge_sources(&site)
        .await
        .map_err(map_store_err)?;
    let stored = sources
        .iter()
        .find(|source| source.id == source_id)
        .map(source_json)
        .ok_or_else(Problem::server_error)?;
    Ok(Json(stored))
}

/// `DELETE /sites/:id/chat-knowledge/:source` → withdraws the document from
/// the assistant. The document itself stays in Drive untouched.
pub async fn remove_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, source)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let site = SiteId::new(id);
    require_knowledge_site(&account, &site).await?;
    account
        .acc
        .remove_site_knowledge_source(&site, &SiteKnowledgeSourceId::new(source))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "status": "removed" })))
}
