//! Deal ↔ mail linking HTTP surface (alo CRM, ADR 0035, wave B2) — the
//! conversations a deal belongs to, on top of [`alo_store::crm_deal_threads`].
//!
//! The rest of `/crm/*` defends the tenant. This module defends one boundary
//! *inside* it: a deal is tenant-wide, a mailbox is not
//! (`docs/design/crm.md`, "Deal ↔ mail thread"). Three rules follow, and the
//! store enforces all three — the edge's job is to keep the vocabulary honest.
//!
//! - **A link is a confirmation, never a consequence.** `GET
//!   …/thread-suggestions` proposes and writes nothing; only `POST …/threads`
//!   links, and it links exactly the one conversation it was handed.
//! - **A link stores no message content.** The reply carries the conversation's
//!   subject, who linked it, and whether **this** reader can open it — never a
//!   body, an address list or a message count.
//! - **A thread the caller has no message in is the same `404` as one that does
//!   not exist**, so the route cannot be used to ask whether a conversation
//!   exists in a colleague's mailbox.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::ThreadId;
use alo_store::crm_deal_threads::{DealThread, SUGGESTIONS_MAX, ThreadSuggestion};
use alo_store::{AccountStore, CrmDealId};

use crate::billing::{iso, map_store_err, parse_body};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How many conversations a suggestion call answers with when the caller does
/// not say. A drawer shows a handful and a longer list stops being a proposal.
const SUGGESTIONS_DEFAULT: usize = 10;

/// A linked conversation as JSON.
///
/// `readable` is computed for **this** caller: a colleague who holds the
/// conversation can open it in mail, and one who does not still sees that it is
/// linked, what it is called, and who linked it — the useful answer being "ask
/// Sam" rather than a silent gap.
fn thread_json(t: &DealThread) -> Value {
    json!({
        "threadId": t.thread_id.as_str(),
        "subject": t.subject,
        "readable": t.readable,
        "linkedBy": t.linked_by,
        "linkedAt": iso(t.linked_at),
    })
}

/// A proposed conversation as JSON. `reason` and `matchedAddress` are what make
/// it reviewable: a user is told *why* before they confirm.
fn suggestion_json(s: &ThreadSuggestion) -> Value {
    json!({
        "threadId": s.thread_id.as_str(),
        "subject": s.subject,
        "reason": s.reason.as_str(),
        "matchedAddress": s.matched_address,
        "lastMessageAt": iso(s.last_message_at),
    })
}

/// `GET /crm/deals/{id}/threads` → `{"threads":[…]}` — the conversations linked
/// to a deal, most recently linked first.
///
/// A deal that is not this tenant's is the same `404` an id that never existed
/// gets, never an empty list.
pub async fn list_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let threads = account
        .acc
        .crm_deal_threads(&CrmDealId::new(id))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "threads": threads.iter().map(thread_json).collect::<Vec<_>>(),
    })))
}

/// The body of the link route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkBody {
    #[serde(default)]
    thread_id: Option<String>,
}

/// Reads an id a request must state, refusing a blank one with a `422`.
fn required_thread_id(raw: Option<&str>) -> Result<ThreadId, Problem> {
    raw.map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| ThreadId::new(v.to_owned()))
        .ok_or_else(|| Problem::with(StatusCode::UNPROCESSABLE_ENTITY, "threadId is required"))
}

/// `POST /crm/deals/{id}/threads` `{threadId}` → `{"thread":{…},"created":bool}`
/// — attach a conversation the caller can already see.
///
/// Idempotent: linking a conversation that is already linked answers `200` with
/// `created:false`, because linking twice is the same link and not something a
/// user should have to read an error about.
///
/// The conversation must resolve through the **caller's own** mail. One of
/// another tenant, one that does not exist, and one the caller simply has no
/// message in are the same `404` — the route is not an oracle for what is in a
/// colleague's mailbox.
pub async fn link_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let req: LinkBody = parse_body(&body)?;
    let thread = required_thread_id(req.thread_id.as_deref())?;
    let deal = CrmDealId::new(id);
    let created = account
        .acc
        .link_crm_deal_thread(&deal, &thread)
        .await
        .map_err(map_store_err)?;
    let linked = one_link(&account.acc, &deal, &thread).await?;
    Ok(Json(json!({
        "thread": thread_json(&linked),
        "created": created,
    })))
}

/// Reads back the link just written, so the answer is the stored record rather
/// than an echo of the request — the same contract every billing write holds.
async fn one_link(
    acc: &AccountStore,
    deal: &CrmDealId,
    thread: &ThreadId,
) -> Result<DealThread, Problem> {
    acc.crm_deal_threads(deal)
        .await
        .map_err(map_store_err)?
        .into_iter()
        .find(|t| t.thread_id.as_str() == thread.as_str())
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "no such link"))
}

/// `DELETE /crm/deals/{id}/threads/{threadId}` → `{"unlinked":true}`.
///
/// Any member of the tenant may unlink, including one who cannot open the
/// conversation: a link left by a colleague who has since left would otherwise
/// be permanent. Nothing is destroyed — the link never held the mail.
pub async fn unlink_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, thread)): Path<(String, String)>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account
        .acc
        .unlink_crm_deal_thread(&CrmDealId::new(id), &ThreadId::new(thread))
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({ "unlinked": true })))
}

/// Query string of the suggestions route.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// `GET /crm/deals/{id}/thread-suggestions[?limit]` → `{"suggestions":[…]}` —
/// conversations worth linking, computed over the **caller's own** recent mail.
///
/// It links nothing and it never reaches into a colleague's mailbox. A deal with
/// no usable address answers with an empty list rather than everything recent,
/// and `limit` is clamped rather than refused: it is a page size, not an
/// assertion about the data.
pub async fn suggest_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<SuggestQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let limit = q
        .limit
        .unwrap_or(SUGGESTIONS_DEFAULT)
        .clamp(1, SUGGESTIONS_MAX);
    let suggestions = account
        .acc
        .suggest_crm_deal_threads(&CrmDealId::new(id), limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "suggestions": suggestions.iter().map(suggestion_json).collect::<Vec<_>>(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_states_a_conversation_or_is_refused() {
        let body: LinkBody = serde_json::from_value(json!({ "threadId": "  thr_1  " }))
            .unwrap_or_else(|e| {
                panic!("body rejected: {e}");
            });
        assert_eq!(
            required_thread_id(body.thread_id.as_deref())
                .map(|t| t.as_str().to_owned())
                .ok(),
            Some("thr_1".to_owned())
        );
        for absent in [None, Some(""), Some("   ")] {
            let problem = required_thread_id(absent)
                .err()
                .unwrap_or_else(|| panic!("accepted {absent:?}"));
            assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(problem.detail.as_deref(), Some("threadId is required"));
        }
    }

    #[test]
    fn an_absent_link_body_is_a_422_not_a_400() {
        // `{}` is well-formed JSON that simply does not say which conversation:
        // a malformed *request*, not malformed bytes.
        let body: LinkBody = serde_json::from_value(json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert!(required_thread_id(body.thread_id.as_deref()).is_err());
    }

    #[test]
    fn the_suggestion_limit_is_clamped_never_refused() {
        let clamp =
            |raw: Option<usize>| raw.unwrap_or(SUGGESTIONS_DEFAULT).clamp(1, SUGGESTIONS_MAX);
        assert_eq!(clamp(None), SUGGESTIONS_DEFAULT);
        assert_eq!(clamp(Some(0)), 1);
        assert_eq!(clamp(Some(3)), 3);
        assert_eq!(clamp(Some(usize::MAX)), SUGGESTIONS_MAX);
    }
}
