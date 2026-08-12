//! The middleware behind the admin console's per-user app switches
//! (migration 0208; `platform/alo-store/src/user_modules.rs`).
//!
//! An administrator decides which apps each person gets, and the rail shows
//! only those. Hiding a rail entry is not access control — the URL is still
//! typeable and the API is still callable with curl — so the same decision is
//! enforced here, at the one place every request to a module's routes passes
//! through:
//!
//! > A caller whose admin has switched off a module may not call that module's
//! > routes at all, by any method.
//!
//! One layer rather than a gate in three hundred handlers, for the reason
//! [`crate::scoped_roles`] is one layer: the handler added next month by
//! somebody who never read this file is the hole.
//!
//! # What it deliberately does not do
//!
//! - **It does not authenticate.** A request with no token or a dead one is
//!   passed through to the handler, which answers the `401` it always did.
//!   One place decides what an unauthenticated request is told.
//! - **It does not refuse admins.** [`Account::may_open`] passes a tenant
//!   admin unconditionally: the switch lives in the console, and an admin who
//!   had switched their own console away would have no way back.
//! - **It does not grant.** A module that is switched on still answers to
//!   whatever gate it always had — `require_finance`, Space membership, the
//!   HR role. This narrows and only narrows.
//! - **It does not touch mail, the session, or anything shared.** Only the
//!   prefixes in [`MODULE_OF_PREFIX`] are gated. `/jmap`, `/settings`,
//!   `/contacts`, `/spaces`, `/admin` and the identity routes are either the
//!   account itself or the plumbing every app needs, and switching one off
//!   would not read as "no app" — it would read as a broken login.
//!
//! # Why the matched template and not the raw path
//!
//! `MatchedPath` is the route template axum resolved (`/billing/invoices/{id}`),
//! so the first segment is the module by construction. Reading the raw URI
//! would make `/billing/../jmap` and percent-encoding this file's problem.

