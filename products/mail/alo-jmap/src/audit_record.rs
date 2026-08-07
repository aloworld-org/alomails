//! The middleware that writes the business audit trail (ADR 0035, wave B2.13).
//!
//! One layer over the whole router, so that **every** successful mutation of a
//! billing or CRM record leaves exactly one entry, without a single handler
//! knowing the audit log exists. [`crate::audit_action`] decides what the entry
//! says; this module decides when one is written and where the record's id
//! comes from.
//!
//! Four rules, each of them a thing that would otherwise be got wrong once per
//! handler:
//!
//! - **Only successes are recorded.** A refused write changed nothing, and a
//!   log that files refusals as events makes a record's history a list of
//!   things that did not happen. (The refusals themselves are still visible in
//!   the service log; they are just not part of the record's story.)
//! - **The entry is written after the response is produced**, never in the same
//!   transaction as the change. An audit failure must not undo the act it
//!   describes, so a failed write is logged and swallowed.
//! - **A create's id is read from the response**, because it exists nowhere
//!   else at that point: the route was `POST /billing/invoices`, and the
//!   invoice's id was minted inside the handler.
//! - **The actor comes from the bearer token**, exactly as the handler's own
//!   authentication did. The request body never names the actor, so a body that
//!   claims to be someone else changes nothing.

use axum::body::{Body, Bytes};
use axum::extract::{MatchedPath, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::Response;
use serde_json::Value;

use crate::audit_action::{self, AuditEvent};
use crate::state::{AppState, bearer_token};

/// Records one audit entry per successful billing/CRM mutation.
///
/// Applied with [`axum::middleware::from_fn_with_state`] to the whole router:
/// it runs after routing (so the matched route template is available) and short
/// circuits — with the response untouched — for everything that is not an
/// audited mutation, which is the overwhelming majority of traffic.
pub async fn audit_business_mutations(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !audit_action::is_mutating(request.method().as_str()) {
        return next.run(request).await;
    }
    let method = request.method().as_str().to_owned();
    // The route template, not the request path: `/billing/invoices/{id}/issue`
    // is what says which segment is an id. Without it there is nothing to
    // derive an entry from, so nothing is recorded (a 404 never gets this far —
    // the layer is applied to the routes, not around the router).
    let Some(template) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
    else {
        return next.run(request).await;
    };
    let path = request.uri().path().to_owned();
    let Some(event) = audit_action::event_for(&method, &template, &path) else {
        return next.run(request).await;
    };
    let token = bearer_token(request.headers());

    let response = next.run(request).await;
    if !response.status().is_success() {
        return response;
    }
    let Some(token) = token else {
        return response;
    };
    let (response, entity_id) = match event.entity_id.clone() {
        Some(id) => (response, Some(id)),
        None => created_id_from(response).await,
    };
    write_entry(&state, &token, &event, entity_id.as_deref(), &path).await;
    response
}

/// Resolves the bearer token and appends the entry. Best effort by contract:
/// the act it describes has already happened and answered the client, so a
/// failure here is logged (never with the token, the body, or the store's error
/// text at any level a tenant can read) and the request still succeeds.
async fn write_entry(
    state: &AppState,
    token: &str,
    event: &AuditEvent,
    entity_id: Option<&str>,
    path: &str,
) {
    let principal = match state.identity.resolve_access_token(token).await {
        Ok(Some(principal)) => principal,
        // The handler answered 2xx, so the token resolved a moment ago; this is
        // a race with a revocation or a store blip, not a normal path.
        Ok(None) => return,
        Err(_) => {
            tracing::warn!(action = %event.action, "audit entry skipped: token unresolved");
            return;
        }
    };
    if let Err(error) = state
        .store
        .record_entity_audit(
            &principal.tenant,
            Some(&principal.user),
            &event.action,
            &event.entity_type,
            entity_id,
            Some(path),
        )
        .await
    {
        tracing::error!(action = %event.action, %error, "audit entry not written");
    }
}

/// Buffers a JSON response to read the id of the record it just created, and
/// hands the response back unchanged.
///
/// Only 2xx JSON answers from this service's own handlers reach here, and those
/// are built in memory as a whole (`Json<Value>`), so collecting the body is a
/// move rather than a read — which is why it is collected without a size cap:
/// a cap could only ever be hit by a body that is already resident, and
/// truncating it would send the client something other than what the handler
/// wrote. A non-JSON answer (the SEPA file) is passed straight through and its
/// entry simply carries no record id.
async fn created_id_from(response: Response) -> (Response, Option<String>) {
    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_json {
        return (response, None);
    }
    let (parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        // Unreachable for an in-memory body; rebuilt empty rather than panicking
        // in a layer that must never take a request down.
        tracing::error!("audited response body could not be buffered");
        return (Response::from_parts(parts, Body::empty()), None);
    };
    let id = created_id(&bytes);
    (Response::from_parts(parts, Body::from(bytes)), id)
}

/// The id of the record a create answered with.
///
/// Two shapes are in use across billing and CRM and both are accepted: the bare
/// `{"id": …}` and the wrapped `{"deal": {"id": …}}` that most handlers answer
/// with so the client gets the canonical record back. Anything else — a report,
/// a status envelope, a list — has no single record to point at and yields
/// `None`, which is an entry without a record id rather than a wrong one.
fn created_id(body: &Bytes) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        return Some(id.to_owned());
    }
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    let (_, only) = object.iter().next()?;
    only.get("id").and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn id_of(body: &str) -> Option<String> {
        created_id(&Bytes::from(body.to_owned()))
    }

    #[test]
    fn both_create_response_shapes_yield_the_new_id() {
        assert_eq!(
            id_of(r#"{"id":"inv-1","total":0}"#).as_deref(),
            Some("inv-1")
        );
        assert_eq!(
            id_of(r#"{"deal":{"id":"deal-1","title":"x"}}"#).as_deref(),
            Some("deal-1")
        );
    }

    #[test]
    fn an_answer_that_is_not_one_record_yields_no_id() {
        assert!(id_of(r#"{"entries":[{"id":"a"}]}"#).is_none());
        assert!(id_of(r#"{"imported":3,"skipped":{"id":"x"},"report":[]}"#).is_none());
        assert!(id_of(r#"{"status":"ok"}"#).is_none());
        assert!(id_of("not json at all").is_none());
    }
}
