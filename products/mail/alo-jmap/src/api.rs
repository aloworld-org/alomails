//! The JMAP API endpoint (RFC 8620 §3): the Request/Response envelope,
//! ordered method dispatch, result references, and the Mailbox/Email/
//! Thread methods mapped onto the tenant-scoped store.

use alo_store::{
    BlobId, CategoryId, Contact, ContactField, ContactId, EmailFilter, EmailQuery, MailboxId,
    MessageId, Page, SortDirection, StoreError, ThreadId, UserId,
};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::{Json, body::Bytes};
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{Problem, method_error, method_error_desc};
use crate::jtypes;
use crate::push::StateChangeMsg;
use crate::state::{Account, AppState, authenticate};

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

/// `POST /jmap/api` — process a JMAP Request, return the Response.
pub async fn api(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, Problem> {
    if body.len() > state.limits.max_size_request {
        return Err(Problem::too_large());
    }
    let account = authenticate(&state, &headers).await?;

    let request: Value = serde_json::from_slice(&body).map_err(|_| Problem::not_json())?;
    let obj = request.as_object().ok_or_else(Problem::not_request)?;

    // `using` must list only capabilities we support.
    let using = obj
        .get("using")
        .and_then(Value::as_array)
        .ok_or_else(Problem::not_request)?;
    for cap in using {
        match cap.as_str() {
            Some(CAP_CORE) | Some(CAP_MAIL) | Some(CAP_SIEVE) | Some(CAP_SUBMISSION)
            | Some(CAP_CATEGORIES) | Some(CAP_CONTACTS) | Some(CAP_VACATION) | Some(CAP_QUOTA) => {}
            other => {
                return Err(Problem::unknown_capability().detail(other.unwrap_or("").to_owned()));
            }
        }
    }

    let method_calls = obj
        .get("methodCalls")
        .and_then(Value::as_array)
        .ok_or_else(Problem::not_request)?;
    if method_calls.len() > state.limits.max_calls_in_request {
        return Err(Problem::limit("too many method calls"));
    }

    let state_before = account.acc.state().await.unwrap_or_default();
    let mut responses: Vec<Value> = Vec::new();
    // Delegated mailboxes (owner account ids) this request mutated — their
    // streams are notified after the loop so shared mailboxes update live.
    let mut touched_owners: std::collections::HashSet<String> = std::collections::HashSet::new();

    for call in method_calls {
        let (name, mut args, call_id) = match parse_invocation(call) {
            Some(triple) => triple,
            None => return Err(Problem::not_request()),
        };
        if let Err(reason) = resolve_references(&mut args, &responses) {
            responses.push(json!([
                "error",
                method_error_desc("invalidResultReference", &reason),
                call_id
            ]));
            continue;
        }
        // Which account does this call target — the signed-in user's own, or a
        // mailbox they were delegated (ADR 0017)? A foreign/ungranted accountId
        // resolves to None, so `check_account` reports the usual accountNotFound.
        let target = match args.get("accountId").and_then(Value::as_str) {
            Some(id) => crate::state::resolve_target(&account, &state, id).await,
            None => None,
        };
        let acct = target.as_ref().unwrap_or(&account);
        // Read-only delegates may read (…/get, …/query, …/changes) but not
        // mutate. A …/set on a read-only delegated mailbox is accountReadOnly.
        if let Some(d) = &acct.delegated
            && !d.can_write
            && name.ends_with("/set")
        {
            responses.push(json!(["error", method_error("accountReadOnly"), call_id]));
            continue;
        }
        // A successful mutation on a delegated mailbox must notify that owner's
        // stream (the owner and every other delegate), not the signed-in user's.
        let mutated = name.ends_with("/set");
        match dispatch(acct, &state, &name, &args).await {
            Ok(result) => {
                if mutated && acct.delegated.is_some() {
                    touched_owners.insert(acct.account_id().to_owned());
                }
                responses.push(json!([name, result, call_id]));
            }
            Err(err) => responses.push(json!(["error", err, call_id])),
        }
    }

    // Push: notify this account's stream if its own state changed…
    let session_state = account
        .acc
        .state()
        .await
        .unwrap_or_else(|_| state_before.clone());
    let change_types = vec![
        alo_store::changes::TYPE_MAILBOX,
        alo_store::changes::TYPE_EMAIL,
        alo_store::changes::TYPE_THREAD,
    ];
    if session_state != state_before {
        state.push.publish(
            account.tenant.as_str(),
            StateChangeMsg {
                account_id: account.account_id().to_owned(),
                types: change_types.clone(),
                state: session_state.clone(),
            },
        );
    }
    // …and each shared mailbox this request mutated, so its owner and every
    // connected delegate refresh in real time (ADR 0017).
    for owner_id in touched_owners {
        let owner_acc = state
            .store
            .for_account(account.tenant.clone(), UserId::new(owner_id.clone()));
        if let Ok(owner_state) = owner_acc.state().await {
            state.push.publish(
                account.tenant.as_str(),
                StateChangeMsg {
                    account_id: owner_id,
                    types: change_types.clone(),
                    state: owner_state,
                },
            );
        }
    }

    Ok(Json(
        json!({ "methodResponses": responses, "sessionState": session_state }),
    ))
}

/// Splits a method call `[name, args, callId]`.
fn parse_invocation(call: &Value) -> Option<(String, Value, Value)> {
    let arr = call.as_array()?;
    if arr.len() != 3 {
        return None;
    }
    Some((arr[0].as_str()?.to_owned(), arr[1].clone(), arr[2].clone()))
}

/// Resolves `#name` result references in an args object (RFC 8620 §3.7),
/// supporting plain JSON pointers and the `/*/prop` array-map form.
fn resolve_references(args: &mut Value, responses: &[Value]) -> Result<(), String> {
    let Some(obj) = args.as_object_mut() else {
        return Ok(());
    };
    let refs: Vec<String> = obj.keys().filter(|k| k.starts_with('#')).cloned().collect();
    for hashed in refs {
        let reference = obj.remove(&hashed).unwrap_or(Value::Null);
        let target = &hashed[1..];
        let result_of = reference.get("resultOf").and_then(Value::as_str);
        let name = reference.get("name").and_then(Value::as_str);
        let path = reference.get("path").and_then(Value::as_str);
        let (Some(result_of), Some(name), Some(path)) = (result_of, name, path) else {
            return Err(format!("malformed ResultReference for {target}"));
        };
        // Find the referenced prior response (matching callId + method).
        let source = responses.iter().rev().find_map(|r| {
            let a = r.as_array()?;
            if a.first()?.as_str()? == name && a.get(2)?.as_str()? == result_of {
                Some(a.get(1)?.clone())
            } else {
                None
            }
        });
        let source = source.ok_or_else(|| format!("no result for {result_of}/{name}"))?;
        let value = eval_path(&source, path).ok_or_else(|| format!("path {path} not found"))?;
        obj.insert(target.to_owned(), value);
    }
    Ok(())
}

/// Evaluates a JMAP reference path: a JSON pointer, optionally with one
/// `/*/` mapping an array element property.
fn eval_path(value: &Value, path: &str) -> Option<Value> {
    if let Some((prefix, suffix)) = path.split_once("/*/") {
        let array = value.pointer(prefix)?.as_array()?;
        let collected: Vec<Value> = array
            .iter()
            .filter_map(|el| el.pointer(&format!("/{suffix}")).cloned())
            .collect();
        return Some(Value::Array(collected));
    }
    value.pointer(path).cloned()
}

async fn dispatch(
    account: &Account,
    state: &AppState,
    name: &str,
    args: &Value,
) -> Result<Value, Value> {
    match name {
        "Core/echo" => Ok(args.clone()),
        "Mailbox/get" => mailbox_get(account, args).await,
        "Mailbox/set" => mailbox_set(account, args).await,
        "Mailbox/changes" => changes(account, args, alo_store::changes::TYPE_MAILBOX, state).await,
        "Category/get" => category_get(account, args).await,
        "Category/set" => category_set(account, args).await,
        "Contact/get" => contact_get(account, args).await,
        "Contact/set" => contact_set(account, args).await,
        "Email/get" => email_get(account, args, state).await,
        "Email/query" => email_query(account, args).await,
        "Email/set" => email_set(account, args, state).await,
        "Email/changes" => changes(account, args, alo_store::changes::TYPE_EMAIL, state).await,
        "SearchSnippet/get" => search_snippet_get(account, args, state).await,
        "Thread/get" => thread_get(account, args).await,
        "Identity/get" => identity_get(account, args, state).await,
        "VacationResponse/get" => vacation_get(account, args).await,
        "VacationResponse/set" => vacation_set(account, args).await,
        "Quota/get" => quota_get(account, args).await,
        "SieveScript/get" => crate::sieve::get(account, args).await,
        "SieveScript/set" => crate::sieve::set(account, args).await,
        "SieveScript/validate" => crate::sieve::validate(account, args).await,
        "EmailSubmission/set" => crate::submission::set(account, args, state).await,
        _ => Err(method_error("unknownMethod")),
    }
}

// ---- account guard + helpers ------------------------------------------

pub(crate) fn check_account(args: &Value, account: &Account) -> Result<(), Value> {
    match args.get("accountId").and_then(Value::as_str) {
        Some(id) if id == account.account_id() => Ok(()),
        _ => Err(method_error("accountNotFound")),
    }
}

/// Maps a store failure to the JMAP `serverFail` the client sees — and says so
/// in the log on the way past.
///
/// The error used to be dropped here, which is how a reading pane could show
/// "could not load messages" while the server's log stayed completely silent:
/// nothing to search for, nothing to alert on, and no way to tell a missing
/// message body from a database that had gone away. `serverFail` carries no
/// detail to the client by design — so the detail has to land somewhere, and
/// this is the only place it still exists.
///
/// `log_cause` is what makes that safe to write down; see its documentation
/// for what is summarised and why.
fn store_err(error: StoreError) -> Value {
    tracing::warn!(cause = %error.log_cause(), "store failure returned as serverFail");
    method_error("serverFail")
}

/// Maps a store error on a per-object mutation to a JMAP SetError.
fn set_error(e: &StoreError) -> Value {
    match e {
        StoreError::NotFound => method_error("notFound"),
        StoreError::Conflict(msg) => method_error_desc("invalidProperties", msg),
        StoreError::TooLarge { .. } => method_error("tooLarge"),
        StoreError::OverQuota => method_error("overQuota"),
        _ => method_error("serverFail"),
    }
}

/// A "#rrggbb" hex color (exactly 7 chars). Colors are rendered into CSS on the
/// client, so we accept nothing but a strict hex triple — never arbitrary text.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse a create-time `color` property: absent or null → no color; a valid
/// "#rrggbb" → that color; anything else → an `invalidProperties` error.
fn parse_color(v: Option<&Value>) -> Result<Option<String>, Value> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if is_hex_color(s) => Ok(Some(s.clone())),
        _ => Err(method_error_desc(
            "invalidProperties",
            "color must be #rrggbb",
        )),
    }
}

