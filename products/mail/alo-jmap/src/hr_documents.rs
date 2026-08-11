//! The papers on a person's file (alo HR, ADR 0035, wave B6.02b) — over
//! [`alo_store::hr_documents`].
//!
//! Three decisions this file makes rather than the store.
//!
//! - **Filing uploads into the HR area in the same act.** The body carries an
//!   already-uploaded blob (`POST /jmap/upload/{accountId}`, as every other
//!   upload in the product does), and this route creates the Drive node **in
//!   the tenant's HR area** and files it. A two-step "create the node, then
//!   file it" would have needed `/drive` to accept the HR area as a location a
//!   client can name — which would mean a colleague's client could name it too,
//!   and the only thing standing between them and somebody's contract would be
//!   the store's gate rather than the store's gate *and* the absence of a door.
//! - **The employee is checked before the node is created.** A filing against
//!   an id from another tenant must not leave an orphan file in our HR area, so
//!   the `404` is answered before anything is written.
//! - **A detach removes the filing, not the file.** The paper stays in the HR
//!   area, where it can be filed against the person it actually belongs to.
//!   Deleting somebody's contract because a filing was a mistake answers a
//!   different request than the one that was made; Drive's own trash is where a
//!   file goes.
//!
//! **There is no download route here**, deliberately: a filed paper is fetched
//! with `GET /drive/nodes/{nodeId}/download`, which refuses everybody without
//! the HR role because of where the node lives. A second download path would be
//! a second access rule to keep in step with the first.
//!
//! Everything on this surface is HR's door. Nothing here reads, parses or logs
//! the contents of a document, and the note a filer writes is about *which
//! paper it is*, never about the person.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::hr_documents::HrDocumentKind;
use alo_store::{DriveLocation, HrDocument, HrDocumentId, HrEmployeeId, NewDriveFile};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One filing as JSON.
///
/// `fileName` and `size` come from the Drive node in the same read, so a list
/// of somebody's documents is one round trip; they are `null` when the file has
/// since been purged through Drive's trash, and the filing stays as the honest
/// record that a paper was once here.
fn document_json(d: &HrDocument) -> Value {
    json!({
        "id": d.id.as_str(),
        "employeeId": d.employee_id.as_str(),
        "nodeId": d.node_id.as_str(),
        "kind": d.kind.as_str(),
        "note": d.note,
        "fileName": d.file_name,
        "size": d.size,
        "contentType": d.content_type,
        "trashed": d.trashed,
        "filedBy": d.filed_by.as_str(),
        "filedAt": iso(d.filed_at),
    })
}

/// The body of a filing: an uploaded blob, and what kind of paper it is.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilingBody {
    /// The blob id from `POST /jmap/upload/{accountId}`.
    blob_id: String,
    /// The file's name, as it will read in the HR area.
    name: String,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    content_type: Option<String>,
    /// `contract` | `amendment` | `letter` | `certificate` | `other`.
    kind: String,
    /// The filer's word for *which* paper this is.
    #[serde(default)]
    note: Option<String>,
}

/// `GET /hr/employees/{id}/documents` → `{"documents":[…]}` — **HR only**:
/// what is on this person's file, newest first.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the employee is not this tenant's.
pub async fn list_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let filed = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_documents(&HrEmployeeId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "documents": filed.iter().map(document_json).collect::<Vec<_>>(),
    })))
}

/// `POST /hr/employees/{id}/documents` `{blobId, name, kind, size?,
/// contentType?, note?}` → `{"document":{…}}` — **HR only**: file a contract,
/// an amendment or a letter against a person.
///
/// The file is created **in the tenant's HR area**, whose read and write gate
/// is the HR role itself — so the paper cannot be opened through
/// `/drive/nodes/{id}/download` by a colleague who learns its id, and it never
/// appears in anybody's search.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the employee is not this tenant's;
/// `409` when the file is already filed against somebody; `422` on an unknown
/// kind or a note that is too long; `400` on a body without a blob.
pub async fn file_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: FilingBody = parse_body(&body)?;
    if req.blob_id.trim().is_empty() || req.name.trim().is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "blobId and name are required",
        ));
    }
    let kind = HrDocumentKind::parse(&req.kind).map_err(map_store_err)?;
    let employee = HrEmployeeId::new(id);
    let hr = state.store.for_tenant(account.tenant.clone());
    // The person is proved to be this tenant's **before** a file is written, so
    // an id from another tenant cannot leave an orphan in our HR area.
    if hr
        .hr_employee(&employee)
        .await
        .map_err(map_store_err)?
        .is_none()
    {
        return Err(Problem::with(StatusCode::NOT_FOUND, "no such employee"));
    }
    let node = account
        .acc
        .drive_create_file(
            &DriveLocation::Hr,
            None,
            &NewDriveFile {
                name: req.name.trim().to_owned(),
                blob_id: req.blob_id.trim().to_owned(),
                size: req.size.unwrap_or_default(),
                content_type: req.content_type,
                ..Default::default()
            },
        )
        .await
        .map_err(map_store_err)?;
    let filing = hr
        .file_hr_document(
            &employee,
            &node,
            kind,
            req.note.as_deref().unwrap_or_default(),
            &account.user,
        )
        .await
        .map_err(map_store_err)?;
    load(&state, &account.tenant, &employee, &filing).await
}

/// `DELETE /hr/employees/{id}/documents/{document_id}` → `{"detached":true}` —
/// **HR only**: a paper filed against the wrong person.
///
/// The file itself stays in the HR area (see the module docs).
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the employee or the filing is not
/// this tenant's.
pub async fn detach_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, document_id)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    state
        .store
        .for_tenant(account.tenant.clone())
        .detach_hr_document(&HrEmployeeId::new(id), &HrDocumentId::new(document_id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "detached": true })))
}

/// The filing as it now stands — what a write answers with, so a client never
/// has to ask again for what it just filed.
async fn load(
    state: &AppState,
    tenant: &alo_store::TenantId,
    employee: &HrEmployeeId,
    filing: &HrDocumentId,
) -> Result<Json<Value>, Problem> {
    let document = state
        .store
        .for_tenant(tenant.clone())
        .hr_document(employee, filing)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such document"))?;
    Ok(Json(json!({ "document": document_json(&document) })))
}
