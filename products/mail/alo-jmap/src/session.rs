//! The Session resource (RFC 8620 §2): capabilities, accounts, URLs, and
//! the honest, enforced limits.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde_json::{Map, Value, json};

use crate::error::Problem;
use crate::state::{AppState, authenticate};

const CAP_CORE: &str = "urn:ietf:params:jmap:core";
const CAP_MAIL: &str = "urn:ietf:params:jmap:mail";
const CAP_SIEVE: &str = "urn:ietf:params:jmap:sieve";
const CAP_SUBMISSION: &str = "urn:ietf:params:jmap:submission";
/// alo extension: user-defined colored message categories (Category/get+set).
const CAP_CATEGORIES: &str = "urn:alo:params:jmap:categories";
/// The JMAP Contacts capability (RFC 9610): Contact/get+set.
const CAP_CONTACTS: &str = "urn:ietf:params:jmap:contacts";
const CAP_VACATION: &str = "urn:ietf:params:jmap:vacationresponse";
const CAP_QUOTA: &str = "urn:ietf:params:jmap:quota";

/// The origin the Session resource should advertise its URLs on.
///
/// **The host the client actually reached, not the one this service was
/// configured with.** One `alo-jmap` serves several front-ends — `mail.…` and
/// `app.aloworkplace.com` today — and Caddy proxies `/jmap/*` on each. Handing
/// every client the configured `ALO_JMAP_BASE_URL` told the workspace app that
/// its API lived on the mail host, which is a *different origin*: every request
/// it made was then blocked by its own `connect-src 'self'`, so the app loaded
/// and did nothing at all. RFC 8620 §2 makes these URLs the client's only route
/// to the API, so getting the origin wrong disables the whole session.
///
/// **The `Host` header is allowlisted rather than trusted.** It is
/// caller-controlled, and a value echoed into a URL a client will then call is
/// exactly the shape of an open redirect. Only a host this deployment already
/// serves is honoured; anything else falls back to the configured base, which is
/// the safe and previously-correct answer. The scheme is taken from the
/// configured base too — a deployment reached over HTTPS does not start
/// advertising `http://` because a header said so.
fn session_base(state: &AppState, headers: &HeaderMap) -> String {
    let configured = state.base_url.trim_end_matches('/');
    let Some(host) = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|h| !h.is_empty())
    else {
        return configured.to_owned();
    };
    let scheme = configured.split("://").next().unwrap_or("https");
    let configured_host = configured.split("://").nth(1).unwrap_or_default();
    if host.eq_ignore_ascii_case(configured_host) {
        return configured.to_owned();
    }
    if state
        .session_origins
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(host))
    {
        return format!("{scheme}://{host}");
    }
    configured.to_owned()
}

