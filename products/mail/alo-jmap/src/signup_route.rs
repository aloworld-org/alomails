//! Public self-service signup surface (ADR 0018, slice 3).
//!
//! Three unauthenticated endpoints let a person claim a personal address on a
//! platform-operated domain, verify ownership of a recovery mailbox, and have
//! their account provisioned:
//!
//! - `POST /signup/available` — is this address claimable?
//! - `POST /signup/begin` — reserve it pending; email a code to the recovery
//!   mailbox.
//! - `POST /signup/verify` — code + password → [`Identity::provision_personal`].
//!
//! Defences: only configured personal domains are offered; RFC 2142 / reserved
//! localparts are refused ([`normalize_localpart`]); provisioning happens only
//! after verification (an unverified attempt creates no tenant); the code is
//! stored only as a salted SHA-256 hash and compared in constant time, with a
//! per-address attempt cap and a short expiry; and both the recovery address
//! and the client IP are rate-limited. The verification code and recovery
//! address are never logged (Law 1).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_identity::secret;
use alo_identity::signup::{SignupError, normalize_localpart};

use crate::error::Problem;
use crate::state::AppState;

/// How long a verification code is valid.
const CODE_TTL_SECS: i64 = 600;
/// Verify attempts allowed before the pending signup is burned.
const MAX_VERIFY_ATTEMPTS: i32 = 6;
/// Minimum password length for a personal account.
const MIN_PASSWORD: usize = 8;

#[derive(Deserialize)]
struct AvailableRequest {
    address: String,
}

#[derive(Deserialize)]
struct BeginRequest {
    address: String,
    #[serde(rename = "recoveryEmail")]
    recovery_email: String,
}

#[derive(Deserialize)]
struct VerifyRequest {
    address: String,
    code: String,
    password: String,
}

/// `GET /signup/domains` → `{domains: [...]}` — the domains open to personal
/// signup. Empty means the surface is disabled; the web page reads this to
/// show the address suffix and to hide signup when it is off.
pub async fn domains(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "domains": state.personal_domains }))
}

/// `POST /signup/available` → `{available, reason}`. `reason` is a stable
/// machine string the web UI localises (`ok`/`invalid`/`reserved`/
/// `unavailable_domain`/`taken`).
pub async fn available(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: AvailableRequest = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let reason = availability(&state, &req.address).await?;
    Ok(Json(
        json!({ "available": reason == "ok", "reason": reason }),
    ))
}

/// `POST /signup/begin` → `{status:"sent"}`. Reserves the address as pending
/// and emails a fresh code to the recovery mailbox. Rate-limited per client IP
/// and per recovery address.
pub async fn begin(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: BeginRequest = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let (localpart, domain) = parse_personal(&state, &req.address)?;
    let address = format!("{localpart}@{domain}");
    let recovery = req.recovery_email.trim().to_owned();
    if !looks_like_email(&recovery) {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "a valid recovery email is required",
        ));
    }

    // Throttle both the client and the target recovery mailbox.
    let ip = client_ip(&headers);
    for key in [format!("signup-ip|{ip}"), format!("signup-rcpt|{recovery}")] {
        if state.signup_limiter.retry_after(&key).is_some() {
            return Err(Problem::with(
                StatusCode::TOO_MANY_REQUESTS,
                "too many signup attempts — please wait a little and try again",
            ));
        }
    }
    state
        .signup_limiter
        .record_failure(&format!("signup-ip|{ip}"));
    state
        .signup_limiter
        .record_failure(&format!("signup-rcpt|{recovery}"));

    // Only offer a still-claimable address.
    match availability(&state, &address).await?.as_str() {
        "ok" => {}
        "taken" => {
            return Err(Problem::with(
                StatusCode::CONFLICT,
                "that address is already taken",
            ));
        }
        "reserved" => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "that address is reserved",
            ));
        }
        _ => {
            return Err(Problem::with(
                StatusCode::BAD_REQUEST,
                "that address is not available",
            ));
        }
    }

    // Keep the table bounded; best-effort.
    if let Err(error) = state.store.reap_expired_signups().await {
        tracing::warn!(%error, "signup: reap failed");
    }

    let code = generate_code();
    let code_hash = secret::hash_at_rest(&salt(&address, &code));
    state
        .store
        .upsert_pending_signup(&address, &recovery, &code_hash, CODE_TTL_SECS)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "signup: could not store pending signup");
            Problem::server_error()
        })?;

    send_code(&state, &domain, &recovery, &code).await?;
    Ok(Json(json!({ "status": "sent" })))
}

/// `POST /signup/verify` → `{accountId, email}`. Checks the code, then
/// provisions the account and clears the pending signup.
pub async fn verify(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, Problem> {
    let req: VerifyRequest = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let (localpart, domain) = parse_personal(&state, &req.address)?;
    let address = format!("{localpart}@{domain}");
    if req.password.len() < MIN_PASSWORD {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "the password is too short",
        ));
    }

    let pending = state
        .store
        .pending_signup(&address)
        .await
        .map_err(|_| Problem::server_error())?
        .ok_or_else(|| {
            Problem::with(
                StatusCode::BAD_REQUEST,
                "no pending signup for that address — start again",
            )
        })?;

    // Count the attempt first; burn the pending signup once the cap is hit so a
    // short code cannot be ground down.
    let attempts = state
        .store
        .bump_signup_attempts(&address)
        .await
        .map_err(|_| Problem::server_error())?;
    if attempts > MAX_VERIFY_ATTEMPTS {
        let _ = state.store.delete_pending_signup(&address).await;
        return Err(Problem::with(
            StatusCode::TOO_MANY_REQUESTS,
            "too many attempts — please start the signup again",
        ));
    }

    let given = secret::hash_at_rest(&salt(&address, req.code.trim()));
    if !secret::ct_eq(given.as_bytes(), pending.code_hash.as_bytes()) {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that code is not correct",
        ));
    }

    match state
        .identity
        .provision_personal(&domain, &localpart, &req.password)
        .await
    {
        Ok(account) => {
            let _ = state.store.delete_pending_signup(&address).await;
            state
                .signup_limiter
                .record_success(&format!("signup-rcpt|{}", pending.recovery_email));
            Ok(Json(json!({
                "accountId": account.user.as_str(),
                "email": account.email,
            })))
        }
        Err(SignupError::AddressTaken) => {
            let _ = state.store.delete_pending_signup(&address).await;
            Err(Problem::with(
                StatusCode::CONFLICT,
                "that address was just taken",
            ))
        }
        Err(SignupError::Reserved) => Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that address is reserved",
        )),
        Err(SignupError::InvalidAddress) => Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that address is not valid",
        )),
        Err(SignupError::Internal) => Err(Problem::server_error()),
    }
}

