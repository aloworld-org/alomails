//! The middleware that keeps a scoped role scoped (ADR 0035, wave B4.12;
//! `docs/design/finance.md`, "The accountant role").
//!
//! An accountant must be able to **see** the document behind a posting — the
//! invoice a receivable came from, the deal a quote was raised on — and must
//! not be able to change it. That is a read/write distinction across two whole
//! modules, and there is exactly one place in this service where every
//! `/billing` and `/crm` write passes through: a layer over the router, the
//! same trick the audit trail (B2.13) uses one line below.
//!
//! Sixty handlers with a gate each would be sixty chances to forget, and the
//! sixty-first — added next month by somebody who never read this file — would
//! be the hole. So the rule lives here, stated once:
//!
//! > A caller holding **only** the accountant role may not use a mutating
//! > method on `/billing/*` or `/crm/*`.
//!
//! Four things it deliberately does not do:
//!
//! - **It does not authenticate.** A request with no token or a dead one is
//!   passed straight through to the handler, which answers the `401` it always
//!   did — one place decides what an unauthenticated request is told, and it is
//!   not this one.
//! - **It does not refuse admins.** An admin who is also an accountant is an
//!   admin; the role only ever adds.
//! - **It does not refuse dry runs.** `POST /crm/imports/leads/preview` writes
//!   nothing, and refusing a reader permission to look would be a rule about
//!   the HTTP method rather than about the data
//!   ([`crate::audit_action::writes_nothing`], one list, shared).
//! - **It does not touch `/finance/*`.** Those routes gate themselves, per
//!   route, on `Account::require_finance` — an accountant *writes* there, and a
//!   blanket rule at this layer could not tell the period lock from a rate
//!   table.

use axum::extract::{MatchedPath, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use alo_store::TenantRole;

use crate::audit_action;
use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// The modules a scoped reader may read and may not write. `/finance` is
/// absent on purpose (see the module docs); mail, calendar, drive, tasks and
/// the rest are an accountant's own data and are governed by owning it.
const READ_ONLY_FOR_ACCOUNTANT: [&str; 2] = ["billing", "crm"];

/// The first path segment of a matched route template, e.g. `billing` for
/// `/billing/invoices/{id}/issue`.
fn module_of(template: &str) -> Option<&str> {
    template.split('/').find(|segment| !segment.is_empty())
}

/// Refuses billing and CRM writes to a caller whose only access is a scoped
/// role.
///
/// Applied with [`axum::middleware::from_fn_with_state`] to the routes (not
/// around the router), so it runs *after* routing and can read the matched
/// template. It short circuits before touching the store for every request
/// that is not a mutating call into one of the read-only modules — which is
/// nearly all traffic, including every `GET`.
pub async fn enforce_scoped_roles(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !audit_action::is_mutating(request.method().as_str()) {
        return next.run(request).await;
    }
    let Some(template) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
    else {
        return next.run(request).await;
    };
    if !module_of(&template).is_some_and(|module| READ_ONLY_FOR_ACCOUNTANT.contains(&module))
        || audit_action::writes_nothing(&template)
    {
        return next.run(request).await;
    }
    // Only now — a mutating call into a read-only module — is the caller worth
    // a store read. An unresolvable token is the handler's `401` to give.
    let Ok(account) = authenticate(&state, request.headers()).await else {
        return next.run(request).await;
    };
    if !account.is_admin && account.has_role(TenantRole::Accountant) {
        return Problem::with(
            StatusCode::FORBIDDEN,
            "an accountant may read billing and CRM, not change them",
        )
        .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::{READ_ONLY_FOR_ACCOUNTANT, module_of};

    #[test]
    fn the_module_is_the_first_segment_of_the_template() {
        assert_eq!(module_of("/billing/invoices/{id}/issue"), Some("billing"));
        assert_eq!(module_of("/crm/deals"), Some("crm"));
        assert_eq!(module_of("/finance/periods/{id}/close"), Some("finance"));
        assert_eq!(module_of("/"), None);
        assert_eq!(module_of(""), None);
    }

    #[test]
    fn finance_is_not_a_read_only_module_for_this_gate() {
        assert!(!READ_ONLY_FOR_ACCOUNTANT.contains(&"finance"));
        assert!(READ_ONLY_FOR_ACCOUNTANT.contains(&"billing"));
        assert!(READ_ONLY_FOR_ACCOUNTANT.contains(&"crm"));
    }
}
