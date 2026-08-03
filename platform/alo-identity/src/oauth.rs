//! The OpenID Connect / OAuth 2.0 provider (RFC 6749 authorization-code +
//! RFC 7636 PKCE, RFC 8414 / OIDC discovery, RFC 7009 revocation). alo
//! as an identity provider: the endpoint set a Relying Party integrates
//! against. Access tokens are opaque (ADR 0008); the ID token is an EdDSA
//! JWT.
//!
//! The resource-owner login (password + optional TOTP) is submitted to
//! `POST /oauth/authorize`; a first-party web app renders the login page
//! and posts here. Only `S256` PKCE is accepted — no `plain`, no
//! challenge-less code.

use axum::Router;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use time::OffsetDateTime;

use alo_store::AuthCodeOutcome;

use crate::jwt::Claims;
use crate::keys::ALG_EDDSA;
use crate::secret::{self, Secret};
use crate::{Identity, SCOPE_EMAIL, SCOPE_OFFLINE, SCOPE_OPENID, SCOPE_PROFILE};

/// Builds the OAuth/OIDC provider router over an [`Identity`].
pub fn router(identity: Identity) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/oauth/jwks", get(jwks))
        .route("/oauth/authorize", post(authorize))
        .route("/oauth/token", post(token))
        .route("/oauth/userinfo", get(userinfo))
        .route("/oauth/revoke", post(revoke))
        .with_state(identity)
}

// ---- discovery + jwks ------------------------------------------------

async fn discovery(State(id): State<Identity>) -> Json<Value> {
    let cfg = id.config();
    Json(json!({
        "issuer": cfg.issuer,
        "authorization_endpoint": cfg.authorization_endpoint(),
        "token_endpoint": cfg.token_endpoint(),
        "userinfo_endpoint": cfg.userinfo_endpoint(),
        "jwks_uri": cfg.jwks_uri(),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "id_token_signing_alg_values_supported": [ALG_EDDSA],
        "subject_types_supported": ["public"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": [SCOPE_OPENID, SCOPE_EMAIL, SCOPE_PROFILE, SCOPE_OFFLINE],
        "claims_supported": [
            "sub", "iss", "aud", "exp", "iat", "nonce",
            "email", "email_verified", "preferred_username"
        ],
    }))
}

async fn jwks(State(id): State<Identity>) -> Response {
    match id.jwks().await {
        Ok(doc) => Json(doc).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ---- authorize -------------------------------------------------------

#[derive(Deserialize)]
struct AuthorizeForm {
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    response_type: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: String,
    nonce: Option<String>,
    username: String,
    password: String,
    otp: Option<String>,
}

async fn authorize(State(id): State<Identity>, Form(f): Form<AuthorizeForm>) -> Response {
    // 1. The client and redirect URI are validated *before* any redirect,
    // so we never bounce a credential or an error to an unvetted URI. A
    // store fault is a server_error, distinct from an unknown client.
    let client = match id.store().oauth_client(&f.client_id).await {
        Ok(Some(client)) => client,
        Ok(None) => {
            return oauth_error(StatusCode::BAD_REQUEST, "invalid_client", "unknown client");
        }
        Err(error) => {
            tracing::error!(%error, "oauth/authorize: client lookup failed");
            return server_error();
        }
    };
    if !client.redirect_uris.iter().any(|u| u == &f.redirect_uri) {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
    }

    // 2. From here, protocol errors redirect back per RFC 6749 §4.1.2.1.
    if f.response_type != "code" {
        return redirect_error(&f.redirect_uri, "unsupported_response_type", &f.state);
    }
    if f.code_challenge_method != "S256" || f.code_challenge.is_empty() {
        return redirect_error(&f.redirect_uri, "invalid_request", &f.state); // PKCE S256 required
    }
    let scope = normalize_scope(&f.scope);

    // 3. Authenticate the resource owner. A failure is a 401 (the user is
    // not authenticated, so we do NOT redirect to the RP) — with backoff.
    let rl_key = format!("{}|{}", f.client_id, f.username);
    if let Some(wait) = id.rate_limiter().retry_after(&rl_key) {
        return too_many_requests(wait.as_secs());
    }
    let principal = match id.authenticate_password(&f.username, &f.password).await {
        Ok(Some(principal)) => principal,
        Ok(None) => {
            id.rate_limiter().record_failure(&rl_key);
            // Login is personal data (Law 1): username at debug only.
            tracing::debug!(client = %f.client_id, user = %f.username, "oauth/authorize: bad credentials");
            tracing::info!(client = %f.client_id, "oauth/authorize: authentication failed");
            return oauth_error(
                StatusCode::UNAUTHORIZED,
                "access_denied",
                "invalid credentials",
            );
        }
        Err(error) => {
            // A store fault is not a credential rejection: don't penalize
            // the user with a rate-limit strike for our outage.
            tracing::error!(%error, "oauth/authorize: credential lookup failed");
            return server_error();
        }
    };
    // A tenant-scoped client may only be used by that tenant's users; a
    // deployment-wide client (NULL tenant) is usable by everyone.
    if let Some(client_tenant) = &client.tenant
        && client_tenant != &principal.tenant
    {
        return oauth_error(
            StatusCode::UNAUTHORIZED,
            "access_denied",
            "client not permitted for this account",
        );
    }
    // 4. Second factor, if enrolled.
    match id
        .check_second_factor(&principal.tenant, &principal.user, f.otp.as_deref())
        .await
    {
        Ok(crate::totp::TotpOutcome::Failed) => {
            id.rate_limiter().record_failure(&rl_key);
            return oauth_error(
                StatusCode::UNAUTHORIZED,
                "access_denied",
                "second factor required",
            );
        }
        Ok(_) => {}
        Err(_) => return oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "error"),
    }
    id.rate_limiter().record_success(&rl_key);

    // 5. Issue a single-use authorization code bound to the PKCE challenge.
    let Ok(code) = secret::random_token() else {
        return oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "error");
    };
    let code_hash = secret::hash_at_rest(code.reveal());
    let expires_at = OffsetDateTime::now_utc() + id.config().code_ttl;
    if id
        .store()
        .for_tenant(principal.tenant.clone())
        .issue_auth_code(
            &principal.user,
            &code_hash,
            &f.client_id,
            &f.redirect_uri,
            &f.code_challenge,
            &scope,
            f.nonce.as_deref(),
            expires_at,
        )
        .await
        .is_err()
    {
        return oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "error");
    }

    // 6. Redirect back with the code (and echoed state).
    let mut params = vec![("code", code.reveal().to_owned())];
    if !f.state.is_empty() {
        params.push(("state", f.state.clone()));
    }
    redirect_with(&f.redirect_uri, &params)
}

