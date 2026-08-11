//! Hiring, over HTTP (alo HR, ADR 0035, wave B6.06a) — over
//! [`alo_store::hr_openings`] and [`alo_store::hr_applicants`].
//!
//! One surface, because openings and the people who applied for them are one
//! screen and one door: everything here is `require_hr`, and there is no
//! employee-facing or manager-facing view of a candidate at all.
//!
//! Four decisions this file makes rather than the store.
//!
//! - **A CV arrives as an uploaded blob and becomes a node in the tenant's HR
//!   area in the same act** — the shape `POST /hr/employees/{id}/documents`
//!   already has, and for the same reason: a two-step "create the node, then
//!   attach it" would mean `/drive` had to accept the HR area as a location a
//!   client can name, and then a colleague's client could name it too.
//! - **Replacing or clearing a CV trashes the file it replaced.** This is
//!   deliberately *unlike* `DELETE …/documents/{id}`, whose whole point is that
//!   the paper survives to be filed against the right person. A CV belongs to
//!   exactly one candidate; leaving the old one in the HR area would leave a
//!   file that the retention deadline no longer points at, which is the one
//!   thing this module promises not to do.
//! - **Erasing a candidate is a route, not a job.** `DELETE
//!   /hr/applicants/{id}` removes the row, its notes and its CV. Nothing here
//!   runs on a timer: `retentionExpired` tells a person which records are past
//!   their date, and a person presses the button (`docs/design/hr.md`,
//!   "Applicants are different, and get a deadline").
//! - **The stage vocabulary is served with the pipeline**, so a board renders
//!   the columns this build actually accepts rather than a list a client
//!   hard-coded and we later changed.
//!
//! **Nothing here reads a CV.** No parse, no extract, no index, no score, no
//! rank, no shortlist — not even suggest-only, not even with a human after it
//! (`docs/design/hr.md`, "The EU AI Act posture"). The only thing this service
//! does with the file is store it and hand it back through Drive's own
//! HR-gated download path.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::hr_applicants::ApplicantStage;
use alo_store::hr_openings::{NewOpening, OpeningStatus};
use alo_store::{
    Applicant, ApplicantNote, ContractKind, DriveLocation, DriveNodeId, HrApplicantId, HrOpeningId,
    NewApplicant, NewDriveFile, Opening, TenantStore,
};

use crate::billing::{
    blank_to_none, flag, iso, iso_date, map_store_err, parse_body, parse_iso_date,
};
use crate::error::Problem;
use crate::state::{Account, AppState, authenticate};

/// One opening as JSON, with the size of its pipeline.
fn opening_json(o: &Opening) -> Value {
    json!({
        "id": o.id.as_str(),
        "title": o.title,
        "team": o.team,
        "location": o.location,
        "employmentKind": o.employment_kind.as_str(),
        "status": o.status.as_str(),
        "openedOn": o.opened_on.map(iso_date),
        "closedOn": o.closed_on.map(iso_date),
        "applicants": o.applicants,
        "createdBy": o.created_by.as_str(),
        "createdAt": iso(o.created_at),
        "updatedAt": iso(o.updated_at),
    })
}

/// One candidate as JSON.
///
/// `cvFileName` and `cvSize` come from the Drive node in the same read, so a
/// pipeline is one round trip; they are `null` when the file has since been
/// purged, and the row stays as the honest record that there was one.
fn applicant_json(a: &Applicant) -> Value {
    json!({
        "id": a.id.as_str(),
        "openingId": a.opening_id.as_str(),
        "name": a.name,
        "email": a.email,
        "phone": a.phone,
        "source": a.source,
        "stage": a.stage.as_str(),
        "cvNodeId": a.cv_node_id.as_ref().map(DriveNodeId::as_str),
        "cvFileName": a.cv_file_name,
        "cvSize": a.cv_size,
        "cvTrashed": a.cv_trashed,
        "retainUntil": iso_date(a.retain_until),
        "retentionExpired": a.retention_expired,
        "createdAt": iso(a.created_at),
        "updatedAt": iso(a.updated_at),
    })
}