/// `GET /.well-known/jmap` → the Session resource.
pub async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let account_id = account.account_id().to_owned();
    let state_str = account
        .acc
        .state()
        .await
        .map_err(|_| Problem::server_error())?;
    // Whether AI is enabled for this tenant (ADR 0011), so the client shows or
    // hides AI affordances. A read failure degrades to "off", never an error.
    let ai_enabled = account
        .acc
        .default_ai_config()
        .await
        .ok()
        .flatten()
        .is_some_and(|c| c.enabled);
    // The addresses this user may send from — their canonical address plus any
    // aliases — so the compose UI can offer a From picker. The submission path
    // already authorizes sending from exactly this set (submission.rs). Best
    // effort: a read failure degrades to an empty list, and the client falls
    // back to the signed-in address.
    let ts = state.store.for_tenant(account.tenant.clone());
    let mut send_as: Vec<String> = Vec::new();
    if let Ok(Some(canonical)) = ts.email_of(&account.user).await {
        send_as.push(canonical);
    }
    if let Ok(aliases) = ts.aliases_of(&account.user).await {
        send_as.extend(aliases);
    }
    let l = &state.limits;
    let base = &session_base(&state, &headers);

    let mut accounts = Map::new();
    accounts.insert(
        account_id.clone(),
        json!({
            "name": account_id,
            "isPersonal": true,
            "isReadOnly": false,
            "accountCapabilities": {
                CAP_MAIL: {}, CAP_SIEVE: {}, CAP_SUBMISSION: {}, CAP_CATEGORIES: {}, CAP_CONTACTS: {}, CAP_VACATION: {}, CAP_QUOTA: {}
            }
        }),
    );
    // Shared mailboxes: accounts this user was delegated access to (ADR 0017),
    // advertised as non-personal accounts so the client offers them as shared
    // mailboxes. They stay writable (move/flag/delete); `alo:canSend` says
    // whether the delegate may also send as that address (submission enforces).
    if let Ok(delegations) = state
        .store
        .for_tenant(account.tenant.clone())
        .delegations_for(&account.user)
        .await
    {
        for (owner_id, owner_email, can_write, send_mode) in delegations {
            accounts.insert(
                owner_id,
                json!({
                    "name": owner_email,
                    "isPersonal": false,
                    "isReadOnly": !can_write,
                    "alo:canSend": send_mode != "none",
                    "accountCapabilities": {
                        CAP_MAIL: {}, CAP_SIEVE: {}, CAP_SUBMISSION: {}, CAP_CATEGORIES: {}, CAP_CONTACTS: {}, CAP_VACATION: {}, CAP_QUOTA: {}
                    }
                }),
            );
        }
    }
    let mut primary = Map::new();
    primary.insert(CAP_MAIL.to_owned(), json!(account_id));
    primary.insert(CAP_SUBMISSION.to_owned(), json!(account_id));

    Ok(Json(json!({
        "capabilities": {
            CAP_CORE: {
                "maxSizeUpload": l.max_size_upload,
                "maxConcurrentUpload": l.max_concurrent_upload,
                "maxSizeRequestObject": l.max_size_request,
                "maxConcurrentRequests": 8,
                "maxCallsInRequest": l.max_calls_in_request,
                "maxObjectsInGet": l.max_objects_in_get,
                "maxObjectsInSet": l.max_objects_in_set,
                "collationAlgorithms": ["i;ascii-casemap", "i;unicode-casemap"]
            },
            CAP_MAIL: {
                "maxMailboxesPerEmail": Value::Null,
                "maxMailboxDepth": Value::Null,
                "maxSizeMailboxName": 490,
                "maxSizeAttachmentsPerEmail": l.max_size_upload,
                "emailQuerySortOptions": ["receivedAt"],
                "mayCreateTopLevelMailbox": true
            },
            CAP_SIEVE: {
                "maxSizeScriptName": 512,
                "maxSizeScript": 65536,
                "maxNumberScripts": 100,
                "maxNumberRedirects": 3,
                "sieveExtensions": [
                    "fileinto", "envelope", "vacation", "subaddress", "imap4flags",
                    "comparator-i;ascii-numeric"
                ],
                "notificationMethods": Value::Null,
                "externalLists": Value::Null
            },
            CAP_SUBMISSION: {
                "maxDelayedSend": 0,
                "submissionExtensions": {}
            },
            CAP_CATEGORIES: {},
            CAP_CONTACTS: {},
            CAP_VACATION: {},
            CAP_QUOTA: {}
        },
        "accounts": accounts,
        "primaryAccounts": primary,
        "username": account_id,
        "apiUrl": format!("{base}/jmap/api"),
        "downloadUrl": format!("{base}/jmap/download/{{accountId}}/{{blobId}}/{{name}}"),
        "uploadUrl": format!("{base}/jmap/upload/{{accountId}}"),
        "eventSourceUrl": format!("{base}/jmap/eventsource?types={{types}}&closeafter={{closeafter}}&ping={{ping}}"),
        "state": state_str,
        // alo extension (additive): whether AI features are enabled for this
        // tenant, so the client shows or hides AI affordances (ADR 0011).
        "alo:aiEnabled": ai_enabled,
        // Whether the signed-in user is a tenant admin (gates the admin console).
        "alo:isAdmin": account.is_admin,
        // The tenant-wide scoped roles this user holds (ADR 0035, B4.12) —
        // today only `accountant`. Advertised so a client can show the surfaces
        // the role opens instead of offering every module and letting the
        // server refuse; the server refuses regardless, because a client is
        // never an access decision.
        "alo:roles": account.roles.iter().map(|role| role.as_str()).collect::<Vec<_>>(),
        // The rail modules a tenant admin has switched off for this person
        // (migration 0208). The client hides them, because offering an app
        // that answers 403 is worse than not offering it. As with the roles
        // above, this is advertised so the UI can be honest and is never the
        // decision: `module_access` refuses the routes regardless, and a
        // client is never an access decision.
        //
        // Empty for an admin even when rows exist — an admin is not denied —
        // so the console reads its switches from `/admin/users/modules`
        // instead, which reports what was stored rather than what applies.
        "alo:deniedModules": if account.is_admin {
            Vec::new()
        } else {
            account
                .denied_modules
                .iter()
                .map(|module| module.as_str())
                .collect::<Vec<_>>()
        },
        // The addresses this user may send from (canonical + aliases), for the
        // compose From picker. Authorized identically in the submission path.
        "alo:sendAs": send_as
    })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_host(host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::header::HOST,
            HeaderValue::from_str(host).unwrap(),
        );
        h
    }

    /// The base a session would advertise, without standing a server up.
    fn base_for(configured: &str, allowed: &[&str], host: Option<&str>) -> String {
        let headers = host.map_or_else(HeaderMap::new, headers_with_host);
        let configured = configured.trim_end_matches('/');
        let scheme = configured.split("://").next().unwrap_or("https");
        let configured_host = configured.split("://").nth(1).unwrap_or_default();
        let Some(host) = headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|h| !h.is_empty())
        else {
            return configured.to_owned();
        };
        if host.eq_ignore_ascii_case(configured_host) {
            return configured.to_owned();
        }
        if allowed.iter().any(|a| a.eq_ignore_ascii_case(host)) {
            return format!("{scheme}://{host}");
        }
        configured.to_owned()
    }

    #[test]
    fn a_client_is_told_the_origin_it_actually_reached() {
        // The bug this exists for: the workspace app was told its API lived on
        // the mail host, a different origin, and its own connect-src blocked
        // every call — the app loaded and did nothing.
        assert_eq!(
            base_for(
                "https://mail.alomails.com",
                &["app.aloworkplace.com"],
                Some("app.aloworkplace.com")
            ),
            "https://app.aloworkplace.com"
        );
        assert_eq!(
            base_for(
                "https://mail.alomails.com",
                &["app.aloworkplace.com"],
                Some("mail.alomails.com")
            ),
            "https://mail.alomails.com"
        );
    }

    #[test]
    fn a_host_nobody_allowed_is_ignored_rather_than_echoed() {
        // The Host header is caller-controlled. Echoing it into a URL the
        // client will then call is the shape of an open redirect, so anything
        // outside the allowlist falls back to the configured base.
        assert_eq!(
            base_for(
                "https://mail.alomails.com",
                &["app.aloworkplace.com"],
                Some("evil.example")
            ),
            "https://mail.alomails.com"
        );
        // And with no allowlist at all, the configured base is the only answer.
        assert_eq!(
            base_for(
                "https://mail.alomails.com",
                &[],
                Some("app.aloworkplace.com")
            ),
            "https://mail.alomails.com"
        );
    }

    #[test]
    fn the_scheme_comes_from_configuration_never_from_the_header() {
        // A deployment reached over HTTPS does not start advertising http://
        // because a header said so.
        let base = base_for(
            "https://mail.alomails.com",
            &["app.aloworkplace.com"],
            Some("app.aloworkplace.com"),
        );
        assert!(base.starts_with("https://"), "{base}");
    }

    #[test]
    fn an_absent_or_blank_host_falls_back_to_the_configured_base() {
        assert_eq!(
            base_for(
                "https://mail.alomails.com/",
                &["app.aloworkplace.com"],
                None
            ),
            "https://mail.alomails.com"
        );
        assert_eq!(
            base_for(
                "https://mail.alomails.com",
                &["app.aloworkplace.com"],
                Some("   ")
            ),
            "https://mail.alomails.com"
        );
    }
}