// ---- per-folder delegation enforcement (ADR 0017) ---------------------

/// Whether the account may see/act on message `mid`. Owners and whole-mailbox
/// delegates always pass; a folder-restricted delegate passes only when the
/// message lives in a granted folder. Fails closed on a store error.
async fn message_folder_allowed(account: &Account, mid: &MessageId) -> bool {
    let Some(d) = &account.delegated else {
        return true;
    };
    if d.folders.is_none() {
        return true;
    }
    match account.acc.mailboxes_of_message(mid).await {
        Ok(boxes) => d.any_folder_allowed(boxes.iter().map(|b| b.to_string())),
        Err(_) => false,
    }
}

/// Whether every folder an Email/set patch would move a message **into** is
/// granted — covers both the full `mailboxIds` object and `mailboxIds/<id>`
/// patch keys. True for unrestricted grants.
fn patch_dest_allowed(d: &crate::state::Delegation, patch: &Value) -> bool {
    let Some(obj) = patch.as_object() else {
        return true;
    };
    if let Some(dest) = obj.get("mailboxIds").and_then(Value::as_object) {
        for (mb, on) in dest {
            if on.as_bool().unwrap_or(false) && !d.folder_allowed(mb) {
                return false;
            }
        }
    }
    for (key, value) in obj {
        if let Some(mb) = key.strip_prefix("mailboxIds/") {
            let on = value.as_bool().unwrap_or(!value.is_null());
            if on && !d.folder_allowed(mb) {
                return false;
            }
        }
    }
    true
}

// ---- Mailbox ----------------------------------------------------------

async fn mailbox_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let state = account.acc.state().await.map_err(store_err)?;
    let ids = args.get("ids");

    let mut list = Vec::new();
    let mut not_found = Vec::new();
    if ids.is_none() || ids == Some(&Value::Null) {
        // All the account's mailboxes.
        let boxes = account
            .acc
            .mailboxes(Page::first(alo_store::MAX_PAGE))
            .await
            .map_err(store_err)?;
        for m in &boxes {
            // A folder-restricted delegate (ADR 0017) sees only granted folders;
            // every other folder is omitted, exactly as if it did not exist.
            if let Some(d) = &account.delegated
                && !d.folder_allowed(m.id.as_str())
            {
                continue;
            }
            list.push(jtypes::mailbox_json(m));
        }
    } else {
        for id in ids.and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = id.as_str() else { continue };
            let mid = MailboxId::new(id);
            // The account door is the scope: a foreign mailbox is NotFound.
            match account.acc.mailbox(&mid).await {
                // A folder outside a restricted grant is NotFound too — no oracle.
                Ok(m)
                    if account
                        .delegated
                        .as_ref()
                        .is_some_and(|d| !d.folder_allowed(m.id.as_str())) =>
                {
                    not_found.push(json!(id));
                }
                Ok(m) => list.push(jtypes::mailbox_json(&m)),
                Err(StoreError::NotFound) => not_found.push(json!(id)),
                Err(e) => return Err(store_err(e)),
            }
        }
    }
    Ok(
        json!({ "accountId": account.account_id(), "state": state, "list": list, "notFound": not_found }),
    )
}

async fn mailbox_set(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    // A folder-restricted delegate (ADR 0017) may work within its granted
    // folders but not restructure the owner's mailbox (create/rename/delete).
    if account
        .delegated
        .as_ref()
        .is_some_and(|d| d.folders.is_some())
    {
        return Err(method_error("accountReadOnly"));
    }
    let old_state = account.acc.state().await.map_err(store_err)?;
    if let Some(expected) = args.get("ifInState").and_then(Value::as_str)
        && expected != old_state
    {
        return Err(method_error("stateMismatch"));
    }

    let (mut created, mut not_created) = (Map::new(), Map::new());
    let (mut updated, mut not_updated) = (Map::new(), Map::new());
    let (mut destroyed, mut not_destroyed) = (Vec::new(), Map::new());

    // create
    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for (cid, props) in creates {
            let name = props.get("name").and_then(Value::as_str).unwrap_or("");
            if name.is_empty() {
                not_created.insert(
                    cid.clone(),
                    method_error_desc("invalidProperties", "name required"),
                );
                continue;
            }
            let parent = props
                .get("parentId")
                .and_then(Value::as_str)
                .map(MailboxId::new);
            // A foreign parent is rejected by create_mailbox itself
            // (the account door scopes it) — no separate guard to forget.
            let role = props.get("role").and_then(Value::as_str);
            // An invalid color is rejected before the mailbox is created, so we
            // never make a folder we then can't color as asked.
            let color = match parse_color(props.get("color")) {
                Ok(c) => c,
                Err(e) => {
                    not_created.insert(cid.clone(), e);
                    continue;
                }
            };
            match account
                .acc
                .create_mailbox(parent.as_ref(), name, role)
                .await
            {
                Ok(id) => {
                    if let Some(color) = color {
                        let _ = account.acc.set_mailbox_color(&id, Some(&color)).await;
                    }
                    created.insert(cid.clone(), json!({ "id": id.as_str() }));
                }
                Err(e) => {
                    not_created.insert(cid.clone(), set_error(&e));
                }
            }
        }
    }

    // update (name / parentId)
    if let Some(updates) = args.get("update").and_then(Value::as_object) {
        for (id, patch) in updates {
            let mailbox = MailboxId::new(id.as_str());
            // Account-scoped existence check: a foreign mailbox is
            // NotFound, so an empty patch cannot report a spurious success.
            if let Err(e) = account.acc.mailbox(&mailbox).await {
                not_updated.insert(id.clone(), set_error(&e));
                continue;
            }
            let mut result: Result<(), StoreError> = Ok(());
            if let Some(name) = patch.get("name").and_then(Value::as_str) {
                result = account.acc.rename_mailbox(&mailbox, name).await;
            }
            if result.is_ok() && patch.get("parentId").is_some() {
                let parent = patch
                    .get("parentId")
                    .and_then(Value::as_str)
                    .map(MailboxId::new);
                result = account.acc.move_mailbox(&mailbox, parent.as_ref()).await;
            }
            // color: a "#rrggbb" string sets it, an explicit null clears it,
            // anything else is invalidProperties.
            if result.is_ok()
                && let Some(color) = patch.get("color")
            {
                if color.is_null() {
                    result = account.acc.set_mailbox_color(&mailbox, None).await;
                } else if let Some(hex) = color.as_str().filter(|s| is_hex_color(s)) {
                    result = account.acc.set_mailbox_color(&mailbox, Some(hex)).await;
                } else {
                    not_updated.insert(
                        id.clone(),
                        method_error_desc("invalidProperties", "color must be #rrggbb"),
                    );
                    continue;
                }
            }
            match result {
                Ok(()) => {
                    updated.insert(id.clone(), Value::Null);
                }
                Err(e) => {
                    not_updated.insert(id.clone(), set_error(&e));
                }
            }
        }
    }

    // destroy
    if let Some(ids) = args.get("destroy").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            let mbox = MailboxId::new(id);
            // destroy_mailbox is account-scoped: a foreign mailbox is
            // NotFound → notDestroyed, no separate guard to forget.
            match account.acc.destroy_mailbox(&mbox).await {
                Ok(()) => destroyed.push(json!(id)),
                Err(e) => {
                    not_destroyed.insert(id.to_owned(), set_error(&e));
                }
            }
        }
    }

    let new_state = account.acc.state().await.map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(), "oldState": old_state, "newState": new_state,
        "created": created, "updated": updated, "destroyed": destroyed,
        "notCreated": not_created, "notUpdated": not_updated, "notDestroyed": not_destroyed
    }))
}