/// Computes an availability reason for `address`: `ok`, `invalid`, `reserved`,
/// `unavailable_domain`, or `taken`.
async fn availability(state: &AppState, address: &str) -> Result<String, Problem> {
    let Some((local, domain)) = split_address(address) else {
        return Ok("invalid".to_owned());
    };
    if !is_personal_domain(state, &domain) {
        return Ok("unavailable_domain".to_owned());
    }
    match normalize_localpart(&local) {
        Ok(_) => {}
        Err(SignupError::Reserved) => return Ok("reserved".to_owned()),
        Err(_) => return Ok("invalid".to_owned()),
    }
    let full = format!("{local}@{domain}");
    let taken = state
        .store
        .account_by_email(&full)
        .await
        .map_err(|_| Problem::server_error())?
        .is_some()
        || state
            .store
            .pending_signup(&full)
            .await
            .map_err(|_| Problem::server_error())?
            .is_some();
    Ok(if taken { "taken" } else { "ok" }.to_owned())
}

/// Validates that `address` is a well-formed, valid, non-reserved localpart on
/// a configured personal domain, returning `(localpart, domain)`.
fn parse_personal(state: &AppState, address: &str) -> Result<(String, String), Problem> {
    let Some((local, domain)) = split_address(address) else {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that address is not valid",
        ));
    };
    if !is_personal_domain(state, &domain) {
        return Err(Problem::with(
            StatusCode::BAD_REQUEST,
            "that domain is not offered for personal signup",
        ));
    }
    let local = normalize_localpart(&local).map_err(|e| match e {
        SignupError::Reserved => Problem::with(StatusCode::BAD_REQUEST, "that address is reserved"),
        _ => Problem::with(StatusCode::BAD_REQUEST, "that address is not valid"),
    })?;
    Ok((local, domain))
}

fn is_personal_domain(state: &AppState, domain: &str) -> bool {
    state.personal_domains.iter().any(|d| d == domain)
}

/// Splits `local@domain`, lowercased. `None` if malformed.
fn split_address(address: &str) -> Option<(String, String)> {
    let addr = address.trim().to_ascii_lowercase();
    let (local, domain) = addr.rsplit_once('@')?;
    if local.is_empty() || !domain.contains('.') {
        return None;
    }
    Some((local.to_owned(), domain.to_owned()))
}

/// A minimal, permissive check that `s` is a plausible `local@domain`.
fn looks_like_email(s: &str) -> bool {
    match s.rsplit_once('@') {
        Some((l, d)) => !l.is_empty() && d.contains('.') && !s.contains(char::is_whitespace),
        None => false,
    }
}

/// Salts the code with the address so a stolen `code_hash` cannot be matched
/// against a precomputed table of 6-digit codes.
fn salt(address: &str, code: &str) -> String {
    format!("{address}:{code}")
}

/// A random 6-digit numeric code.
fn generate_code() -> String {
    let mut buf = [0u8; 4];
    // A fill failure is astronomically unlikely; fall back to a fixed value
    // that is still gated by the attempt cap + expiry rather than panicking.
    if secret::random_bytes(&mut buf).is_err() {
        return "000000".to_owned();
    }
    let n = u32::from_le_bytes(buf) % 1_000_000;
    format!("{n:06}")
}

/// The client IP for rate-limiting, from the proxy's forwarding headers.
fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Sends the verification code to `recovery` from a system address on the
/// personal domain, through the trusted internal submission listener (which
/// adds Date/Message-ID and DKIM-signs). The code is never logged.
async fn send_code(
    state: &AppState,
    domain: &str,
    recovery: &str,
    code: &str,
) -> Result<(), Problem> {
    let Some(addr) = state.submission_addr.as_deref() else {
        tracing::error!("signup: no submission listener configured");
        return Err(Problem::with(
            StatusCode::SERVICE_UNAVAILABLE,
            "signup email is not available right now",
        ));
    };
    let mail_from = format!("noreply@{domain}");
    let message = format!(
        "From: alo <noreply@{domain}>\r\n\
         To: {recovery}\r\n\
         Subject: Your alo verification code\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         Your alo verification code is {code}.\r\n\
         \r\n\
         It expires in 10 minutes. If you did not request an alo account, you can ignore this email.\r\n"
    );
    crate::submission::submit(
        addr,
        &mail_from,
        std::slice::from_ref(&recovery.to_owned()),
        message.as_bytes(),
    )
    .await
    .map_err(|reason| {
        tracing::error!(reason = %reason, "signup: could not send verification code");
        Problem::with(
            StatusCode::BAD_GATEWAY,
            "could not send the verification email — please try again",
        )
    })
}
