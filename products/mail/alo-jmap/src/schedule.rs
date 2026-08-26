//! Send later (scheduled send): hold a validated draft until a chosen time
//! instead of submitting it now. `POST /send-later` runs the *exact* submission
//! validation an immediate `EmailSubmission/set` would (send-from rights,
//! anti-spoof `From:`, recipient sanity) and then records the schedule; a
//! background sweeper (`submission::run_due_scheduled`) sends it when due.
//! `POST /send-later/cancel` takes it back to Drafts. See `submission.rs`.

use alo_store::MessageId;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::{Account, AppState, authenticate, resolve_target};
use crate::submission;

/// A far-future guard on the schedule time (~2 years). A caller cannot pin a
/// draft in the queue indefinitely; anything beyond this is a bad request.
const MAX_SCHEDULE_HORIZON_SECS: i64 = 2 * 365 * 24 * 60 * 60;

/// Map a submission validation error (`{type, description}`) to an HTTP problem.
/// Ownership/spoof failures are a client 403; everything else a 400. The draft
/// body and recipients are never echoed (law #1) — only the coarse reason code.
fn submission_problem(err: &Value) -> Problem {
    let kind = err
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("invalidProperties");
    let status = match kind {
        "forbiddenFrom" | "forbiddenToSend" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    Problem::with(status, kind)
}

/// Resolves the account a send-later call targets: the signed-in user's own
/// (no/own `accountId`), or a shared mailbox they hold a delegation grant on
/// (ADR 0017). An ungranted or foreign `accountId` is the same 404 as an
/// unknown draft — no oracle for "exists but not yours".
async fn target_account(
    state: &AppState,
    headers: &HeaderMap,
    body: &Value,
) -> Result<Account, Problem> {
    let signed_in = authenticate(state, headers).await?;
    match body.get("accountId").and_then(Value::as_str) {
        None => Ok(signed_in),
        Some(id) => resolve_target(&signed_in, state, id)
            .await
            .ok_or_else(|| Problem::with(StatusCode::NOT_FOUND, "notFound")),
    }
}

/// `POST /send-later` — `{"emailId": "...", "envelope": {...}, "sendAt": <unix>,
/// "accountId"?: "..."}` → `{"scheduled": true, "sendAt": <unix>}`. Validates
/// like an immediate send, then moves the draft to Scheduled and records the
/// send time. `accountId` targets a delegated shared mailbox (ADR 0017): the
/// draft, the Scheduled folder, and the eventual Sent copy all live in the
/// shared mailbox, and the send grant is enforced exactly as an immediate
/// submission's.
pub async fn send_later(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Problem> {
    let account = target_account(&state, &headers, &body).await?;
    let now = now_epoch();
    let send_at = body
        .get("sendAt")
        .and_then(Value::as_i64)
        .filter(|&t| t > 0 && t <= now + MAX_SCHEDULE_HORIZON_SECS)
        .ok_or_else(|| {
            Problem::with(
                StatusCode::BAD_REQUEST,
                "sendAt is required and must be a near-future time",
            )
        })?;

    // Same validation as an immediate submission — reject a forbidden send now,
    // not silently at sweep time.
    let prepared = submission::validate_and_prepare(&account, &body, &state)
        .await
        .map_err(|e| submission_problem(&e))?;

    account
        .acc
        .schedule_send(
            &prepared.mid,
            &prepared.mail_from,
            &prepared.rcpts,
            send_at,
            prepared.on_behalf_sender.as_deref(),
        )
        .await
        .map_err(|_| Problem::server_error())?;

    Ok(Json(json!({ "scheduled": true, "sendAt": send_at })))
}

/// `POST /send-later/cancel` — `{"emailId": "...", "accountId"?: "..."}` →
/// `{"cancelled": <bool>}`. Deletes the schedule and returns the draft to
/// Drafts; a no-op if the message wasn't scheduled. On a delegated shared
/// mailbox (`accountId`, ADR 0017) cancelling is a mutation, so a read-only
/// delegate is refused.
pub async fn cancel_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Problem> {
    let account = target_account(&state, &headers, &body).await?;
    if let Some(d) = &account.delegated
        && !d.can_write
    {
        return Err(Problem::with(StatusCode::FORBIDDEN, "accountReadOnly"));
    }
    let email_id = body
        .get("emailId")
        .and_then(Value::as_str)
        .ok_or_else(|| Problem::with(StatusCode::BAD_REQUEST, "emailId is required"))?;
    let cancelled = account
        .acc
        .cancel_send(&MessageId::new(email_id))
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "cancelled": cancelled })))
}

/// Current Unix time in seconds. A thin wrapper so the horizon check reads
/// clearly; the system clock is the right source for "how far ahead is this".
fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