// ---- Category (alo extension) -----------------------------------------
// A user-defined colored label. Membership lives in the message's
// `$category_<id>` keyword (set/cleared/filtered via the standard Email
// methods); these methods only manage the catalog of name + color.

async fn category_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let state = account.acc.state().await.map_err(store_err)?;
    let all = account.acc.categories().await.map_err(store_err)?;

    let ids = args.get("ids");
    let mut list = Vec::new();
    let mut not_found = Vec::new();
    if ids.is_none() || ids == Some(&Value::Null) {
        list.extend(all.iter().map(jtypes::category_json));
    } else {
        for id in ids.and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = id.as_str() else { continue };
            match all.iter().find(|c| c.id.as_str() == id) {
                Some(c) => list.push(jtypes::category_json(c)),
                None => not_found.push(json!(id)),
            }
        }
    }
    Ok(
        json!({ "accountId": account.account_id(), "state": state, "list": list, "notFound": not_found }),
    )
}

async fn category_set(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let old_state = account.acc.state().await.map_err(store_err)?;
    if let Some(expected) = args.get("ifInState").and_then(Value::as_str)
        && expected != old_state
    {
        return Err(method_error("stateMismatch"));
    }

    let (mut created, mut not_created) = (Map::new(), Map::new());
    let (mut updated, mut not_updated) = (Map::new(), Map::new());
    let (mut destroyed, mut not_destroyed) = (Vec::new(), Map::new());

    // create
    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for (cid, props) in creates {
            let name = props
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                not_created.insert(
                    cid.clone(),
                    method_error_desc("invalidProperties", "name required"),
                );
                continue;
            }
            let color = match parse_color(props.get("color")) {
                Ok(c) => c,
                Err(e) => {
                    not_created.insert(cid.clone(), e);
                    continue;
                }
            };
            match account.acc.create_category(name, color.as_deref()).await {
                Ok(id) => {
                    created.insert(
                        cid.clone(),
                        json!({ "id": id.as_str(), "keyword": alo_store::category_keyword(&id) }),
                    );
                }
                Err(e) => {
                    not_created.insert(cid.clone(), set_error(&e));
                }
            }
        }
    }

    // update (name and/or color)
    if let Some(updates) = args.get("update").and_then(Value::as_object) {
        // Snapshot the current catalog to resolve partial patches (name-only or
        // color-only) against the stored value, and to reject foreign ids.
        let current = account.acc.categories().await.map_err(store_err)?;
        for (id, patch) in updates {
            let Some(existing) = current.iter().find(|c| c.id.as_str() == id.as_str()) else {
                not_updated.insert(id.clone(), method_error("notFound"));
                continue;
            };
            let name = match patch.get("name") {
                None => existing.name.clone(),
                Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_owned(),
                Some(_) => {
                    not_updated.insert(
                        id.clone(),
                        method_error_desc("invalidProperties", "invalid name"),
                    );
                    continue;
                }
            };
            let color = match patch.get("color") {
                None => existing.color.clone(),
                Some(c) => match parse_color(Some(c)) {
                    Ok(c) => c,
                    Err(e) => {
                        not_updated.insert(id.clone(), e);
                        continue;
                    }
                },
            };
            let cat = CategoryId::new(id.as_str());
            match account
                .acc
                .update_category(&cat, &name, color.as_deref())
                .await
            {
                Ok(()) => {
                    updated.insert(id.clone(), Value::Null);
                }
                Err(e) => {
                    not_updated.insert(id.clone(), set_error(&e));
                }
            }
        }
    }

    // destroy
    if let Some(ids) = args.get("destroy").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            match account.acc.delete_category(&CategoryId::new(id)).await {
                Ok(()) => destroyed.push(json!(id)),
                Err(e) => {
                    not_destroyed.insert(id.to_owned(), set_error(&e));
                }
            }
        }
    }

    let new_state = account.acc.state().await.map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(), "oldState": old_state, "newState": new_state,
        "created": created, "updated": updated, "destroyed": destroyed,
        "notCreated": not_created, "notUpdated": not_updated, "notDestroyed": not_destroyed
    }))
}

// ---- Contact (address book) -------------------------------------------

/// `Contact/get`: the account's saved contacts (all, or by `ids`).
async fn contact_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let state = account.acc.state().await.map_err(store_err)?;
    let all = account.acc.contacts().await.map_err(store_err)?;

    let ids = args.get("ids");
    let mut list = Vec::new();
    let mut not_found = Vec::new();
    if ids.is_none() || ids == Some(&Value::Null) {
        list.extend(all.iter().map(jtypes::contact_json));
    } else {
        for id in ids.and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = id.as_str() else { continue };
            match all.iter().find(|c| c.id.as_str() == id) {
                Some(c) => list.push(jtypes::contact_json(c)),
                None => not_found.push(json!(id)),
            }
        }
    }
    Ok(
        json!({ "accountId": account.account_id(), "state": state, "list": list, "notFound": not_found }),
    )
}

/// `Contact/set`: create / update / destroy address-book contacts.
async fn contact_set(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let old_state = account.acc.state().await.map_err(store_err)?;
    if let Some(expected) = args.get("ifInState").and_then(Value::as_str)
        && expected != old_state
    {
        return Err(method_error("stateMismatch"));
    }

    let (mut created, mut not_created) = (Map::new(), Map::new());
    let (mut updated, mut not_updated) = (Map::new(), Map::new());
    let (mut destroyed, mut not_destroyed) = (Vec::new(), Map::new());

    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for (cid, props) in creates {
            let contact = match parse_contact(props, None) {
                Ok(c) => c,
                Err(e) => {
                    not_created.insert(cid.clone(), e);
                    continue;
                }
            };
            match account.acc.create_contact(&contact).await {
                Ok(id) => {
                    created.insert(cid.clone(), json!({ "id": id.as_str() }));
                }
                Err(e) => {
                    not_created.insert(cid.clone(), set_error(&e));
                }
            }
        }
    }

    if let Some(updates) = args.get("update").and_then(Value::as_object) {
        // Snapshot so a partial patch merges over the stored value and a
        // foreign id is rejected as notFound (never touches another account).
        let current = account.acc.contacts().await.map_err(store_err)?;
        for (id, patch) in updates {
            let Some(existing) = current.iter().find(|c| c.id.as_str() == id.as_str()) else {
                not_updated.insert(id.clone(), method_error("notFound"));
                continue;
            };
            let contact = match parse_contact(patch, Some(existing)) {
                Ok(c) => c,
                Err(e) => {
                    not_updated.insert(id.clone(), e);
                    continue;
                }
            };
            match account
                .acc
                .update_contact(&ContactId::new(id.as_str()), &contact)
                .await
            {
                Ok(()) => {
                    updated.insert(id.clone(), Value::Null);
                }
                Err(e) => {
                    not_updated.insert(id.clone(), set_error(&e));
                }
            }
        }
    }

    if let Some(ids) = args.get("destroy").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            match account.acc.delete_contact(&ContactId::new(id)).await {
                Ok(()) => destroyed.push(json!(id)),
                Err(e) => {
                    not_destroyed.insert(id.to_owned(), set_error(&e));
                }
            }
        }
    }

    let new_state = account.acc.state().await.map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(), "oldState": old_state, "newState": new_state,
        "created": created, "updated": updated, "destroyed": destroyed,
        "notCreated": not_created, "notUpdated": not_updated, "notDestroyed": not_destroyed
    }))
}