/// One interview note as JSON, with the person who wrote it on it.
fn note_json(n: &ApplicantNote) -> Value {
    json!({
        "id": n.id.as_str(),
        "author": n.author.as_str(),
        "body": n.body,
        "createdAt": iso(n.created_at),
    })
}

/// Every stage this build accepts, in board order — served with a pipeline so a
/// client renders the columns that exist rather than a list it hard-coded.
fn stages_json() -> Vec<&'static str> {
    ApplicantStage::ALL
        .iter()
        .map(|stage| stage.as_str())
        .collect()
}

/// The writable fields of an opening. Absent fields keep what is stored, so a
/// `PATCH` carrying one field changes one field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpeningBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    employment_kind: Option<String>,
}

impl OpeningBody {
    /// Merges the stated fields onto `base`.
    fn apply(self, base: NewOpening) -> Result<NewOpening, Problem> {
        Ok(NewOpening {
            title: self.title.unwrap_or(base.title),
            team: self.team.unwrap_or(base.team),
            location: self.location.unwrap_or(base.location),
            employment_kind: match self.employment_kind.as_deref() {
                None => base.employment_kind,
                Some(word) => ContractKind::parse(word).map_err(map_store_err)?,
            },
        })
    }
}

/// The stored opening as writable input — the base a `PATCH` merges onto.
fn editable_opening(o: &Opening) -> NewOpening {
    NewOpening {
        title: o.title.clone(),
        team: o.team.clone(),
        location: o.location.clone(),
        employment_kind: o.employment_kind,
    }
}

/// An uploaded CV: the blob, and the name it takes in the HR area.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CvBody {
    /// The blob id from `POST /jmap/upload/{accountId}`.
    blob_id: String,
    /// The file's name, as it will read in the HR area.
    name: String,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    content_type: Option<String>,
}

/// The writable fields of an application. **`stage` is deliberately not one of
/// them**: it moves through `POST /hr/applicants/{id}/move` and nowhere else, so
/// a correction to a telephone number can never reorder somebody's candidacy.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplicantBody {
    #[serde(default)]
    name: Option<String>,
    /// `null` clears the address; absent leaves it alone.
    #[serde(default, deserialize_with = "crate::billing::absent_or_null")]
    email: Option<Option<String>>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    source: Option<String>,
    /// A new CV to upload, or `null` to take the one on file off. Absent leaves
    /// whatever is attached alone.
    #[serde(default, deserialize_with = "crate::billing::absent_or_null")]
    cv: Option<Option<CvBody>>,
    /// `YYYY-MM-DD`. Absent keeps the stored deadline — six months from the day
    /// the application was recorded, unless somebody has already changed it.
    #[serde(default)]
    retain_until: Option<String>,
}

impl ApplicantBody {
    /// Merges the stated fields onto `base`, leaving the CV to the caller (it
    /// is an upload, not a field).
    fn apply(&self, base: NewApplicant) -> Result<NewApplicant, Problem> {
        let retain_until = match self.retain_until.as_deref() {
            None => base.retain_until,
            Some(raw) => Some(parse_iso_date(raw).ok_or_else(|| {
                Problem::with(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "retainUntil must be a date written YYYY-MM-DD",
                )
            })?),
        };
        Ok(NewApplicant {
            name: self.name.clone().unwrap_or(base.name),
            email: match &self.email {
                None => base.email,
                Some(stated) => blank_to_none(stated.clone()),
            },
            phone: self.phone.clone().unwrap_or(base.phone),
            source: self.source.clone().unwrap_or(base.source),
            cv_node_id: base.cv_node_id,
            retain_until,
        })
    }
}

/// The stored candidate as writable input — the base a `PATCH` merges onto.
fn editable_applicant(a: &Applicant) -> NewApplicant {
    NewApplicant {
        name: a.name.clone(),
        email: a.email.clone(),
        phone: a.phone.clone(),
        source: a.source.clone(),
        cv_node_id: a.cv_node_id.clone(),
        retain_until: Some(a.retain_until),
    }
}