// ---- token -----------------------------------------------------------

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    // authorization_code grant
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    // refresh_token grant
    refresh_token: Option<String>,
}

async fn token(State(id): State<Identity>, Form(f): Form<TokenForm>) -> Response {
    match f.grant_type.as_str() {
        "authorization_code" => token_auth_code(&id, &f).await,
        "refresh_token" => token_refresh(&id, &f).await,
        _ => oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "unsupported grant_type",
        ),
    }
}

async fn token_auth_code(id: &Identity, f: &TokenForm) -> Response {
    let (Some(code), Some(client_id), Some(redirect_uri), Some(verifier)) = (
        f.code.as_deref(),
        f.client_id.as_deref(),
        f.redirect_uri.as_deref(),
        f.code_verifier.as_deref(),
    ) else {
        return invalid_grant("missing authorization_code parameters");
    };

    let code_hash = secret::hash_at_rest(code);
    let row = match id.store().consume_auth_code(&code_hash).await {
        Ok(AuthCodeOutcome::Valid(row)) => row,
        Ok(AuthCodeOutcome::Replayed {
            tenant,
            user,
            client_id: cid,
        }) => {
            // A reused code is a replay (RFC 6749 §10.4): revoke everything
            // minted for that (user, client) and refuse. If the revoke
            // itself fails we fail closed (server_error), never reporting a
            // safety action we did not complete.
            tracing::warn!(client = %cid, "oauth/token: authorization code replay detected; revoking token chain");
            if let Err(error) = id
                .store()
                .revoke_user_client_tokens(&tenant, &user, &cid)
                .await
            {
                tracing::error!(%error, "oauth/token: chain revocation on code replay FAILED");
                return server_error();
            }
            return invalid_grant("authorization code already used");
        }
        Ok(AuthCodeOutcome::NotFound) => return invalid_grant("unknown or expired code"),
        Err(_) => return server_error(),
    };

    if row.client_id != client_id || row.redirect_uri != redirect_uri {
        return invalid_grant("client or redirect_uri mismatch");
    }
    if !verify_pkce(verifier, &row.code_challenge) {
        return invalid_grant("PKCE verification failed");
    }

    issue_grant(
        id,
        &row.tenant,
        &row.user,
        client_id,
        &row.scope,
        row.nonce.as_deref(),
        true,
    )
    .await
}