/// Builds a [`Contact`] from a create/update patch. On update, `base`
/// supplies any field the patch omits (partial patches merge). Validates
/// the display name (derivable and non-empty) and every address/number.
fn parse_contact(props: &Value, base: Option<&Contact>) -> Result<Contact, Value> {
    let get_str = |key: &str| -> Option<Option<String>> {
        // Some(Some(v)) = set to v; Some(None) = explicitly cleared/absent-in-base;
        // None = not in this patch (keep base).
        match props.get(key) {
            None => None,
            Some(Value::Null) => Some(None),
            Some(Value::String(s)) if !s.trim().is_empty() => Some(Some(s.trim().to_owned())),
            Some(Value::String(_)) => Some(None),
            Some(_) => Some(None),
        }
    };
    let field = |key: &str| -> Option<String> {
        get_str(key).unwrap_or_else(|| base.and_then(|b| pick(b, key)))
    };

    let first_name = field("firstName");
    let last_name = field("lastName");
    let organization = field("organization");
    let job_title = field("jobTitle");
    let notes = field("notes");

    let emails = match props.get("emails") {
        Some(v) => parse_contact_fields(v, "emails")?,
        None => base.map(|b| b.emails.clone()).unwrap_or_default(),
    };
    let phones = match props.get("phones") {
        Some(v) => parse_contact_fields(v, "phones")?,
        None => base.map(|b| b.phones.clone()).unwrap_or_default(),
    };

    // Display name: an explicit `name`, else derive from N or the first
    // email, else keep the base's. Never empty (vCard FN is required).
    let display_name = match get_str("name") {
        Some(Some(name)) => name,
        _ => derive_display_name(&first_name, &last_name, &emails)
            .or_else(|| base.map(|b| b.display_name.clone()))
            .ok_or_else(|| {
                method_error_desc("invalidProperties", "a contact needs a name, or an email")
            })?,
    };

    Ok(Contact {
        id: ContactId::new(base.map(|b| b.id.as_str().to_owned()).unwrap_or_default()),
        display_name,
        first_name,
        last_name,
        emails,
        phones,
        organization,
        job_title,
        notes,
    })
}

/// The stored value of a scalar contact field, for merge-on-update.
fn pick(c: &Contact, key: &str) -> Option<String> {
    match key {
        "firstName" => c.first_name.clone(),
        "lastName" => c.last_name.clone(),
        "organization" => c.organization.clone(),
        "jobTitle" => c.job_title.clone(),
        "notes" => c.notes.clone(),
        _ => None,
    }
}

/// A display name from the structured parts, or `None` if nothing usable.
fn derive_display_name(
    first: &Option<String>,
    last: &Option<String>,
    emails: &[ContactField],
) -> Option<String> {
    match (first, last) {
        (Some(f), Some(l)) => Some(format!("{f} {l}")),
        (Some(f), None) => Some(f.clone()),
        (None, Some(l)) => Some(l.clone()),
        (None, None) => emails.first().map(|e| e.value.clone()),
    }
}

/// Parses an `emails`/`phones` array of `{kind?, value}` objects,
/// validating each value is present and control-character-free.
fn parse_contact_fields(value: &Value, field: &str) -> Result<Vec<ContactField>, Value> {
    let Some(array) = value.as_array() else {
        return Err(method_error_desc("invalidProperties", "expected an array"));
    };
    let mut out = Vec::new();
    for item in array {
        let val = item
            .get("value")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(val) = val else {
            return Err(method_error_desc(
                "invalidProperties",
                &format!("each {field} entry needs a non-empty value"),
            ));
        };
        if val.len() > 320 || val.chars().any(|c| c.is_control()) {
            return Err(method_error_desc("invalidProperties", "invalid value"));
        }
        let kind = item
            .get("kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        out.push(ContactField {
            kind,
            value: val.to_owned(),
        });
    }
    Ok(out)
}

// ---- Email ------------------------------------------------------------

async fn email_get(account: &Account, args: &Value, state: &AppState) -> Result<Value, Value> {
    check_account(args, account)?;
    let acct_state = account.acc.state().await.map_err(store_err)?;
    let want_body = args
        .get("fetchTextBodyValues")
        .and_then(Value::as_bool)
        .or_else(|| args.get("fetchAllBodyValues").and_then(Value::as_bool))
        .unwrap_or(false);
    let max_body = args
        .get("maxBodyValueBytes")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).min(state.limits.max_body_value_bytes))
        .unwrap_or(state.limits.max_body_value_bytes);

    let mut list = Vec::new();
    let mut not_found = Vec::new();
    let ids: Vec<&str> = args
        .get("ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(state.limits.max_objects_in_get)
        .collect();

    for id in ids {
        let mid = MessageId::new(id);
        // The account door scopes every read: a foreign message is
        // NotFound from message() itself — no separate ownership guard.
        match account.acc.message(&mid).await {
            Ok(m) => {
                // A folder-restricted delegate can only read messages in granted
                // folders; anything else is NotFound (no oracle).
                if !message_folder_allowed(account, &mid).await {
                    not_found.push(json!(id));
                    continue;
                }
                let mailbox_ids: Vec<String> = account
                    .acc
                    .mailboxes_of_message(&mid)
                    .await
                    .map_err(store_err)?
                    .into_iter()
                    .map(|b| b.to_string())
                    .collect();
                let keywords = account.acc.keywords(&mid).await.map_err(store_err)?;
                let flag_due = account.acc.flag_due(&mid).await.map_err(store_err)?;
                let body = if want_body {
                    let raw = account.acc.message_bytes(&mid).await.map_err(store_err)?;
                    Some(read_body(&raw, m.blob_id.as_str(), max_body))
                } else {
                    None
                };
                list.push(jtypes::email_json(
                    &m,
                    &mailbox_ids,
                    &keywords,
                    body.as_ref(),
                    flag_due,
                ));
            }
            Err(StoreError::NotFound) => not_found.push(json!(id)),
            Err(e) => return Err(store_err(e)),
        }
    }
    Ok(
        json!({ "accountId": account.account_id(), "state": acct_state, "list": list, "notFound": not_found }),
    )
}

async fn email_query(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let query_state = account.acc.state().await.map_err(store_err)?;

    let filter = parse_email_filter(args.get("filter"));
    let sort = parse_sort(args.get("sort"));
    let position = args
        .get("position")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(50);
    let page = Page::new(limit, position);

    let query = EmailQuery { filter, sort, page };
    let results = account.acc.query_emails(&query).await.map_err(store_err)?;
    // A folder-restricted delegate (ADR 0017) never sees messages outside its
    // granted folders — filter the page to visible messages. (This can shorten
    // a page below `limit`; correctness over exact paging for restricted views.)
    let mut ids: Vec<String> = Vec::with_capacity(results.len());
    for m in &results {
        if message_folder_allowed(account, &m.id).await {
            ids.push(m.id.to_string());
        }
    }

    Ok(json!({
        "accountId": account.account_id(),
        "queryState": query_state,
        "canCalculateChanges": false,
        "position": position,
        "ids": ids
    }))
}