/// Loads one of the tenant's openings, or the `404` an id from another tenant
/// gets — the same answer an id that was never issued gets.
async fn load_opening(hr: &TenantStore, id: &HrOpeningId) -> Result<Opening, Problem> {
    hr.hr_opening(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such opening"))
}

/// Loads one of the tenant's candidates, or the `404`.
async fn load_applicant(hr: &TenantStore, id: &HrApplicantId) -> Result<Applicant, Problem> {
    hr.hr_applicant(id)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such applicant"))
}

/// Uploads a CV into the tenant's HR area and answers with the node it became.
///
/// The area's read gate is the HR role itself, so the file cannot be opened
/// through `/drive/nodes/{id}/download` by a colleague who learns its id, and it
/// never appears in anybody's search.
async fn upload_cv(account: &Account, cv: &CvBody) -> Result<DriveNodeId, Problem> {
    if cv.blob_id.trim().is_empty() || cv.name.trim().is_empty() {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a CV needs blobId and name",
        ));
    }
    account
        .acc
        .drive_create_file(
            &DriveLocation::Hr,
            None,
            &NewDriveFile {
                name: cv.name.trim().to_owned(),
                blob_id: cv.blob_id.trim().to_owned(),
                size: cv.size.unwrap_or_default(),
                content_type: cv.content_type.clone(),
                ..Default::default()
            },
        )
        .await
        .map_err(map_store_err)
}

/// Trashes a CV that is no longer attached to anybody.
///
/// Best-effort by design: the record is already correct, and a file that was
/// purged through Drive in the meantime must not turn a successful erasure into
/// a `500`. It goes to the HR area's trash rather than being purged, so an
/// accidental replacement is recoverable for as long as Drive keeps it.
async fn discard_cv(account: &Account, node: &DriveNodeId) {
    let _ = account.acc.drive_trash_node(node).await;
}

/// Query string of the openings list.
#[derive(Deserialize)]
pub struct OpeningsQuery {
    /// `includeClosed=1` also returns the rounds that are over.
    #[serde(default, rename = "includeClosed")]
    include_closed: Option<String>,
}

/// `GET /hr/openings[?includeClosed=1]` → `{"openings":[…]}` — **HR only**:
/// what this tenant is hiring for, with the size of each pipeline.
///
/// # Errors
/// `401`/`403` per the HR door.
pub async fn list_openings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<OpeningsQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let openings = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_openings(flag(q.include_closed.as_deref()))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "openings": openings.iter().map(opening_json).collect::<Vec<_>>(),
    })))
}

/// `POST /hr/openings` `{title, team?, location?, employmentKind?}` →
/// `{"opening":{…}}` — **HR only**: write down a role, as a draft.
///
/// # Errors
/// `401`/`403` per the HR door; `422` on a blank or over-long field or an
/// employment kind this build does not know; `400` on a malformed body.
pub async fn create_opening(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: OpeningBody = parse_body(&body)?;
    let input = req.apply(NewOpening::default())?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = hr
        .create_hr_opening(&input, &account.user)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "opening": opening_json(&load_opening(&hr, &id).await?) }),
    ))
}

/// `GET /hr/openings/{id}` → `{"opening":{…}}` — **HR only**.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's.
pub async fn get_opening(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let opening = load_opening(&hr, &HrOpeningId::new(id)).await?;
    Ok(Json(json!({ "opening": opening_json(&opening) })))
}

