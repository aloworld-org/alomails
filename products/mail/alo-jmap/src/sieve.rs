//! JMAP for Sieve (RFC 9661) — `SieveScript/{get,set,validate}` over the
//! account's script store (ADR 0007). Scripts are per-account data on
//! `AccountStore`, so isolation and validation (compile-on-`set`) are
//! inherited. One deviation from RFC 9661, documented in `docs/interop.md`:
//! script content is carried **inline** (`content`) rather than via a
//! `blobId` round-trip — additive to switch to blobs later.

use alo_store::StoreError;
use serde_json::{Map, Value, json};

use crate::error::{method_error, method_error_desc};
use crate::state::Account;

/// `SieveScript/get` — the account's scripts (id = name), with `isActive`
/// and inline `content`.
pub async fn get(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let metas = account
        .acc
        .list_sieve_scripts()
        .await
        .map_err(server_fail)?;
    let want: Option<Vec<String>> = args.get("ids").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    });

    let mut list = Vec::new();
    let mut not_found = Vec::new();
    for meta in metas {
        if let Some(ids) = &want
            && !ids.contains(&meta.name)
        {
            continue;
        }
        let content = account
            .acc
            .sieve_script(&meta.name)
            .await
            .unwrap_or_default();
        list.push(json!({
            "id": meta.name,
            "name": meta.name,
            "isActive": meta.active,
            "content": content,
        }));
    }
    // Report explicitly-requested ids that do not exist.
    if let Some(ids) = &want {
        let present: Vec<String> = list
            .iter()
            .filter_map(|v| v.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        for id in ids {
            if !present.contains(id) {
                not_found.push(json!(id));
            }
        }
    }
    let state = account.acc.state().await.map_err(server_fail)?;
    Ok(json!({
        "accountId": account.account_id(),
        "state": state,
        "list": list,
        "notFound": not_found,
    }))
}

/// `SieveScript/set` — create/update/destroy scripts (compile-validated),
/// plus `onSuccessActivateScript` (an id, or `null` to deactivate all).
pub async fn set(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let old_state = account.acc.state().await.map_err(server_fail)?;

    let mut created = Map::new();
    let mut updated = Map::new();
    let mut destroyed = Vec::new();
    let mut not_created = Map::new();
    let mut not_updated = Map::new();
    let mut not_destroyed = Map::new();
    // creationId → script name, for resolving `#id` in onSuccessActivateScript.
    let mut created_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // create: { creationId: { name, content } }
    if let Some(obj) = args.get("create").and_then(Value::as_object) {
        for (cid, spec) in obj {
            let name = spec.get("name").and_then(Value::as_str);
            let content = spec.get("content").and_then(Value::as_str);
            let (Some(name), Some(content)) = (name, content) else {
                not_created.insert(
                    cid.clone(),
                    method_error_desc("invalidProperties", "name and content required"),
                );
                continue;
            };
            match account.acc.put_sieve_script(name, content).await {
                Ok(()) => {
                    created.insert(cid.clone(), json!({ "id": name, "isActive": false }));
                    created_names.insert(cid.clone(), name.to_owned());
                }
                Err(e) => {
                    not_created.insert(cid.clone(), set_error(&e));
                }
            }
        }
    }

    // update: { id: { content? } } (name is the id; content replaces).
    if let Some(obj) = args.get("update").and_then(Value::as_object) {
        for (id, patch) in obj {
            match patch.get("content").and_then(Value::as_str) {
                Some(content) => match account.acc.put_sieve_script(id, content).await {
                    Ok(()) => {
                        updated.insert(id.clone(), Value::Null);
                    }
                    Err(e) => {
                        not_updated.insert(id.clone(), set_error(&e));
                    }
                },
                None => {
                    // No content change (e.g. only isActive) → treat as no-op
                    // success; activation is handled by onSuccessActivateScript.
                    updated.insert(id.clone(), Value::Null);
                }
            }
        }
    }

    // destroy: [ ids ]
    if let Some(ids) = args.get("destroy").and_then(Value::as_array) {
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            match account.acc.delete_sieve_script(id).await {
                Ok(()) => destroyed.push(json!(id)),
                Err(e) => {
                    not_destroyed.insert(id.to_owned(), set_error(&e));
                }
            }
        }
    }

    // Activation (RFC 9661 §2.5, §3.3): a script id (or `#creationId` from
    // this same call) to activate, or JSON `null` to deactivate all. Applied
    // best-effort after the set — it never discards the set results, and a
    // failure is logged, not surfaced as a (mis-typed) method error.
    if let Some(field) = args.get("onSuccessActivateScript") {
        let raw = field.as_str();
        let target: Option<String> = match raw {
            Some(s) => match s.strip_prefix('#') {
                Some(cid) => created_names.get(cid).cloned(),
                None => Some(s.to_owned()),
            },
            None => None, // explicit null → deactivate
        };
        // Deactivate on null; activate a resolved target; skip an unresolved
        // `#creationId` (its create must have failed).
        if raw.is_none() || target.is_some() {
            if let Err(e) = account.acc.activate_sieve_script(target.as_deref()).await {
                tracing::warn!(error = %e, "SieveScript activation failed");
            }
        } else {
            tracing::warn!("onSuccessActivateScript references an unknown creation id");
        }
    }

    let new_state = account.acc.state().await.map_err(server_fail)?;
    Ok(json!({
        "accountId": account.account_id(),
        "oldState": old_state,
        "newState": new_state,
        "created": created,
        "updated": updated,
        "destroyed": destroyed,
        "notCreated": not_created,
        "notUpdated": not_updated,
        "notDestroyed": not_destroyed,
    }))
}

/// `SieveScript/validate` — compile-check content without storing.
pub async fn validate(account: &Account, args: &Value) -> Result<Value, Value> {
    check_account(args, account)?;
    let content = args.get("content").and_then(Value::as_str).unwrap_or("");
    let (is_valid, error) = match alo_sieve::compile(content, alo_sieve::Limits::default()) {
        Ok(_) => (true, Value::Null),
        Err(e) => (false, json!(e.to_string())),
    };
    Ok(json!({
        "accountId": account.account_id(),
        "isValid": is_valid,
        "errorDescription": error,
    }))
}

fn check_account(args: &Value, account: &Account) -> Result<(), Value> {
    match args.get("accountId").and_then(Value::as_str) {
        Some(id) if id == account.account_id() => Ok(()),
        _ => Err(method_error("accountNotFound")),
    }
}

fn server_fail(_e: StoreError) -> Value {
    method_error("serverFail")
}

fn set_error(e: &StoreError) -> Value {
    match e {
        StoreError::NotFound => method_error("notFound"),
        // A compile failure is surfaced as invalidScript (RFC 9661 §3.2).
        StoreError::Conflict(msg) => method_error_desc("invalidScript", msg),
        _ => method_error("serverFail"),
    }
}
