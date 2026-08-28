//! A deal's log HTTP surface (alo CRM, ADR 0035, wave B2) — the notes, calls
//! and meetings on a deal, on top of [`alo_store::crm_activities`].
//!
//! Three rules from the design note (`docs/design/crm.md`, "Activities and next
//! steps") are what this module is shaped by:
//!
//! - **There is no edit.** A correction is another note, so the surface is
//!   `GET`, `POST` and `DELETE` and nothing else — a log that can be rewritten
//!   is not a log of what was said and done.
//! - **Only the author may delete**, and a colleague who tries reads `403`
//!   rather than `404`: they can already see the entry, so hiding its existence
//!   would be theatre.
//! - **`kind` is a closed vocabulary.** An unrecognised one is a `422` and never
//!   a silent `note`, because a log that quietly demotes a call to a note is
//!   worse than one that refuses the word.
//!
//! A **next step is not here**: it is a real task, and it lives in
//! [`crate::crm_next_steps`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;

use alo_store::crm_activities::{Activity, ActivityKind, NewActivity};
use alo_store::{CrmActivityId, CrmDealId};

use crate::billing::{iso, map_store_err, parse_body, parse_rfc3339};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One entry as JSON.
///
/// `happenedAt` is when it happened and `createdAt` when it was written: a call
/// logged an hour later is dated the hour it took place, and a reader can see
/// both rather than being told a story about one.
pub(crate) fn activity_json(a: &Activity) -> Value {
    json!({
        "id": a.id.as_str(),
        "dealId": a.deal_id.as_str(),
        "kind": a.kind.as_str(),
        "body": a.body,
        "happenedAt": iso(a.happened_at),
        "authorUserId": a.author_user_id,
        "createdAt": iso(a.created_at),
    })
}

/// The body of the write route. Everything but `body` is optional: an entry
/// nobody typed a kind for is a note, and one nobody dated happened now.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityBody {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    happened_at: Option<String>,
}

/// Reads the kind, refusing a word the vocabulary does not have.
fn kind_of(raw: Option<&str>) -> Result<ActivityKind, Problem> {
    let Some(stated) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(ActivityKind::default());
    };
    ActivityKind::parse(&stated.to_ascii_lowercase()).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "kind must be one of note, call, meeting",
        )
    })
}

/// Reads `happenedAt`, which a caller may state as a full RFC 3339 timestamp
/// and nothing else.
///
/// Unlike a deal's `expectedClose` — a **day**, which must not be written as a
/// timestamp — this is an instant: a call at 16:05 in Warsaw happened at one
/// moment, and the zone is part of the record. It is normalised to UTC, the way
/// every stored instant in alo is.
fn happened_at(raw: Option<&str>) -> Result<Option<OffsetDateTime>, Problem> {
    let Some(stated) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    parse_rfc3339(stated).map(Some).ok_or_else(|| {
        Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "happenedAt must be an RFC 3339 timestamp",
        )
    })
}

/// `GET /crm/deals/{id}/activities` → `{"activities":[…]}` — one deal's log,
/// most recent first.
///
/// Readable by every member of the tenant, exactly like the deal it hangs on. A
/// deal that is not this tenant's is the same `404` an id that never existed
/// gets, never an empty list.
pub async fn list_activities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let activities = account
        .acc
        .crm_activities(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "activities": activities.iter().map(activity_json).collect::<Vec<_>>(),
    })))
}

/// `POST /crm/deals/{id}/activities` `{body, kind?, happenedAt?}` →
/// `{"activity":{…}}` — write one entry into a deal's log.
///
/// The answer carries the **stored** record rather than an echo of the request,
/// the same contract every billing write holds.
pub async fn add_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: ActivityBody = parse_body(&body)?;
    let input = NewActivity {
        kind: kind_of(req.kind.as_deref())?,
        body: req.body.unwrap_or_default(),
        happened_at: happened_at(req.happened_at.as_deref())?,
    };
    let deal = CrmDealId::new(id);
    let written = account
        .acc
        .add_crm_activity(&deal, &input)
        .await
        .map_err(map_store_err)?;
    let stored = account
        .acc
        .crm_activities(&deal)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|a| a.id.as_str() == written.as_str())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such activity"))?;
    Ok(Json(json!({ "activity": activity_json(&stored) })))
}

/// `DELETE /crm/activities/{id}` → `{"deleted":true}` — remove an entry you
/// wrote.
///
/// A colleague who did not write it reads `403`, not `404`: the entry is
/// readable tenant-wide, so pretending it does not exist would be theatre. An
/// entry of another tenant is the ordinary `404`.
pub async fn delete_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .delete_crm_activity(&CrmActivityId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unstated_kind_is_a_note_and_an_unknown_one_is_refused() {
        for absent in [None, Some(""), Some("   ")] {
            assert_eq!(kind_of(absent).ok(), Some(ActivityKind::Note));
        }
        assert_eq!(kind_of(Some(" CALL ")).ok(), Some(ActivityKind::Call));
        assert_eq!(kind_of(Some("meeting")).ok(), Some(ActivityKind::Meeting));
        for bad in ["email", "task", "phonecall", "0"] {
            let problem = kind_of(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
            assert_eq!(
                problem.detail.as_deref(),
                Some("kind must be one of note, call, meeting")
            );
        }
    }

    #[test]
    fn a_time_is_an_instant_or_a_refusal() {
        let parsed = happened_at(Some("2026-08-07T16:05:00+02:00"))
            .ok()
            .flatten()
            .unwrap_or(OffsetDateTime::UNIX_EPOCH);
        // Stored as UTC, so the zone a call was dialled in never changes when it
        // happened.
        assert_eq!(parsed.offset(), time::UtcOffset::UTC);
        assert_eq!(parsed.hour(), 14);
        for absent in [None, Some(""), Some("  ")] {
            assert_eq!(happened_at(absent).ok(), Some(None));
        }
        for bad in ["2026-08-07", "yesterday", "07/08/2026", "1754582700"] {
            let problem = happened_at(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("accepted {bad}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        }
    }

    #[test]
    fn a_body_is_carried_verbatim_and_the_store_owns_the_bound() {
        // The edge does not second-guess the store's validation: a blank body is
        // one rule, named once, in the place that also writes the row.
        let req: ActivityBody =
            serde_json::from_value(json!({ "body": "  " })).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(req.body.as_deref(), Some("  "));
        let absent: ActivityBody =
            serde_json::from_value(json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert!(absent.body.is_none() && absent.kind.is_none());
    }
}