use axum::extract::{MatchedPath, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use alo_store::AppModule;

use crate::error::Problem;
use crate::state::{AppState, authenticate};

/// Route prefix → the module whose switch governs it.
///
/// Every entry is a prefix this service actually serves, and every value is a
/// module the store's CHECK knows. The two lists are kept in step by
/// [`tests::every_gated_prefix_names_a_module_the_store_knows`].
///
/// `calendar` maps to `agenda` because the API prefix and the rail id were
/// named at different times — the store speaks the rail's vocabulary, which is
/// what the admin console shows.
const MODULE_OF_PREFIX: [(&str, AppModule); 13] = [
    ("billing", AppModule::Billing),
    ("calendar", AppModule::Agenda),
    ("chat", AppModule::Chat),
    ("crm", AppModule::Crm),
    ("drive", AppModule::Drive),
    ("finance", AppModule::Finance),
    ("hr", AppModule::Hr),
    ("insights", AppModule::Insights),
    ("inventory", AppModule::Inventory),
    ("meet", AppModule::Meet),
    ("projects", AppModule::Projects),
    ("sites", AppModule::Sites),
    ("tasks", AppModule::Tasks),
];

/// The first path segment of a matched route template, e.g. `billing` for
/// `/billing/invoices/{id}/issue`.
fn prefix_of(template: &str) -> Option<&str> {
    // The whole API is mounted twice — at its own paths and again under
    // `/api` — so the first segment is `api` for half the traffic. Skipping it
    // makes both mounts answer to the same rule; without this, every gated
    // module would read as the module called "api", which is none of them, and
    // the gate would wave everything through.
    template
        .split('/')
        .filter(|segment| !segment.is_empty())
        .find(|segment| *segment != "api")
}

/// The module a route template belongs to, or `None` when it is not gated.
fn module_of(template: &str) -> Option<AppModule> {
    let prefix = prefix_of(template)?;
    MODULE_OF_PREFIX
        .iter()
        .find(|(name, _)| *name == prefix)
        .map(|(_, module)| *module)
}

/// Refuses a module's routes to a caller whose admin switched that app off.
///
/// Applied with [`axum::middleware::from_fn_with_state`] to the routes (not
/// around the router), so it runs *after* routing and can read the matched
/// template. It short circuits before touching the store for every request to
/// an ungated prefix — mail, the session, uploads, the event stream — which is
/// most traffic.
pub async fn enforce_module_access(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(template) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
    else {
        return next.run(request).await;
    };
    let Some(module) = module_of(&template) else {
        return next.run(request).await;
    };
    // Only now — a request into a gated module — is the caller worth a store
    // read. An unresolvable token is the handler's `401` to give.
    let Ok(account) = authenticate(&state, request.headers()).await else {
        return next.run(request).await;
    };
    if !account.may_open(module) {
        // Says which app and says who can undo it. A bare "forbidden" here
        // reads as a bug to the person it happens to, because every other app
        // in their rail works.
        return Problem::with(
            StatusCode::FORBIDDEN,
            format!(
                "{module} is switched off for this account — a workspace \
                 administrator can switch it back on"
            ),
        )
        .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::{MODULE_OF_PREFIX, module_of, prefix_of};
    use alo_store::{ALL_MODULES, AppModule};

    #[test]
    fn the_prefix_is_the_first_segment_of_the_template() {
        assert_eq!(prefix_of("/billing/invoices/{id}/issue"), Some("billing"));
        assert_eq!(prefix_of("/hr"), Some("hr"));
        assert_eq!(prefix_of("/"), None);
        assert_eq!(prefix_of(""), None);
    }

    #[test]
    fn the_api_mount_gates_exactly_as_the_bare_path_does() {
        // The API answers at two addresses. A module switched off must be
        // refused at both, and the gate reads the first path segment — which
        // is `api` for half of them. Getting this wrong fails open: every
        // request would look like a module called "api", which no switch names.
        for (bare, under_api) in [
            ("/billing/invoices", "/api/billing/invoices"),
            ("/chat/rooms", "/api/chat/rooms"),
            ("/calendar/events", "/api/calendar/events"),
        ] {
            assert_eq!(module_of(bare), module_of(under_api), "{under_api}");
            assert!(module_of(under_api).is_some(), "{under_api} must still gate");
        }
        // And an ungated prefix stays ungated under the mount.
        assert_eq!(module_of("/api/jmap/upload/{accountId}"), None);
        assert_eq!(module_of("/api"), None);
    }

    #[test]
    fn a_gated_prefix_resolves_to_its_module() {
        assert_eq!(module_of("/billing/invoices"), Some(AppModule::Billing));
        assert_eq!(module_of("/drive/files/{id}"), Some(AppModule::Drive));
        // The API says calendar, the rail and the console say agenda.
        assert_eq!(module_of("/calendar/events"), Some(AppModule::Agenda));
    }

    #[test]
    fn the_account_and_its_plumbing_are_never_gated() {
        // Switching one of these off would not read as "no app" to the person
        // it happened to — it would read as a broken login.
        for template in [
            "/jmap/upload/{accountId}",
            "/jmap/eventsource",
            "/settings/mail",
            "/contacts",
            "/spaces/{id}/members",
            "/admin/users/admin",
            "/.well-known/jmap",
            "/signup",
        ] {
            assert_eq!(module_of(template), None, "{template} must not be gated");
        }
    }

    #[test]
    fn every_gated_prefix_names_a_module_the_store_knows() {
        // The store's CHECK and this table are two lists of the same set. A
        // module here that the store refuses to store would be a switch the
        // console could never write; one the store knows and this table omits
        // would be a switch that silently did nothing.
        for (_, module) in MODULE_OF_PREFIX {
            assert!(
                ALL_MODULES.contains(&module),
                "{module} unknown to the store"
            );
        }
        for module in ALL_MODULES {
            assert!(
                MODULE_OF_PREFIX.iter().any(|(_, m)| *m == module),
                "{module} is switchable but no route prefix enforces it"
            );
        }
    }
}
