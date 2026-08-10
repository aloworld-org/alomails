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
    let base = &state.base_url;

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
        // The addresses this user may send from (canonical + aliases), for the
        // compose From picker. Authorized identically in the submission path.
        "alo:sendAs": send_as
    })))
}
