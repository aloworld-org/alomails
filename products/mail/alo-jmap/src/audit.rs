//! `GET /audit` — one business record's history (ADR 0035, wave B2.13).
//!
//! The cross-module read that makes the trail worth writing: given a record —
//! an invoice, a deal, a customer — answer *what has been done to it, by whom,
//! and when*, in the order it happened. The web surfaces it as a tab on the
//! record itself, which is the only place the question is ever actually asked.
//!
//! Since ADR 0058 the answer merges two sources: the audit entries the route
//! middleware writes for a person's clicks, and the tenant's event stream,
//! where every intent execution an agent runs lands (A4.6). One act is in
//! exactly one of them, so the merged history never shows a change twice —
//! and an agent's `send_quote` finally shows on the quote it sent.
//!
//! Deliberately **not** admin-only. `/admin/audit` is the tenant-wide
//! administrative log and stays behind the admin gate; this is the history of a
//! record the caller can already open and edit, and hiding "who changed this
//! invoice" from the people working on it would only push them to ask each
//! other in chat. Tenancy does the entire job: the store's read is bound to the
//! caller's tenant, so another tenant's record id is an empty history, exactly
//! like an id that was never issued — never a `404` that confirms it exists.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use alo_store::{AuditEntry, DomainEvent};

use crate::billing::iso;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// How many entries one record's history answers with when the caller asks for
/// no particular number. Records accumulate slowly (a handful of events a
/// month), so this is a whole life for almost every one of them.
const DEFAULT_LIMIT: i64 = 100;

/// The ceiling on `limit`, mirrored from the store's own clamp so the published
/// contract and the query agree.
const MAX_LIMIT: i64 = 500;

#[derive(Deserialize)]
pub struct AuditQuery {
    /// `entityType:entityId` — the record to read, e.g.
    /// `billing.invoice:4f2c…`. One parameter rather than two because it is one
    /// address: half of it is never useful.
    entity: Option<String>,
    limit: Option<i64>,
}

/// Splits `entityType:entityId` into its halves, rejecting anything that is not
/// one addressable record.
///
/// The type is checked against the shape the audit vocabulary actually uses
/// (`module.record`, lowercase) rather than passed through: a value from a
/// query string reaching a `WHERE` clause unexamined is the habit that costs
/// you eventually, even through a bound parameter.
fn parse_entity(raw: &str) -> Option<(String, String)> {
    let (entity_type, entity_id) = raw.split_once(':')?;
    let type_ok = !entity_type.is_empty()
        && entity_type.len() <= 64
        && entity_type
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
        && entity_type.contains('.');
    let id_ok = !entity_id.is_empty() && entity_id.len() <= 128;
    (type_ok && id_ok).then(|| (entity_type.to_owned(), entity_id.to_owned()))
}

/// One entry as JSON. `actor` is the acting user's address when they are still
/// a user of the tenant, else whatever label was recorded — never a raw user
/// id, which names nobody to the person reading the tab.
fn entry_json(entry: &AuditEntry) -> Value {
    json!({
        "id": entry.id,
        "action": entry.action,
        "actor": entry.actor,
        "agent": Value::Null,
        "entityType": entry.entity_type,
        "entityId": entry.entity_id,
        "target": entry.target,
        "detail": entry.detail,
        "at": iso(entry.created_at),
    })
}

/// One event of the record's stream, in the entry shape the tab already
/// reads — `action` is the verb that ran, and `agent` names the agent that
/// ran it (a person's own palette run has none). The entity address is the
/// one the caller asked with, because every item of the answer is about that
/// record.
fn event_json(event: &DomainEvent, entity_type: &str, entity_id: &str) -> Value {
    json!({
        "id": event.id.as_str(),
        "action": event.kind,
        "actor": event.actor,
        "agent": event.agent,
        "entityType": entity_type,
        "entityId": entity_id,
        "target": Value::Null,
        "detail": Value::Null,
        "at": iso(event.created_at),
    })
}

/// `GET /audit?entity=billing.invoice:<id>&limit=<n>` →
/// `{ "entries": [ … ] }`, newest first.
///
/// # Errors
/// `401` without a valid bearer token; `422` when `entity` is missing or is not
/// a `type:id` pair.
pub async fn list_entity_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Value>, Problem> {
    let account = authenticate(&state, &headers).await?;
    let Some((entity_type, entity_id)) = query.entity.as_deref().and_then(parse_entity) else {
        return Err(Problem::with(
            StatusCode::UNPROCESSABLE_ENTITY,
            "entity must be given as \"entityType:entityId\", e.g. billing.invoice:abc123",
        ));
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let tenant = state.store.for_tenant(account.tenant.clone());
    let entries = tenant
        .list_entity_audit(&entity_type, &entity_id, limit)
        .await
        .map_err(|_| Problem::server_error())?;
    // The record's history has two sources until wave A8 unifies them: the
    // route middleware's entries (a person's clicks) and the event stream (an
    // agent's executions, ADR 0058 §5). One act lands in exactly one of them,
    // so the merge never shows a change twice.
    let events = tenant
        .list_record_events(&entity_type, &entity_id, limit)
        .await
        .map_err(|_| Problem::server_error())?;
    let mut merged: Vec<(time::OffsetDateTime, Value)> = entries
        .iter()
        .map(|entry| (entry.created_at, entry_json(entry)))
        .chain(events.iter().map(|event| {
            (
                event.created_at,
                event_json(event, &entity_type, &entity_id),
            )
        }))
        .collect();
    merged.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    merged.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let list: Vec<Value> = merged.into_iter().map(|(_, item)| item).collect();
    Ok(Json(json!({ "entries": list })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::parse_entity;

    #[test]
    fn a_record_address_is_a_type_and_an_id() {
        let (kind, id) = parse_entity("billing.invoice:abc123").expect("parsed");
        assert_eq!(kind, "billing.invoice");
        assert_eq!(id, "abc123");
        // The id keeps everything after the first colon; ids never contain one,
        // but splitting on the last would silently address a different record.
        let (kind, id) = parse_entity("crm.deal:a:b").expect("parsed");
        assert_eq!(kind, "crm.deal");
        assert_eq!(id, "a:b");
    }

    #[test]
    fn anything_that_is_not_one_record_is_refused() {
        assert!(parse_entity("billing.invoice").is_none());
        assert!(parse_entity(":abc").is_none());
        assert!(parse_entity("billing.invoice:").is_none());
        assert!(
            parse_entity("invoice:abc").is_none(),
            "a type needs a module"
        );
        assert!(parse_entity("billing.invoice';DROP--:abc").is_none());
        assert!(parse_entity("Billing.Invoice:abc").is_none());
        assert!(parse_entity(&format!("billing.invoice:{}", "x".repeat(129))).is_none());
    }
}