/// `PATCH /hr/openings/{id}` `{any writable field}` → `{"opening":{…}}` —
/// **HR only**.
///
/// A **closed** opening is frozen: rewriting the title of a role thirty people
/// applied for would rewrite what they applied for.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's;
/// `409` when it is closed; `422` on a field the caller can fix.
pub async fn update_opening(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: OpeningBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrOpeningId::new(id);
    let stored = load_opening(&hr, &id).await?;
    let input = req.apply(editable_opening(&stored))?;
    hr.update_hr_opening(&id, &input)
        .await
        .map_err(map_store_err)?;
    Ok(Json(
        json!({ "opening": opening_json(&load_opening(&hr, &id).await?) }),
    ))
}

/// `POST /hr/openings/{id}/publish` → `{"opening":{…}}` — **HR only**: the
/// round is running from today.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's;
/// `409` when it is not a draft, naming the state it is in.
pub async fn publish_opening(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrOpeningId::new(id);
    hr.publish_hr_opening(&id).await.map_err(map_store_err)?;
    Ok(Json(
        json!({ "opening": opening_json(&load_opening(&hr, &id).await?) }),
    ))
}

/// `POST /hr/openings/{id}/close` → `{"opening":{…}}` — **HR only**: the round
/// is over, however it ended.
///
/// Terminal, and the applicants stay: they are the record of what happened. A
/// role being hired for again is next year's opening.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's;
/// `409` when it is already closed.
pub async fn close_opening(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrOpeningId::new(id);
    hr.close_hr_opening(&id).await.map_err(map_store_err)?;
    Ok(Json(
        json!({ "opening": opening_json(&load_opening(&hr, &id).await?) }),
    ))
}

/// `GET /hr/openings/{id}/applicants` → `{"applicants":[…],"stages":[…]}` —
/// **HR only**: the pipeline, in board order, with the vocabulary the board's
/// columns are drawn from.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's.
pub async fn list_applicants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let applicants = state
        .store
        .for_tenant(account.tenant.clone())
        .hr_applicants(&HrOpeningId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "applicants": applicants.iter().map(applicant_json).collect::<Vec<_>>(),
        "stages": stages_json(),
    })))
}

/// `POST /hr/openings/{id}/applicants` `{name, email?, phone?, source?, cv?,
/// retainUntil?}` → `{"applicant":{…}}` — **HR only**: somebody applied.
///
/// They land at `applied`; every move after that is a person's act through
/// `POST /hr/applicants/{id}/move`. The CV, when there is one, is uploaded into
/// the tenant's HR area and **never read**.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the opening is not this tenant's;
/// `409` when it is closed; `422` on a field the caller can fix; `400` on a
/// body without a name or with a CV missing its blob.
pub async fn record_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: ApplicantBody = parse_body(&body)?;
    let opening = HrOpeningId::new(id);
    let hr = state.store.for_tenant(account.tenant.clone());
    // The opening is proved to be this tenant's and open **before** a file is
    // written, so an id from another tenant cannot leave an orphan CV in our HR
    // area.
    let stored = load_opening(&hr, &opening).await?;
    if stored.status == OpeningStatus::Closed {
        return Err(Problem::with(
            StatusCode::CONFLICT,
            "this opening is closed; applications belong to a round that is running",
        ));
    }
    let mut input = req.apply(NewApplicant::default())?;
    if let Some(Some(cv)) = req.cv.as_ref() {
        input.cv_node_id = Some(upload_cv(&account, cv).await?);
    }
    let created = hr.record_hr_applicant(&opening, &input).await;
    let created = match created {
        Ok(id) => id,
        Err(error) => {
            // The row did not land, so the file it would have belonged to must
            // not stay behind in the HR area.
            if let Some(node) = input.cv_node_id.as_ref() {
                discard_cv(&account, node).await;
            }
            return Err(map_store_err(error));
        }
    };
    Ok(Json(json!({
        "applicant": applicant_json(&load_applicant(&hr, &created).await?),
    })))
}

