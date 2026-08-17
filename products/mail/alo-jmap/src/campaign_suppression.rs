//! `/campaigns/suppressions` (ADR 0044 §2, wave C1) — the addresses this tenant
//! may never mail again.
//!
//! **There is no route that lifts one, and there never will be.** No `DELETE`,
//! no `PATCH`, no `restore`. The store has no method to call either
//! (`campaign_suppression.rs` holds its own SQL to that), so this is not merely
//! an unbuilt endpoint: an API that can remove a suppression is an API a bulk
//! importer is eventually pointed at, and "no segment, import or re-upload can
//! bring them back" would then be true only until somebody was in a hurry.
//! Somebody who suppressed themselves by mistake gives fresh consent through
//! the site form like anybody else — which is evidence, where a tenant deleting
//! a row is not.
//!
//! **`POST` is idempotent and the first reason stands.** Posting for somebody
//! already suppressed answers `200` with the record already in force, not a
//! `409`: the caller asked for a state, and the state holds. A hard bounce
//! three months after an unsubscribe must not rewrite "they asked to stop" into
//! "their mailbox was full", which reads as a technical problem somebody might
//! try to fix.
//!
//! This is the one campaigns surface that reads the **tenant** store rather
//! than the account one, because the loudest future source of these rows has no
//! logged-in colleague behind it at all: the one-click unsubscribe endpoint
//! (RFC 8058, queue item C2s.2) works with no account and no login.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{CampaignSuppression, NewSuppression, SUPPRESSION_PAGE_MAX, SuppressionReason};

use crate::billing::{blank_to_none, iso, map_store_err, parse_body, parse_rfc3339};
use crate::campaigns::{address_of, stated, unprocessable};
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// One suppression as JSON.
///
/// `personsDecision` is emitted because the difference is the one C4's numbers
/// will turn on: an unsubscribe or a complaint is somebody deciding, a hard
/// bounce is a dead mailbox saying nothing about whether the mail was wanted.
/// A screen that lumped them together would report a reputation problem where
/// there is an addressing one.
fn suppression_json(suppression: &CampaignSuppression) -> Value {
    json!({
        "id": suppression.id.as_str(),
        "address": suppression.address,
        "reason": suppression.reason.as_str(),
        "sourceRef": suppression.source_ref,
        "personsDecision": suppression.reason.is_a_persons_decision(),
        "occurredAt": iso(suppression.occurred_at),
        "recordedAt": iso(suppression.recorded_at),
    })
}

/// What a caller states when suppressing an address.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuppressionBody {
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    /// When it happened, RFC 3339. Absent means now — right for a click being
    /// handled this second, wrong for a bounce report processed hours later,
    /// which knows its own time.
    #[serde(default)]
    occurred_at: Option<String>,
}

/// How many suppressions to list.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    limit: Option<String>,
}

/// `POST /campaigns/suppressions` `{address, reason, sourceRef, occurredAt}` →
/// `{"suppression":{…}}` — this tenant will not mail that address again.
///
/// `reason` is one of `unsubscribe`, `hard_bounce`, `complaint`, `manual`. The
/// fourth is not in ADR 0044's list and is deliberate: recording the person who
/// telephones and asks to be taken off as an `unsubscribe` would put a phone
/// call into the number a sending reputation is judged on, and a complaint rate
/// that lies to us is worse than one that lies to a regulator.
pub async fn suppress_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let request: SuppressionBody = parse_body(&body)?;

    let address = address_of(request.address.as_deref().unwrap_or_default())?;
    let reason_token = stated(request.reason.as_deref()).ok_or_else(|| {
        unprocessable("reason must be one of unsubscribe, hard_bounce, complaint, manual")
    })?;
    let reason = SuppressionReason::parse(&reason_token.to_ascii_lowercase()).ok_or_else(|| {
        unprocessable("reason must be one of unsubscribe, hard_bounce, complaint, manual")
    })?;
    let occurred_at = match stated(request.occurred_at.as_deref()) {
        None => None,
        Some(raw) => Some(parse_rfc3339(raw).ok_or_else(|| {
            unprocessable("occurredAt is a full RFC 3339 timestamp, e.g. 2026-03-04T10:00:00Z")
        })?),
    };
    let source_ref = blank_to_none(request.source_ref);

    let suppression = state
        .store
        .for_tenant(account.tenant.clone())
        .suppress_campaign_address(&NewSuppression {
            address: &address,
            reason,
            source_ref: source_ref.as_deref(),
            occurred_at,
        })
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "suppression": suppression_json(&suppression),
    })))
}

/// `GET /campaigns/suppressions[?limit]` → `{"suppressions":[…]}` — who this
/// tenant has lost, and why, freshest first.
pub async fn list_suppressions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let limit = match stated(query.limit.as_deref()) {
        None => 100,
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|value| (1..=SUPPRESSION_PAGE_MAX).contains(value))
            .ok_or_else(|| {
                unprocessable(format!(
                    "limit is a whole number of people, 1 to {SUPPRESSION_PAGE_MAX}"
                ))
            })?,
    };
    let suppressions = state
        .store
        .for_tenant(account.tenant.clone())
        .campaign_suppressions(limit)
        .await
        .map_err(map_store_err)?;
    Ok(Json(json!({
        "suppressions": suppressions.iter().map(suppression_json).collect::<Vec<_>>(),
    })))
}

/// `GET /campaigns/suppressions/{address}` → `{"suppression":{…}}`, or `404`
/// when nothing suppresses that address.
///
/// The `404` here is honest rather than an oracle: it says only that *this
/// tenant* holds no suppression, which is the same answer a caller gets for an
/// address the tenant has never heard of and for another tenant's suppression.
/// It is not "may we mail them" — consent is a separate question, and the
/// audience answers both in one read.
pub async fn suppression_for(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(address): Path<String>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let address = address_of(&address)?;
    let suppression = state
        .store
        .for_tenant(account.tenant.clone())
        .campaign_suppression_for(&address)
        .await
        .map_err(map_store_err)?
        .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "not found"))?;
    Ok(Json(json!({
        "suppression": suppression_json(&suppression),
    })))
}
