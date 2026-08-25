//! Mail settings endpoints: the signed-in user's signature (read + write) and
//! the tenant's organization footer (read for everyone, write for admins). Both
//! are HTML fragments the compose surface inserts into outgoing mail.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::{Json, body::Bytes};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// `GET /settings/mail` → `{ signature, orgFooter }` — the caller's own
/// signature plus the tenant footer, both for the compose surface.
pub async fn mail_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let signature = account
        .acc
        .signature()
        .await
        .map_err(|_| Problem::server_error())?;
    let org_footer = state
        .store
        .org_footer(&account.tenant)
        .await
        .map_err(|_| Problem::server_error())?;
    let ooo = account
        .acc
        .out_of_office()
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({
        "signature": signature,
        "orgFooter": org_footer,
        "outOfOffice": {
            "enabled": ooo.enabled,
            "subject": ooo.subject,
            "message": ooo.message,
            // The window the web settings screen edits, as plain days that go
            // straight into a date input.
            //
            // The stored end is exclusive — the first moment of the day you are
            // back — so the day to *show* is the one before it: type the 15th,
            // see the 15th. Taking a second off finds that day for any stored
            // instant, including one a JMAP client set to something other than
            // midnight.
            "from": ooo.from.map(|t| t.date().to_string()),
            "to": ooo.to.map(|t| (t - time::Duration::seconds(1)).date().to_string()),
        },
    })))
}

/// `POST /settings/out-of-office` — set the caller's auto-reply. Body
/// `{ enabled, subject?, message?, from?, to? }`. A non-empty message is
/// required to enable.
///
/// `from` and `to` are plain dates (`YYYY-MM-DD`), because that is what the
/// person setting them thinks in — "I am away from the 4th". `from` starts at
/// the beginning of that day and `to` ends at the beginning of its own, so a
/// holiday "to the 15th" stops replying on the 15th: whoever wrote to you that
/// morning should reach a person who is back, not an auto-reply.
pub async fn set_out_of_office(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let enabled = v.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    let subject = v
        .get("subject")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let message = v
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if enabled && message.is_empty() {
        return Err(Problem::with(
            axum::http::StatusCode::BAD_REQUEST,
            "a message is required to turn on out-of-office",
        ));
    }

    let from = parse_day(v.get("from"), false)?;
    let to = parse_day(v.get("to"), true)?;
    if let (Some(start), Some(end)) = (from, to)
        && start >= end
    {
        return Err(Problem::with(
            axum::http::StatusCode::BAD_REQUEST,
            "the last day must not be before the first",
        ));
    }
    // Persist the state only, then rebuild the single managed Sieve script so
    // vacation coexists with any mail filters (one active script per account).
    account
        .acc
        .set_out_of_office_state(enabled, subject, message, from, to)
        .await
        .map_err(|_| Problem::server_error())?;
    crate::filters::rebuild_managed_script(&account).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Reads a `YYYY-MM-DD` day from the request body into an instant.
///
/// `end_of_day` makes the difference between the two bounds: a start is the
/// first moment of that day, and an end is the first moment of the day *after*
/// the one named, so that "away until the 15th" covers the whole of the 15th.
/// Getting this backwards costs somebody a day of replies in one direction or
/// the other, and neither is visible until a real message arrives.
fn parse_day(value: Option<&Value>, end_of_day: bool) -> Result<Option<OffsetDateTime>, Problem> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().unwrap_or("").trim();
    if text.is_empty() {
        return Ok(None);
    }
    let date = time::Date::parse(
        text,
        &time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| {
        Problem::with(
            axum::http::StatusCode::BAD_REQUEST,
            "dates must be written YYYY-MM-DD",
        )
    })?;
    let date = if end_of_day {
        date.next_day().unwrap_or(date)
    } else {
        date
    };
    Ok(Some(date.midnight().assume_utc()))
}

/// `POST /settings/signature` — set the caller's signature. Body
/// `{ signature }` (HTML; empty clears it).
pub async fn set_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let signature = v.get("signature").and_then(Value::as_str).unwrap_or("");
    account
        .acc
        .set_signature(signature)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/org-footer` — set the tenant's organization footer (admin
/// only). Body `{ footer }` (HTML; empty clears it).
pub async fn set_org_footer(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    account.require_admin()?;
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let footer = v.get("footer").and_then(Value::as_str).unwrap_or("");
    state
        .store
        .set_org_footer(&account.tenant, footer)
        .await
        .map_err(|_| Problem::server_error())?;
    Ok(Json(json!({ "ok": true })))
}