/// `GET /hr/applicants/{id}` → `{"applicant":{…},"notes":[…],"stages":[…]}` —
/// **HR only**: one candidate, what was written about them, and where they can
/// be moved to.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the candidate is not this tenant's.
pub async fn get_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrApplicantId::new(id);
    let applicant = load_applicant(&hr, &id).await?;
    let notes = hr.hr_applicant_notes(&id).await.map_err(map_store_err)?;
    Ok(Json(json!({
        "applicant": applicant_json(&applicant),
        "notes": notes.iter().map(note_json).collect::<Vec<_>>(),
        "stages": stages_json(),
    })))
}

/// `PATCH /hr/applicants/{id}` `{any writable field}` → `{"applicant":{…}}` —
/// **HR only**: correct what was recorded.
///
/// Never the stage (see [`move_applicant`]). A CV stated here replaces the one
/// on file and the replaced file is trashed; `"cv": null` takes it off the same
/// way.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the candidate is not this tenant's;
/// `422` on a field the caller can fix; `400` on a malformed body.
pub async fn update_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: ApplicantBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrApplicantId::new(id);
    let stored = load_applicant(&hr, &id).await?;
    let mut input = req.apply(editable_applicant(&stored))?;
    // `None` = the CV is not mentioned; `Some(None)` = take it off;
    // `Some(Some(_))` = this file replaces it.
    let replaced = match req.cv.as_ref() {
        None => None,
        Some(None) => {
            input.cv_node_id = None;
            stored.cv_node_id.clone()
        }
        Some(Some(cv)) => {
            input.cv_node_id = Some(upload_cv(&account, cv).await?);
            stored.cv_node_id.clone()
        }
    };
    hr.update_hr_applicant(&id, &input)
        .await
        .map_err(map_store_err)?;
    // Only once the row is correct: a file trashed before a refused write would
    // be a CV lost to a validation error.
    if let Some(node) = replaced.as_ref() {
        discard_cv(&account, node).await;
    }
    Ok(Json(json!({
        "applicant": applicant_json(&load_applicant(&hr, &id).await?),
    })))
}

/// The body of a move: where the person decided this candidate now stands.
#[derive(Deserialize)]
struct MoveBody {
    /// One of `applied`, `reviewing`, `interview`, `offer`, `hired`,
    /// `rejected`, `withdrawn`.
    stage: String,
}

/// `POST /hr/applicants/{id}/move` `{"stage":"interview"}` →
/// `{"applicant":{…}}` — **HR only, and the only way a stage changes**.
///
/// Audited as `hr.applicant.move` with the caller on it, because this is the
/// route a decision about a person is made through, and the record that a human
/// made it is the point (`docs/design/hr.md`, "The EU AI Act posture"). Any
/// stage may follow any other: a rejection reversed and a candidate who comes
/// back are ordinary.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the candidate is not this tenant's;
/// `422` listing the stages when the word is not one of them.
pub async fn move_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: MoveBody = parse_body(&body)?;
    let stage = ApplicantStage::parse(&req.stage).map_err(map_store_err)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrApplicantId::new(id);
    hr.move_hr_applicant(&id, stage)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "applicant": applicant_json(&load_applicant(&hr, &id).await?),
    })))
}

/// The body of a note.
#[derive(Deserialize)]
struct NoteBody {
    /// What the person who was in the room wrote.
    body: String,
}

/// `POST /hr/applicants/{id}/notes` `{"body":"…"}` → `{"note":{…}}` — **HR
/// only**: an interview note, with its author on it.
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the candidate is not this tenant's;
/// `422` on a blank or over-long note.
pub async fn add_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let req: NoteBody = parse_body(&body)?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrApplicantId::new(id);
    let note = hr
        .add_hr_applicant_note(&id, &account.user, &req.body)
        .await
        .map_err(map_store_err)?;
    let written = hr
        .hr_applicant_notes(&id)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|stored| stored.id.as_str() == note.as_str())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such note"))?;
    Ok(Json(json!({ "note": note_json(&written) })))
}

