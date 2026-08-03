//! Public self-service password-reset surface (ADR 0018 follow-up).
//!
//! Two unauthenticated endpoints let a person who has forgotten their password
//! prove they still control the recovery mailbox captured at signup, and set a
//! new password:
//!
//! - `POST /reset/request` — mail a code to the account's recovery mailbox.
//! - `POST /reset/verify` — code + new password → the credential is re-hashed.
//!
//! Defences mirror signup ([`crate::signup_route`], whose stateless helpers are
//! reused): the account address must be on a configured personal domain; the
//! code is stored only as a salted SHA-256 hash, compared in constant time,
//! attempt-capped and short-lived; the client IP and account address are
//! rate-limited. Crucially, `/reset/request` ALWAYS answers `{status:"sent"}` —
//! whether or not the account exists or has a recovery mailbox on file — so it
//! never reveals which addresses are registered. The code and recovery address
//! are never logged (Law 1).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_identity::secret;

use crate::error::Problem;
use crate::signup_route::{client_ip, generate_code, is_personal_domain, salt, split_address};
use crate::state::AppState;

/// How long a reset code is valid.
const CODE_TTL_SECS: i64 = 600;
/// Verify attempts allowed before the pending reset is burned.
const MAX_VERIFY_ATTEMPTS: i32 = 6;
/// Minimum password length for a personal account (matches signup).
const MIN_PASSWORD: usize = 8;

#[derive(Deserialize)]
struct RequestBody {
    address: String,
}

#[derive(Deserialize)]
struct VerifyBody {
    address: String,
    code: String,
    password: String,
}

/// `POST /reset/request` → `{status:"sent"}`, always. If the address is a valid
/// personal address with a recovery mailbox on file, a fresh code is mailed to
/// it and a pending reset recorded. Otherwise it is a silent no-op — the
/// response is identical either way (no account enumeration). Rate-limited per
/// client IP and per account address.
pub async fn request(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: RequestBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let sent = Json(json!({ "status": "sent" }));

    // Parse leniently: a malformed or non-personal address is a silent no-op,
    // not an error, so the surface reveals nothing about which accounts exist.
    let Some((local, domain)) = split_address(&req.address) else {
        return Ok(sent);
    };
    if !is_personal_domain(&state, &domain) {
        return Ok(sent);
    }
    let address = format!("{local}@{domain}");

    // Throttle the client and the target account address.
    let ip = client_ip(&headers);
    for key in [format!("reset-ip|{ip}"), format!("reset-addr|{address}")] {
        if state.signup_limiter.retry_after(&key).is_some() {
            return Err(Problem::with(
                StatusCode::TOO_MANY_REQUESTS,
                "too many reset attempts — please wait a little and try again",
            ));
        }
    }
    state
        .signup_limiter
        .record_failure(&format!("reset-ip|{ip}"));
    state
        .signup_limiter
        .record_failure(&format!("reset-addr|{address}"));

    // Look up the recovery mailbox captured at signup. Absent (unknown account,
    // or one that predates recovery capture) → silent no-op.
    let recovery = match state.store.account_recovery_email(&address).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(sent),
        Err(error) => {
            tracing::warn!(%error, "reset: recovery lookup failed");
            return Err(Problem::server_error());
        }
    };

    // Keep the table bounded; best-effort.
    if let Err(error) = state.store.reap_expired_resets().await {
        tracing::warn!(%error, "reset: reap failed");
    }

    let code = generate_code();
    let code_hash = secret::hash_at_rest(&salt(&address, &code));
    state
        .store
        .upsert_pending_reset(&address, &recovery, &code_hash, CODE_TTL_SECS)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "reset: could not store pending reset");
            Problem::server_error()
        })?;

    send_reset_code(&state, &domain, &recovery, &code).await?;
    Ok(sent)
}

/// `POST /reset/verify` → `{status:"ok"}`. Checks the code, then sets the new
/// password and clears the pending reset.
pub async fn verify(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: VerifyBody = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let Some((local, domain)) = split_address(&req.address) else {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that address is not valid",
        ));
    };
    let address = format!("{local}@{domain}");
    if req.password.len() < MIN_PASSWORD {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "the password is too short",
        ));
    }

    let pending = state
        .store
        .pending_reset(&address)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::BAD_REQUEST,
                "no reset in progress for that address — start again",
            )
        })?;

    // Count the attempt first; burn the pending reset once the cap is hit so a
    // short code cannot be ground down.
    let attempts = state
        .store
        .bump_reset_attempts(&address)
        .await
        .map_err(|_| Problem::server_error())?;
    if attempts > MAX_VERIFY_ATTEMPTS {
        let _ = state.store.delete_pending_reset(&address).await;
        return Err(Problem::with(
            StatusCode::TOO_MANY_REQUESTS,
            "too many attempts — please start the reset again",
        ));
    }

    let given = secret::hash_at_rest(&salt(&address, req.code.trim()));
    if !secret::ct_eq(given.as_bytes(), pending.code_hash.as_bytes()) {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that code is not correct",
        ));
    }

    state
        .identity
        .reset_password(&address, &req.password)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "reset: setting the new password failed");
            Problem::server_error()
        })?;
    let _ = state.store.delete_pending_reset(&address).await;
    state
        .signup_limiter
        .record_success(&format!("reset-addr|{address}"));
    Ok(Json(json!({ "status": "ok" })))
}

/// Mails a reset code to `recovery` from a system address on the personal
/// domain, through the internal submission listener (which adds Date/Message-ID
/// and DKIM-signs). The code is never logged.
async fn send_reset_code(
    state: &AppState,
    domain: &str,
    recovery: &str,
    code: &str,
) -> Result<(), Problem> {
    let Some(addr) = state.submission_addr.as_deref() else {
        tracing::error!("reset: no submission listener configured");
        return Err(Problem::with(
            StatusCode::SERVICE_UNAVAILABLE,
            "password reset email is not available right now",
        ));
    };
    let mail_from = format!("noreply@{domain}");
    let message = format!(
        "From: alo <noreply@{domain}>\r\n\
         To: {recovery}\r\n\
         Subject: Your alo password reset code\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         Your alo password reset code is {code}.\r\n\
         \r\n\
         It expires in 10 minutes. If you did not ask to reset your alo password, you can ignore this email.\r\n"
    );
    crate::submission::submit(
        addr,
        &mail_from,
        std::slice::from_ref(&recovery.to_owned()),
        message.as_bytes(),
    )
    .await
    .map_err(|reason| {
        tracing::error!(reason = %reason, "reset: could not send the reset code");
        Problem::with(
            StatusCode::BAD_GATEWAY,
            "could not send the reset email — please try again",
        )
    })
}