async fn token_refresh(id: &Identity, f: &TokenForm) -> Response {
    let (Some(presented), Some(client_id)) = (f.refresh_token.as_deref(), f.client_id.as_deref())
    else {
        return invalid_grant("missing refresh_token parameters");
    };
    let hash = secret::hash_at_rest(presented);
    let row = match id.store().refresh_token(&hash).await {
        Ok(Some(row)) => row,
        Ok(None) => return invalid_grant("unknown refresh token"),
        Err(_) => return server_error(),
    };
    // Reuse of a spent (rotated) token is a replay → revoke the chain,
    // failing closed if the revoke itself errors.
    if row.rotated_to.is_some() {
        tracing::warn!(client = %row.client_id, "oauth/token: refresh-token replay detected; revoking token chain");
        if let Err(error) = id
            .store()
            .revoke_user_client_tokens(&row.tenant, &row.user, &row.client_id)
            .await
        {
            tracing::error!(%error, "oauth/token: chain revocation on refresh replay FAILED");
            return server_error();
        }
        return invalid_grant("refresh token already used");
    }
    if row.revoked_at.is_some() || row.expires_at <= OffsetDateTime::now_utc() {
        return invalid_grant("refresh token expired or revoked");
    }
    if row.client_id != client_id {
        return invalid_grant("client mismatch");
    }

    // Rotate the refresh token. The rotate is the atomic single-use gate:
    // if it reports the token was already spent (a concurrent replay the
    // fast check above missed), revoke the whole chain and refuse.
    let Ok(new_refresh) = secret::random_token() else {
        return server_error();
    };
    let new_hash = secret::hash_at_rest(new_refresh.reveal());
    let refresh_expires = OffsetDateTime::now_utc() + id.config().refresh_ttl;
    match id
        .store()
        .rotate_refresh_token(
            &hash,
            &new_hash,
            &row.tenant,
            &row.user,
            &row.client_id,
            &row.scope,
            refresh_expires,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(client = %row.client_id, "oauth/token: concurrent refresh-token reuse; revoking token chain");
            if let Err(error) = id
                .store()
                .revoke_user_client_tokens(&row.tenant, &row.user, &row.client_id)
                .await
            {
                tracing::error!(%error, "oauth/token: chain revocation on refresh race FAILED");
                return server_error();
            }
            return invalid_grant("refresh token already used");
        }
        Err(_) => return server_error(),
    }
    issue_access_and_id(
        id,
        &row.tenant,
        &row.user,
        client_id,
        &row.scope,
        None,
        Some(new_refresh),
    )
    .await
}

/// Issues an access token, optionally a refresh token, and (if `openid`
/// scope) an ID token, as a token-endpoint JSON response.
async fn issue_grant(
    id: &Identity,
    tenant: &alo_store::TenantId,
    user: &alo_store::UserId,
    client_id: &str,
    scope: &str,
    nonce: Option<&str>,
    with_refresh: bool,
) -> Response {
    let refresh = if with_refresh || scope_has(scope, SCOPE_OFFLINE) {
        match id.issue_refresh_token(tenant, user, client_id, scope).await {
            Ok(r) => Some(r),
            Err(_) => return server_error(),
        }
    } else {
        None
    };
    issue_access_and_id(id, tenant, user, client_id, scope, nonce, refresh).await
}

async fn issue_access_and_id(
    id: &Identity,
    tenant: &alo_store::TenantId,
    user: &alo_store::UserId,
    client_id: &str,
    scope: &str,
    nonce: Option<&str>,
    refresh: Option<Secret>,
) -> Response {
    let access = match id
        .issue_access_token(tenant, user, Some(client_id), scope)
        .await
    {
        Ok(t) => t,
        Err(_) => return server_error(),
    };

    let email = match id.store().for_tenant(tenant.clone()).email_of(user).await {
        Ok(email) => email,
        Err(_) => return server_error(),
    };

    let id_token = if scope_has(scope, SCOPE_OPENID) {
        let claims = Claims {
            sub: user.as_str(),
            aud: client_id,
            nonce,
            email: if scope_has(scope, SCOPE_EMAIL) {
                email.as_deref()
            } else {
                None
            },
            preferred_username: if scope_has(scope, SCOPE_PROFILE) {
                email.as_deref()
            } else {
                None
            },
        };
        match id.sign_id_token(&claims).await {
            Ok(t) => Some(t),
            Err(_) => return server_error(),
        }
    } else {
        None
    };

    let mut body = json!({
        "access_token": access.reveal(),
        "token_type": "Bearer",
        "expires_in": id.config().access_ttl.whole_seconds(),
        "scope": scope,
    });
    if let Some(r) = &refresh {
        body["refresh_token"] = json!(r.reveal());
    }
    if let Some(t) = &id_token {
        body["id_token"] = json!(t);
    }
    // No-store per RFC 6749 §5.1.
    (
        StatusCode::OK,
        [("Cache-Control", "no-store"), ("Pragma", "no-cache")],
        Json(body),
    )
        .into_response()
}