/// `DELETE /hr/applicants/{id}` → `{"erased":true}` — **HR only**: the
/// retention deadline, acted on.
///
/// Removes the candidate's record, every note on it and their CV. This is the
/// one HR record that is deleted rather than archived: an employee's file
/// carries statutory retention in every member state, an unsuccessful
/// applicant's has none behind it, and a tombstone would be the same personal
/// data under another name.
///
/// Nothing calls this on a timer. `retentionExpired` says which records are past
/// their date; a person decides (`docs/design/hr.md`, "Out of scope").
///
/// # Errors
/// `401`/`403` per the HR door; `404` when the candidate is not this tenant's.
pub async fn delete_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_hr()?;
    let hr = state.store.for_tenant(account.tenant.clone());
    let id = HrApplicantId::new(id);
    let stored = load_applicant(&hr, &id).await?;
    hr.delete_hr_applicant(&id).await.map_err(map_store_err)?;
    if let Some(node) = stored.cv_node_id.as_ref() {
        discard_cv(&account, node).await;
    }
    Ok(Json(json!({ "erased": true })))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn opening(value: Value) -> OpeningBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn applicant(value: Value) -> ApplicantBody {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("body rejected: {e}"))
    }

    fn stored_opening() -> NewOpening {
        NewOpening {
            title: "Backend engineer".to_owned(),
            team: "Platform".to_owned(),
            location: "Rotterdam".to_owned(),
            employment_kind: ContractKind::Permanent,
        }
    }

    fn stored_applicant() -> NewApplicant {
        NewApplicant {
            name: "Amara Diallo".to_owned(),
            email: Some("amara@example.test".to_owned()),
            phone: "+31 6 1234 5678".to_owned(),
            source: "referral, Anna".to_owned(),
            cv_node_id: Some(DriveNodeId::new("node-1".to_owned())),
            retain_until: None,
        }
    }

    #[test]
    fn an_empty_patch_changes_nothing() {
        let merged = opening(json!({}))
            .apply(stored_opening())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.title, "Backend engineer");
        assert_eq!(merged.location, "Rotterdam");
        assert_eq!(merged.employment_kind, ContractKind::Permanent);

        let person = applicant(json!({}))
            .apply(stored_applicant())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(person.name, "Amara Diallo");
        assert_eq!(person.email.as_deref(), Some("amara@example.test"));
        assert_eq!(person.source, "referral, Anna");
    }

    #[test]
    fn an_explicit_null_clears_an_address_and_absence_does_not() {
        let cleared = applicant(json!({ "email": null }))
            .apply(stored_applicant())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(cleared.email, None);
        let kept = applicant(json!({ "phone": "+31 6 0000 0000" }))
            .apply(stored_applicant())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(kept.email.as_deref(), Some("amara@example.test"));
        assert_eq!(kept.phone, "+31 6 0000 0000");
    }

    #[test]
    fn a_body_cannot_move_somebody_through_the_edit_route() {
        // `stage` is not a writable field: a client that sends one is answered
        // with the record unchanged, and the move route is the only door.
        let merged = applicant(json!({ "stage": "hired" }))
            .apply(stored_applicant())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(merged.name, "Amara Diallo");
    }

    #[test]
    fn a_retention_date_must_be_a_day_and_nothing_else() {
        let refused =
            applicant(json!({ "retainUntil": "2026-08-11T00:00:00Z" })).apply(stored_applicant());
        assert!(refused.is_err(), "an instant is not a day");
        let ok = applicant(json!({ "retainUntil": "2027-02-28" }))
            .apply(stored_applicant())
            .unwrap_or_else(|e| panic!("rejected: {e:?}"));
        assert_eq!(ok.retain_until.map(iso_date).as_deref(), Some("2027-02-28"));
    }

    #[test]
    fn a_word_this_build_does_not_know_is_refused() {
        assert!(
            opening(json!({ "employmentKind": "freelance-ish" }))
                .apply(stored_opening())
                .is_err()
        );
        assert!(ApplicantStage::parse("shortlisted").is_err());
        assert_eq!(stages_json().first().copied(), Some("applied"));
        assert_eq!(stages_json().len(), 7);
    }
}
