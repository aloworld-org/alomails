//! The public half of a workspace invitation (migration 0209).
//!
//! Two unauthenticated endpoints. The person holding the link has no account
//! yet — that is the point — so the token is the only credential either of
//! them accepts:
//!
//! - `GET  /invite/{token}` — who the invitation is for, to show on the page.
//! - `POST /invite/{token}` — password + recovery address → the credential is
//!   installed and the link is spent.
//!
//! # One answer for unknown, spent and expired
//!
//! All three are `404` with the same sentence. Telling them apart would make
//! the endpoint an oracle: somebody with a list of guessed tokens could learn
//! which ones had ever been issued, and "expired" additionally says a real
//! person at a real address was invited to this workspace.
//!
//! # Why the recovery address is required here
//!
//! `/reset/*` proves control of the recovery mailbox captured at signup, and
//! an admin-created account never had one. Acceptance is the only moment the
//! person is present, authenticated by the token, and able to name an address
//! that is not the mailbox they would otherwise be locked out of. Making it
//! optional would recreate exactly the hole this feature closes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, body::Bytes};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::state::AppState;

/// The shortest password an invited person may choose. The same floor
/// `create_user` applies, so the two paths cannot disagree about what counts
/// as a password.
const MIN_PASSWORD: usize = 8;

fn gone() -> Problem {
    Problem::with(
        StatusCode::NOT_FOUND,
        "This invitation has expired or has already been used.",
    )
}

/// `GET /invite/{token}` — the facts the setup screen needs, and nothing else.
///
/// # Errors
/// `404` when the token is unknown, spent or expired.
pub async fn get_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<Value>, Problem> {
    let token_hash = alo_identity::secret::hash_at_rest(&token);
    let invitation = state
        .store
        .invites()
        .invite(&token_hash)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(gone)?;
    // The address only. Not the tenant name, not who invited them, not which
    // apps they have — a token in a mailbox should reveal as little about the
    // workspace as it can while still letting somebody recognise themselves.
    Ok(Json(json!({ "email": invitation.email })))
}

/// `POST /invite/{token}` — sets the password, records the recovery address,
/// spends the link. Body `{ password, recoveryEmail }`.
///
/// # Errors
/// `422` for a short password or a malformed recovery address; `404` when the
/// token is unknown, spent or expired.
pub async fn accept_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    let v: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let password = v.get("password").and_then(Value::as_str).unwrap_or("");
    let recovery = v
        .get("recoveryEmail")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if password.len() < MIN_PASSWORD {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Choose a password of at least 8 characters.",
        ));
    }
    if !recovery.contains('@') {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Enter an address you can read from somewhere else, so you can get back in.",
        ));
    }
    let accepted = state
        .identity
        .accept_user_invite(&token, password, &recovery)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(gone)?;
    // The address they now sign in with, so the page can send them to the
    // login form with it already filled in.
    Ok(Json(json!({ "email": accepted.email })))
}