async fn email_set(account: &Account, args: &Value, state: &AppState) -> Result<Value, Value> {
    check_account(args, account)?;
    let old_state = account.acc.state().await.map_err(store_err)?;
    if let Some(expected) = args.get("ifInState").and_then(Value::as_str)
        && expected != old_state
    {
        return Err(method_error("stateMismatch"));
    }

    let (mut created, mut not_created) = (Map::new(), Map::new());
    let (mut updated, mut not_updated) = (Map::new(), Map::new());
    let (mut destroyed, mut not_destroyed) = (Vec::new(), Map::new());

    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for (cid, props) in creates {
            match email_create(account, props).await {
                Ok(created_obj) => {
                    created.insert(cid.clone(), created_obj);
                }
                Err(e) => {
                    not_created.insert(cid.clone(), e);
                }
            }
        }
    }

    if let Some(updates) = args.get("update").and_then(Value::as_object) {
        for (id, patch) in updates {
            // Account-scoped existence check: a foreign message is
            // NotFound, so an empty patch cannot report a spurious success.
            if let Err(e) = account.acc.message(&MessageId::new(id.as_str())).await {
                not_updated.insert(id.clone(), set_error(&e));
                continue;
            }
            // Per-folder (ADR 0017): the message must live in a granted folder,
            // and any folder it is moved into must be granted too.
            if let Some(d) = &account.delegated
                && d.folders.is_some()
            {
                let mid = MessageId::new(id.as_str());
                if !message_folder_allowed(account, &mid).await {
                    not_updated.insert(id.clone(), method_error("notFound"));
                    continue;
                }
                if !patch_dest_allowed(d, patch) {
                    not_updated.insert(id.clone(), method_error("forbidden"));
                    continue;
                }
            }
            // Junk training: snapshot Junk membership before a patch
            // that touches mailboxes, compare after — a move into Junk
            // is a spam report, out of Junk a ham report (best-effort,
            // spawned; never affects the Email/set outcome).
            let junk_before = junk_membership_before(account, state, id, patch).await;
            match email_update(account, id, patch).await {
                Ok(()) => {
                    updated.insert(id.clone(), Value::Null);
                    if let Some((junk, was_in)) = junk_before {
                        learn_junk_transition(account, state, id, &junk, was_in).await;
                    }
                }
                Err(e) => {
                    not_updated.insert(id.clone(), e);
                }
            }
        }
    }

    if let Some(ids) = args.get("destroy").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            let mid = MessageId::new(id);
            // Per-folder (ADR 0017): a restricted delegate can only destroy
            // messages in granted folders; invisible ones are NotFound.
            if account
                .delegated
                .as_ref()
                .is_some_and(|d| d.folders.is_some())
                && !message_folder_allowed(account, &mid).await
            {
                not_destroyed.insert(id.to_owned(), method_error("notFound"));
                continue;
            }
            // destroy_message is account-scoped: a foreign message is
            // NotFound → notDestroyed, no separate guard to forget.
            match account.acc.destroy_message(&mid).await {
                Ok(()) => destroyed.push(json!(id)),
                Err(e) => {
                    not_destroyed.insert(id.to_owned(), set_error(&e));
                }
            }
        }
    }

    let new_state = account.acc.state().await.map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(), "oldState": old_state, "newState": new_state,
        "created": created, "updated": updated, "destroyed": destroyed,
        "notCreated": not_created, "notUpdated": not_updated, "notDestroyed": not_destroyed
    }))
}

/// Draft create: builds a proper RFC 5322 `text/plain` message from the JMAP
/// Email properties (From, all To/Cc, Subject, reply headers, and the text
/// body — non-ASCII correctly encoded, see [`crate::mime`]) and ingests it into
/// the first target mailbox, applying the requested keywords.
async fn email_create(account: &Account, props: &Value) -> Result<Value, Value> {
    let mailbox_ids = props.get("mailboxIds").and_then(Value::as_object);
    let Some(first_mailbox) = mailbox_ids.and_then(|m| m.keys().next()) else {
        return Err(method_error_desc(
            "invalidProperties",
            "mailboxIds required",
        ));
    };
    // A folder-restricted delegate (ADR 0017) may only create in granted folders.
    if let Some(d) = &account.delegated
        && mailbox_ids.is_some_and(|m| m.keys().any(|mb| !d.folder_allowed(mb)))
    {
        return Err(method_error("forbidden"));
    }
    let mailbox = MailboxId::new(first_mailbox.as_str());
    // The account door scopes ingest: a foreign target mailbox is
    // NotFound from ingest itself — no separate ownership guard here.

    let from = parse_addr_first(props.get("from")).unwrap_or(crate::mime::Addr {
        name: None,
        email: String::new(),
    });
    let domain = domain_of(&from.email);

    // Resolve attachments: fetch each referenced (just-uploaded) blob's bytes to
    // carry into the multipart build. A missing/foreign blob is a hard error —
    // better to fail the create than to send silently without the attachment.
    let mut attachments = Vec::new();
    if let Some(list) = props.get("attachments").and_then(Value::as_array) {
        for att in list {
            let Some(blob_id) = att.get("blobId").and_then(Value::as_str) else {
                return Err(method_error_desc(
                    "invalidProperties",
                    "attachment blobId required",
                ));
            };
            let bytes = account
                .acc
                .blob_bytes_for_send(&BlobId::new(blob_id.to_owned()))
                .await
                .map_err(|e| set_error(&e))?;
            attachments.push(crate::mime::Attachment {
                name: att
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("attachment")
                    .to_owned(),
                content_type: att
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
                bytes: bytes.to_vec(),
            });
        }
    }

    let outgoing = crate::mime::Outgoing {
        from,
        to: parse_addr_list(props.get("to")),
        cc: parse_addr_list(props.get("cc")),
        bcc: parse_addr_list(props.get("bcc")),
        subject: props
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        in_reply_to: parse_msgids(props.get("inReplyTo")),
        references: parse_msgids(props.get("references")),
        body_text: compose_body_text(props),
        body_html: compose_body_html(props),
        attachments,
        message_id_domain: domain,
        message_id_token: new_message_token(),
    };
    let raw = crate::mime::build(&outgoing);

    let id = account
        .acc
        .ingest(&mailbox, &raw)
        .await
        .map_err(|e| set_error(&e))?;
    // Drafts default to $draft; apply requested keywords (failures are
    // surfaced, not swallowed).
    account
        .acc
        .set_keyword(&id, "$draft", true)
        .await
        .map_err(|e| set_error(&e))?;
    if let Some(keywords) = props.get("keywords").and_then(Value::as_object) {
        for (kw, on) in keywords {
            account
                .acc
                .set_keyword(&id, kw, on.as_bool().unwrap_or(false))
                .await
                .map_err(|e| set_error(&e))?;
        }
    }
    // Return the server-set properties (real blobId/threadId/size).
    let m = account.acc.message(&id).await.map_err(|e| set_error(&e))?;
    Ok(json!({
        "id": m.id.as_str(),
        "blobId": m.blob_id.as_str(),
        "threadId": m.thread_id.as_str(),
        "size": m.size
    }))
}

/// Parses a JMAP `EmailAddress` object (`{name?, email}`) into a builder Addr.
fn parse_addr_obj(v: &Value) -> Option<crate::mime::Addr> {
    let email = v.get("email").and_then(Value::as_str)?;
    if email.is_empty() {
        return None;
    }
    Some(crate::mime::Addr {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned),
        email: email.to_owned(),
    })
}

/// First address of a JMAP address array (e.g. `from`).
fn parse_addr_first(v: Option<&Value>) -> Option<crate::mime::Addr> {
    v.and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(parse_addr_obj)
}

/// All addresses of a JMAP address array (e.g. `to`, `cc`).
fn parse_addr_list(v: Option<&Value>) -> Vec<crate::mime::Addr> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(parse_addr_obj).collect())
        .unwrap_or_default()
}