// ---- userinfo --------------------------------------------------------

async fn userinfo(State(id): State<Identity>, headers: HeaderMap) -> Response {
    let Some(token) = bearer(&headers) else {
        return unauthorized("missing bearer token");
    };
    let principal = match id.resolve_access_token(&token).await {
        Ok(Some(p)) => p,
        Ok(None) => return unauthorized("invalid token"),
        Err(_) => return server_error(),
    };
    let email = match id
        .store()
        .for_tenant(principal.tenant.clone())
        .email_of(&principal.user)
        .await
    {
        Ok(email) => email,
        Err(_) => return server_error(),
    };

    let mut claims = json!({ "sub": principal.user.as_str() });
    if scope_has(&principal.scope, SCOPE_EMAIL)
        && let Some(e) = &email
    {
        claims["email"] = json!(e);
        claims["email_verified"] = json!(true);
    }
    if scope_has(&principal.scope, SCOPE_PROFILE)
        && let Some(e) = &email
    {
        claims["preferred_username"] = json!(e);
    }
    Json(claims).into_response()
}

// ---- revoke ----------------------------------------------------------

#[derive(Deserialize)]
struct RevokeForm {
    token: String,
    #[serde(default)]
    token_type_hint: String,
}

async fn revoke(State(id): State<Identity>, Form(f): Form<RevokeForm>) -> Response {
    // RFC 7009: 200 whether or not the token was valid — but only once the
    // revoke has actually run. A store fault must NOT report success (that
    // would tell a user "logged out" when the token is still live); RFC 7009
    // §2.2.1 permits a 503 in that case so the client retries.
    let mut store_failed = false;
    if f.token_type_hint != "refresh_token"
        && let Err(error) = id.revoke_access_token(&f.token).await
    {
        tracing::warn!(%error, "oauth/revoke: access-token revocation failed");
        store_failed = true;
    }
    if f.token_type_hint != "access_token"
        && let Err(error) = id.revoke_refresh_token(&f.token).await
    {
        tracing::warn!(%error, "oauth/revoke: refresh-token revocation failed");
        store_failed = true;
    }
    if store_failed {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "temporarily_unavailable" })),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

// ---- helpers ---------------------------------------------------------

fn scope_has(scope: &str, want: &str) -> bool {
    scope.split_whitespace().any(|s| s == want)
}

/// Keeps only supported scopes and guarantees `openid` is present (this is
/// an OIDC provider). Order-preserving, deduplicated.
fn normalize_scope(requested: &str) -> String {
    let supported = [SCOPE_OPENID, SCOPE_EMAIL, SCOPE_PROFILE, SCOPE_OFFLINE];
    let mut out: Vec<&str> = vec![SCOPE_OPENID];
    for s in requested.split_whitespace() {
        if supported.contains(&s) && !out.contains(&s) {
            out.push(s);
        }
    }
    out.join(" ")
}

fn verify_pkce(verifier: &str, challenge: &str) -> bool {
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    secret::ct_eq(computed.as_bytes(), challenge.as_bytes())
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value.strip_prefix("Bearer ")?.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn oauth_error(status: StatusCode, code: &str, desc: &str) -> Response {
    (
        status,
        Json(json!({ "error": code, "error_description": desc })),
    )
        .into_response()
}

fn invalid_grant(desc: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", desc)
}

fn server_error() -> Response {
    oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "error")
}

/// A 429 with `Retry-After` (seconds) — the backoff response for the
/// credential endpoints.
fn too_many_requests(retry_after_secs: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("Retry-After", retry_after_secs.to_string())],
        Json(json!({
            "error": "temporarily_unavailable",
            "error_description": "too many attempts, retry later"
        })),
    )
        .into_response()
}

fn unauthorized(desc: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            "WWW-Authenticate",
            format!("Bearer error=\"invalid_token\", error_description=\"{desc}\""),
        )],
        Json(json!({ "error": "invalid_token", "error_description": desc })),
    )
        .into_response()
}

fn redirect_error(redirect_uri: &str, error: &str, state: &str) -> Response {
    let mut params = vec![("error", error.to_owned())];
    if !state.is_empty() {
        params.push(("state", state.to_owned()));
    }
    redirect_with(redirect_uri, &params)
}

fn redirect_with(base: &str, params: &[(&str, String)]) -> Response {
    let sep = if base.contains('?') { '&' } else { '?' };
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", pct_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    Redirect::to(&format!("{base}{sep}{query}")).into_response()
}

/// Percent-encode a query-parameter value (RFC 3986 unreserved set kept).
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            other => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}