/// Message-id strings from a JMAP array (`inReplyTo`, `references`).
fn parse_msgids(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The plain-text body to compose from: the `textBody` part's value, else the
/// first body value. (Distinct from `extract_text_body`, which reads a *stored*
/// message's bytes for display.)
fn compose_body_text(props: &Value) -> String {
    let body_values = props.get("bodyValues").and_then(Value::as_object);
    if let Some(part) = props
        .get("textBody")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        && let Some(pid) = part.get("partId").and_then(Value::as_str)
        && let Some(v) = body_values
            .and_then(|bv| bv.get(pid))
            .and_then(|x| x.get("value"))
            .and_then(Value::as_str)
    {
        return v.to_owned();
    }
    body_values
        .and_then(|m| m.values().next())
        .and_then(|v| v.get("value"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

/// The HTML body to compose from, if the draft has an `htmlBody` part with a
/// non-empty value.
fn compose_body_html(props: &Value) -> Option<String> {
    let body_values = props.get("bodyValues").and_then(Value::as_object);
    let pid = props
        .get("htmlBody")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|p| p.get("partId"))
        .and_then(Value::as_str)?;
    let value = body_values
        .and_then(|bv| bv.get(pid))
        .and_then(|x| x.get("value"))
        .and_then(Value::as_str)?;
    (!value.trim().is_empty()).then(|| value.to_owned())
}

/// The domain of an addr-spec, for seeding the `Message-ID`.
pub(crate) fn domain_of(email: &str) -> String {
    match email.rsplit('@').next() {
        Some(d) if !d.is_empty() && d != email => d.to_owned(),
        _ => "localhost".to_owned(),
    }
}

/// The local part for a generated `Message-ID`.
///
/// Delegates to the store, which owns the field: one generator means a second
/// caller cannot invent a weaker one. A timestamp alone was not enough — two
/// messages in the same nanosecond would collide, and receiving servers
/// deduplicate on `Message-ID`.
pub(crate) fn new_message_token() -> String {
    alo_store::mime_write::new_message_id_token()
}

/// Whether `patch` touches mailbox membership at all (full replacement
/// or `mailboxIds/<id>` patch keys) — the junk-training precondition.
fn patch_touches_mailboxes(patch: &Value) -> bool {
    patch.as_object().is_some_and(|obj| {
        obj.keys()
            .any(|k| k == "mailboxIds" || k.starts_with("mailboxIds/"))
    })
}

/// When junk training is on and `patch` moves mailboxes: the account's
/// Junk mailbox id plus whether the message is in it **before** the
/// patch. `None` disables the post-update comparison (training off, no
/// Junk folder yet, patch doesn't move anything, or a lookup failed —
/// training is strictly best-effort).
async fn junk_membership_before(
    account: &Account,
    state: &AppState,
    id: &str,
    patch: &Value,
) -> Option<(MailboxId, bool)> {
    state.junk_learner.as_ref()?;
    if !patch_touches_mailboxes(patch) {
        return None;
    }
    let junk = account.acc.mailbox_by_role("junk").await.ok().flatten()?;
    let mailboxes = account
        .acc
        .mailboxes_of_message(&MessageId::new(id))
        .await
        .ok()?;
    let was_in = mailboxes.contains(&junk);
    Some((junk, was_in))
}

/// After a successful update: if Junk membership flipped, report the
/// message to the learner (spawned — the JMAP response never waits on
/// the scanner).
async fn learn_junk_transition(
    account: &Account,
    state: &AppState,
    id: &str,
    junk: &MailboxId,
    was_in_junk: bool,
) {
    let Some(learner) = &state.junk_learner else {
        return;
    };
    let mid = MessageId::new(id);
    let Ok(now) = account.acc.mailboxes_of_message(&mid).await else {
        return;
    };
    let is_in_junk = now.contains(junk);
    if is_in_junk == was_in_junk {
        return;
    }
    let Ok(raw) = account.acc.message_bytes(&mid).await else {
        return;
    };
    let learner = std::sync::Arc::clone(learner);
    tokio::spawn(async move {
        learner.learn(is_in_junk, raw.to_vec()).await;
    });
}

/// Applies an Email/set update patch: full or patched `keywords` and
/// `mailboxIds`.
async fn email_update(account: &Account, id: &str, patch: &Value) -> Result<(), Value> {
    let mid = MessageId::new(id);
    let Some(obj) = patch.as_object() else {
        return Err(method_error("invalidPatch"));
    };

    // Full replacements first.
    if let Some(keywords) = obj.get("keywords").and_then(Value::as_object) {
        let current = account
            .acc
            .keywords(&mid)
            .await
            .map_err(|e| set_error(&e))?;
        for kw in &current {
            if !keywords.contains_key(kw) {
                account
                    .acc
                    .set_keyword(&mid, kw, false)
                    .await
                    .map_err(|e| set_error(&e))?;
            }
        }
        for (kw, on) in keywords {
            account
                .acc
                .set_keyword(&mid, kw, on.as_bool().unwrap_or(false))
                .await
                .map_err(|e| set_error(&e))?;
        }
    }
    if let Some(mailboxes) = obj.get("mailboxIds").and_then(Value::as_object) {
        let current: Vec<String> = account
            .acc
            .mailboxes_of_message(&mid)
            .await
            .map_err(|e| set_error(&e))?
            .into_iter()
            .map(|b| b.to_string())
            .collect();
        for existing in &current {
            if !mailboxes.contains_key(existing) {
                account
                    .acc
                    .remove_from_mailbox(&mid, &MailboxId::new(existing.as_str()))
                    .await
                    .map_err(|e| set_error(&e))?;
            }
        }
        for (mb, on) in mailboxes {
            if on.as_bool().unwrap_or(false) && !current.contains(mb) {
                account
                    .acc
                    .add_to_mailbox(&mid, &MailboxId::new(mb.as_str()))
                    .await
                    .map_err(|e| set_error(&e))?;
            }
        }
    }

    // Patch keys: `keywords/X` and `mailboxIds/X`.
    for (key, value) in obj {
        if let Some(kw) = key.strip_prefix("keywords/") {
            let on = value.as_bool().unwrap_or(!value.is_null());
            account
                .acc
                .set_keyword(&mid, kw, on)
                .await
                .map_err(|e| set_error(&e))?;
        } else if let Some(mb) = key.strip_prefix("mailboxIds/") {
            let mailbox = MailboxId::new(mb);
            if value.is_null() || value.as_bool() == Some(false) {
                account
                    .acc
                    .remove_from_mailbox(&mid, &mailbox)
                    .await
                    .map_err(|e| set_error(&e))?;
            } else {
                account
                    .acc
                    .add_to_mailbox(&mid, &mailbox)
                    .await
                    .map_err(|e| set_error(&e))?;
            }
        }
    }
    Ok(())
}

// ---- Thread -----------------------------------------------------------

/// The words to highlight in a `SearchSnippet`, gathered from the filter's text
/// conditions (recursing into AND/OR/NOT operator trees).
fn collect_search_terms(filter: Option<&Value>) -> Vec<String> {
    let mut terms = Vec::new();
    let Some(obj) = filter.and_then(Value::as_object) else {
        return terms;
    };
    for key in ["text", "subject", "body", "from", "to"] {
        if let Some(s) = obj.get(key).and_then(Value::as_str) {
            terms.extend(
                s.split_whitespace()
                    .filter(|w| w.len() >= 2)
                    .map(str::to_owned),
            );
        }
    }
    for cond in obj
        .get("conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        terms.extend(collect_search_terms(Some(cond)));
    }
    terms
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(ch),
    }
}

/// HTML-escapes `text` and wraps each (ASCII-case-insensitive) occurrence of a
/// search term in `<mark>…</mark>`, per the JMAP SearchSnippet convention. ASCII
/// lowercasing preserves byte length, so match offsets stay aligned with the
/// original; term bytes are ASCII, so they never fall inside a multibyte char.
fn highlight(text: &str, terms: &[String]) -> String {
    let lower = text.to_ascii_lowercase();
    let mut marked = vec![false; text.len()];
    for term in terms {
        let t = term.to_ascii_lowercase();
        if t.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&t) {
            let s = from + pos;
            let e = s + t.len();
            marked[s..e].iter_mut().for_each(|m| *m = true);
            from = e;
        }
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_mark = false;
    let mut idx = 0;
    for ch in text.chars() {
        let is_marked = marked.get(idx).copied().unwrap_or(false);
        if is_marked && !in_mark {
            out.push_str("<mark>");
            in_mark = true;
        } else if !is_marked && in_mark {
            out.push_str("</mark>");
            in_mark = false;
        }
        push_escaped(&mut out, ch);
        idx += ch.len_utf8();
    }
    if in_mark {
        out.push_str("</mark>");
    }
    out
}

/// `SearchSnippet/get` (RFC 8621 §5.1): for each requested email, the subject
/// and a body preview with the search terms highlighted (`<mark>`), so a client
/// can show why a message matched. Respects a folder-restricted delegate's grant.
async fn search_snippet_get(
    account: &Account,
    args: &Value,
    state: &AppState,
) -> Result<Value, Value> {
    check_account(args, account)?;
    let terms = collect_search_terms(args.get("filter"));
    let ids: Vec<&str> = args
        .get("emailIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(state.limits.max_objects_in_get)
        .collect();

    let mut list = Vec::new();
    let mut not_found = Vec::new();
    for id in ids {
        let mid = MessageId::new(id);
        match account.acc.message(&mid).await {
            Ok(m) => {
                if !message_folder_allowed(account, &mid).await {
                    not_found.push(json!(id));
                    continue;
                }
                let preview = match account.acc.message_bytes(&mid).await {
                    Ok(raw) => jtypes::preview_of(&read_body(
                        &raw,
                        m.blob_id.as_str(),
                        state.limits.max_body_value_bytes,
                    )),
                    Err(_) => String::new(),
                };
                list.push(json!({
                    "emailId": id,
                    "subject": highlight(&m.subject, &terms),
                    "preview": highlight(&preview, &terms),
                }));
            }
            Err(StoreError::NotFound) => not_found.push(json!(id)),
            Err(e) => return Err(store_err(e)),
        }
    }
    Ok(json!({ "accountId": account.account_id(), "list": list, "notFound": not_found }))
}

/// A stable, JMAP-id-safe (`A-Za-z0-9_-`) id for a send identity, derived from
/// its address via FNV-1a-64 → 16 hex chars.
fn identity_id(address: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in address.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `Identity/get` (RFC 8621 §6.1): the addresses the signed-in user may send
/// from — canonical + aliases, the same set the submission path authorizes —
/// as one JMAP Identity each. Standard clients (Thunderbird, Apple Mail) need
/// this before they can submit. Read-only: identities are provisioned, so
/// `mayDelete` is false and there is no `Identity/set`.
async fn identity_get(account: &Account, args: &Value, state: &AppState) -> Result<Value, Value> {
    check_account(args, account)?;
    let acct_state = account.acc.state().await.map_err(store_err)?;
    let ts = state.store.for_tenant(account.tenant.clone());

    let mut addresses: Vec<String> = Vec::new();
    if let Ok(Some(canonical)) = ts.email_of(&account.user).await {
        addresses.push(canonical);
    }
    if let Ok(aliases) = ts.aliases_of(&account.user).await {
        addresses.extend(aliases);
    }
    let signature = account.acc.signature().await.unwrap_or_default();

    let wanted: Option<std::collections::HashSet<String>> =
        args.get("ids").and_then(Value::as_array).map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });

    let mut list = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for addr in addresses {
        if !seen.insert(addr.clone()) {
            continue;
        }
        let id = identity_id(&addr);
        if wanted.as_ref().is_some_and(|w| !w.contains(&id)) {
            continue;
        }
        list.push(json!({
            "id": id,
            "name": "",
            "email": addr,
            "replyTo": Value::Null,
            "bcc": Value::Null,
            "textSignature": "",
            "htmlSignature": signature,
            "mayDelete": false,
        }));
    }
    let not_found: Vec<Value> = match &wanted {
        Some(w) => {
            let have: std::collections::HashSet<&str> =
                list.iter().filter_map(|i| i["id"].as_str()).collect();
            w.iter()
                .filter(|id| !have.contains(id.as_str()))
                .map(|id| json!(id))
                .collect()
        }
        None => Vec::new(),
    };
    Ok(json!({
        "accountId": account.account_id(),
        "state": acct_state,
        "list": list,
        "notFound": not_found,
    }))
}

/// `Quota/get` (RFC 9425): the tenant's mail storage quota — used and hard-limit
/// octets — so a client can show a storage bar. A tenant with no cap (unlimited)
/// reports no quota object. The cap is enforced on ingest and draft creation
/// (ADR 0012); this method just surfaces it.
async fn quota_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let acct_state = account.acc.state().await.map_err(store_err)?;
    let (used, limit) = account.acc.storage_usage().await.map_err(store_err)?;
    let mut all = Vec::new();
    if let Some(hard) = limit {
        all.push(json!({
            "id": "octets",
            "resourceType": "octets",
            "used": used.max(0),
            "hardLimit": hard.max(0),
            "scope": "domain",
            "name": "storage",
            "types": ["Mail"],
        }));
    }
    let (list, not_found) = match args.get("ids").and_then(Value::as_array) {
        None => (all, Vec::new()),
        Some(ids) => {
            let mut list = Vec::new();
            let mut not_found = Vec::new();
            for id in ids {
                match all.iter().find(|q| q["id"].as_str() == id.as_str()) {
                    Some(q) => list.push(q.clone()),
                    None => not_found.push(id.clone()),
                }
            }
            (list, not_found)
        }
    };
    Ok(json!({
        "accountId": account.account_id(), "state": acct_state, "list": list, "notFound": not_found,
    }))
}

/// A `UTCDate` as RFC 8621 §1.4 writes it: `YYYY-MM-DDTHH:MM:SSZ`.
fn utc_date(at: time::OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// Reads a `UTCDate|null` out of a patch.
///
/// `Ok(None)` is an explicit null — the client clearing that bound — which is
/// a different thing from the property being absent, and only the caller can
/// tell those apart. `Err` is a value that is neither.
fn parse_utc_date(value: &Value) -> Result<Option<time::OffsetDateTime>, ()> {
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().ok_or(())?;
    // The format is fixed by the specification, so it is parsed rather than
    // guessed at: a lenient reader here would accept a local time and silently
    // move somebody's holiday by an hour.
    let parsed = time::PrimitiveDateTime::parse(
        text,
        &time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z"),
    )
    .map_err(|_| ())?;
    Ok(Some(parsed.assume_utc()))
}

/// `VacationResponse/get` (RFC 8621 §8): the singleton auto-reply, mapped from
/// the account's stored out-of-office — the switch, the subject, the message,
/// and the window it applies in. Either date may be null, which the standard
/// gives a meaning to on each side independently: no start is "already in
/// effect", no end is "until switched off".
async fn vacation_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let acct_state = account.acc.state().await.map_err(store_err)?;
    let ooo = account.acc.out_of_office().await.map_err(store_err)?;
    let (enabled, subject, message) = (ooo.enabled, ooo.subject.clone(), ooo.message.clone());
    // Both dates were reported as null whatever the user had set, so a client
    // that scheduled a holiday was told its own dates had not been stored.
    let obj = json!({
        "id": "singleton",
        "isEnabled": enabled,
        "fromDate": ooo.from.map_or(Value::Null, |t| json!(utc_date(t))),
        "toDate": ooo.to.map_or(Value::Null, |t| json!(utc_date(t))),
        "subject": if subject.is_empty() { Value::Null } else { json!(subject) },
        "textBody": if message.is_empty() { Value::Null } else { json!(message) },
        "htmlBody": Value::Null,
    });
    // The only id is "singleton"; a request for other ids yields notFound.
    let (list, not_found) = match args.get("ids").and_then(Value::as_array) {
        None => (vec![obj], Vec::new()),
        Some(ids) => {
            let mut list = Vec::new();
            let mut not_found = Vec::new();
            for id in ids {
                if id.as_str() == Some("singleton") {
                    list.push(obj.clone());
                } else {
                    not_found.push(id.clone());
                }
            }
            (list, not_found)
        }
    };
    Ok(json!({
        "accountId": account.account_id(), "state": acct_state, "list": list, "notFound": not_found,
    }))
}

/// `VacationResponse/set` (RFC 8621 §8): update the singleton auto-reply. Only
/// `update` on `"singleton"` is meaningful (create/destroy are refused). Writes
/// through the same path as the settings route (state + managed Sieve rebuild),
/// so vacation coexists with the account's mail filters.
async fn vacation_set(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let old_state = account.acc.state().await.map_err(store_err)?;
    let mut updated = Map::new();
    let mut not_updated = Map::new();

    if let Some(update) = args.get("update").and_then(Value::as_object) {
        for (id, patch) in update {
            if id != "singleton" {
                not_updated.insert(id.clone(), method_error("notFound"));
                continue;
            }
            let current = account.acc.out_of_office().await.map_err(store_err)?;
            let (mut enabled, mut subject, mut message) = (
                current.enabled,
                current.subject.clone(),
                current.message.clone(),
            );
            // A patch that names neither date leaves the window alone; one that
            // names a date as null clears that bound, which is how RFC 8621 §8
            // says "no start" and "no end".
            let mut from = current.from;
            let mut to = current.to;
            let mut bad_date: Option<&str> = None;
            if let Some(v) = patch.get("fromDate") {
                match parse_utc_date(v) {
                    Ok(parsed) => from = parsed,
                    Err(()) => bad_date = Some("fromDate"),
                }
            }
            if let Some(v) = patch.get("toDate") {
                match parse_utc_date(v) {
                    Ok(parsed) => to = parsed,
                    Err(()) => bad_date = Some("toDate"),
                }
            }
            if let Some(field) = bad_date {
                not_updated.insert(
                    id.clone(),
                    method_error_desc(
                        "invalidProperties",
                        &format!("{field} must be a UTCDate or null"),
                    ),
                );
                continue;
            }
            if let (Some(start), Some(end)) = (from, to)
                && start >= end
            {
                not_updated.insert(
                    id.clone(),
                    method_error_desc("invalidProperties", "toDate must be after fromDate"),
                );
                continue;
            }
            if let Some(v) = patch.get("isEnabled").and_then(Value::as_bool) {
                enabled = v;
            }
            if let Some(v) = patch.get("subject") {
                subject = v.as_str().unwrap_or("").to_owned();
            }
            // We store one message body; prefer textBody, fall back to htmlBody.
            if let Some(v) = patch.get("textBody") {
                message = v.as_str().unwrap_or("").to_owned();
            } else if let Some(v) = patch.get("htmlBody").and_then(Value::as_str) {
                message = v.to_owned();
            }
            if enabled && message.trim().is_empty() {
                not_updated.insert(
                    id.clone(),
                    method_error_desc(
                        "invalidProperties",
                        "a message is required to enable vacation",
                    ),
                );
                continue;
            }
            if account
                .acc
                .set_out_of_office_state(enabled, subject.trim(), message.trim(), from, to)
                .await
                .is_err()
                || crate::filters::rebuild_managed_script(account)
                    .await
                    .is_err()
            {
                not_updated.insert(id.clone(), method_error("serverFail"));
                continue;
            }
            updated.insert(id.clone(), Value::Null);
        }
    }

    // A singleton cannot be created or destroyed.
    let mut not_created = Map::new();
    if let Some(creates) = args.get("create").and_then(Value::as_object) {
        for cid in creates.keys() {
            not_created.insert(
                cid.clone(),
                method_error_desc("forbidden", "VacationResponse is a singleton"),
            );
        }
    }
    let mut not_destroyed = Map::new();
    for id in args
        .get("destroy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(s) = id.as_str() {
            not_destroyed.insert(
                s.to_owned(),
                method_error_desc("forbidden", "VacationResponse is a singleton"),
            );
        }
    }

    let new_state = account.acc.state().await.map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(),
        "oldState": old_state,
        "newState": new_state,
        "updated": updated, "notUpdated": not_updated,
        "created": Value::Null, "notCreated": not_created,
        "destroyed": Vec::<Value>::new(), "notDestroyed": not_destroyed,
    }))
}

async fn thread_get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let state = account.acc.state().await.map_err(store_err)?;
    let mut list = Vec::new();
    let mut not_found = Vec::new();
    for id in args
        .get("ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(500)
    {
        let tid = ThreadId::new(id);
        // The account door scopes thread_messages to this account's own
        // messages: a thread the account has no message in comes back
        // empty → notFound. No separate ownership guard.
        let members = account
            .acc
            .thread_messages(&tid, Page::first(alo_store::MAX_PAGE))
            .await
            .map_err(store_err)?;
        if members.is_empty() {
            not_found.push(json!(id));
        } else {
            let email_ids: Vec<String> = members.iter().map(|m| m.to_string()).collect();
            list.push(jtypes::thread_json(id, &email_ids));
        }
    }
    Ok(
        json!({ "accountId": account.account_id(), "state": state, "list": list, "notFound": not_found }),
    )
}

// ---- /changes (shared) ------------------------------------------------

async fn changes(
    account: &Account,
    args: &Value,
    obj_type: &str,
    state: &AppState,
) -> Result<Value, Value> {
    check_account(args, account)?;
    let since = match args
        .get("sinceState")
        .and_then(Value::as_str)
        .and_then(parse_state)
    {
        Some(s) if s >= 0 => s,
        _ => return Err(method_error("cannotCalculateChanges")),
    };
    let current = account
        .acc
        .state()
        .await
        .map_err(store_err)?
        .parse::<i64>()
        .unwrap_or(0);
    if since > current {
        return Err(method_error("cannotCalculateChanges"));
    }
    let max = args
        .get("maxChanges")
        .and_then(Value::as_i64)
        .map(|n| n.clamp(1, state.limits.max_objects_in_get as i64))
        .unwrap_or(state.limits.max_objects_in_get as i64);

    let c = account
        .acc
        .changes(obj_type, since, max)
        .await
        .map_err(store_err)?;
    Ok(json!({
        "accountId": account.account_id(),
        "oldState": c.old_state.to_string(),
        "newState": c.new_state.to_string(),
        "hasMoreChanges": c.has_more,
        "created": c.created,
        "updated": c.updated,
        "destroyed": c.destroyed
    }))
}

// ---- parsing helpers --------------------------------------------------

fn parse_state(s: &str) -> Option<i64> {
    s.parse().ok()
}

fn parse_sort(sort: Option<&Value>) -> SortDirection {
    // Only `receivedAt` is a sort option; default newest-first.
    let ascending = sort
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|c| c.get("isAscending"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if ascending {
        SortDirection::Asc
    } else {
        SortDirection::Desc
    }
}

fn parse_email_filter(filter: Option<&Value>) -> EmailFilter {
    let Some(f) = filter.and_then(Value::as_object) else {
        return EmailFilter::default();
    };
    let str_of = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_owned);
    let date_of = |k: &str| {
        f.get(k)
            .and_then(Value::as_str)
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
    };
    EmailFilter {
        in_mailbox: str_of("inMailbox").map(MailboxId::new),
        from: str_of("from"),
        to: str_of("to"),
        subject: str_of("subject"),
        text: str_of("text"),
        before: date_of("before"),
        after: date_of("after"),
        has_keyword: str_of("hasKeyword"),
        not_keyword: str_of("notKeyword"),
    }
}

/// Best-effort text body extraction: the bytes after the header/body
/// separator, lossily decoded and truncated to `max`. Full MIME
/// structure is additive later (design note out-of-scope).
fn read_body(raw: &[u8], blob_id: &str, max: usize) -> jtypes::ReadBody {
    let parsed = crate::mime_read::parse(raw);
    let text = parsed.text.map(|t| truncate_utf8(t, max));
    let attachments = parsed
        .attachments
        .into_iter()
        .map(|a| jtypes::AttachmentJson {
            // The download route resolves "{messageBlobId}~a{index}" back to
            // the decoded part (see blob::download).
            blob_id: format!("{blob_id}~a{}", a.index),
            content_type: a.content_type,
            name: a.name,
            size: a.size,
            content_id: a.content_id,
            inline: a.inline,
        })
        .collect();
    jtypes::ReadBody {
        text,
        html: parsed.html,
        attachments,
        unsubscribe: parsed.unsubscribe,
        invitation: read_invitation(raw),
    }
}

/// Summarise an inbound scheduling message from the message's `text/calendar`
/// part: an invitation (`REQUEST`) or a cancellation (`CANCEL`). Our own sent
/// copies (`REPLY`) and other methods are ignored. Times are RFC 3339 (UTC).
/// `None` for ordinary mail.
fn read_invitation(raw: &[u8]) -> Option<jtypes::Invitation> {
    let ics = crate::mime_read::calendar_part(raw)?;
    let text = String::from_utf8_lossy(&ics);
    let method = alo_store::ical::method_of(&text)?;
    if method != "REQUEST" && method != "CANCEL" && method != "REPLY" {
        return None;
    }
    let ev = alo_store::ical::from_ics(&text, "")?;
    let fmt = |t: time::OffsetDateTime| {
        t.format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    };
    // A REPLY carries the responding guest's status; map PARTSTAT to the same
    // lowercase vocabulary the RSVP card uses.
    let (attendee, partstat) = if method == "REPLY" {
        match alo_store::ical::reply_of(&text) {
            Some((email, ps)) => {
                let status = match ps.as_str() {
                    "ACCEPTED" => "accepted",
                    "DECLINED" => "declined",
                    "TENTATIVE" => "tentative",
                    _ => "tentative",
                };
                (Some(email), Some(status.to_owned()))
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };
    Some(jtypes::Invitation {
        method,
        uid: ev.id.as_str().to_owned(),
        summary: ev.summary,
        organizer: alo_store::ical::organizer_of(&text),
        starts_at: fmt(ev.starts_at),
        ends_at: fmt(ev.ends_at),
        all_day: ev.all_day,
        location: ev.location,
        attendee,
        partstat,
    })
}

/// Truncate a string to at most `max` bytes on a char boundary, reporting
/// whether it was cut (for `bodyValues.isTruncated`).
fn truncate_utf8(mut s: String, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    (s, true)
}

#[cfg(test)]
mod snippet_tests {
    use super::{collect_search_terms, highlight};
    use serde_json::json;

    #[test]
    fn highlight_marks_terms_case_insensitively_and_escapes() {
        let out = highlight("Project Falcon <go>", &["falcon".to_owned()]);
        // Match is case-insensitive; the original casing is preserved; HTML escaped.
        assert_eq!(out, "Project <mark>Falcon</mark> &lt;go&gt;");
    }

    #[test]
    fn highlight_without_terms_just_escapes() {
        assert_eq!(highlight("a & b < c", &[]), "a &amp; b &lt; c");
    }

    #[test]
    fn highlight_leaves_multibyte_text_intact() {
        // An ASCII term never falls inside a multibyte char; accented text is kept.
        assert_eq!(
            highlight("café crème", &["cr".to_owned()]),
            "café <mark>cr</mark>ème"
        );
    }

    #[test]
    fn collect_terms_gathers_from_text_and_operator_tree() {
        let filter = json!({ "operator": "AND", "conditions": [
            { "text": "falcon has" },
            { "subject": "landed" }
        ]});
        let mut terms = collect_search_terms(Some(&filter));
        terms.sort();
        assert_eq!(terms, vec!["falcon", "has", "landed"]);
    }
}
