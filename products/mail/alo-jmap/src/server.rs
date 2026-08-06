//! The axum router (JMAP methods + the mounted OIDC provider), the
//! non-public first-party `/auth/token` password grant, and `serve`.

use std::net::SocketAddr;
use std::sync::Arc;

use alo_identity::Identity;
use alo_store::Store;
use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::push::PushHub;
use crate::state::{AppState, Limits};
use crate::{
    admin, agent, ai, api, autoconfig, base, blob, calendar, carddav, contacts, delegates, docs, drive,
    filters, flagdue, imap_import_route, push, reset_route, schedule, security, session, settings,
    share, signup_route, snooze, spaces, tasks, unsubscribe, wopi, workspace_search,
};

/// Builds the JMAP router over the given state. The OpenID Connect /
/// OAuth 2.0 provider (`alo-identity`) is mounted alongside so a Phase-1
/// deployment serves JMAP and the IdP from one HTTP service.
pub fn app(state: AppState) -> Router {
    let upload_limit = state.limits.max_size_upload as usize;
    let request_limit = state.limits.max_size_request;
    let identity_routes = alo_identity::router(state.identity.clone());
    let jmap = Router::new()
        .route("/.well-known/jmap", get(session::session))
        // The API route caps at maxSizeRequestObject; uploads get the
        // larger ceiling from the global layer below.
        .route(
            "/jmap/api",
            post(api::api).layer(DefaultBodyLimit::max(request_limit)),
        )
        .route("/auth/token", post(token))
        .route("/jmap/upload/{accountId}", post(blob::upload))
        .route(
            "/jmap/download/{accountId}/{blobId}/{name}",
            get(blob::download),
        )
        .route("/jmap/eventsource", get(push::event_source))
        // AI inference (ADR 0011) — authenticated, tenant-scoped. Its own small
        // body limit: the draft cap, not the large blob-upload ceiling below.
        .route(
            "/ai/improve",
            post(ai::improve).layer(DefaultBodyLimit::max(ai::MAX_IMPROVE_BYTES)),
        )
        .route(
            "/ai/summarize",
            post(ai::summarize).layer(DefaultBodyLimit::max(ai::MAX_SUMMARIZE_BYTES)),
        )
        .route(
            "/ai/replies",
            post(ai::replies).layer(DefaultBodyLimit::max(ai::MAX_SUMMARIZE_BYTES)),
        )
        .route(
            "/ai/extract-tasks",
            post(ai::extract_tasks).layer(DefaultBodyLimit::max(ai::MAX_SUMMARIZE_BYTES)),
        )
        // Ask across the workspace (ADR 0029): cited answers over access-scoped
        // retrieval of files, tasks, and mail.
        .route(
            "/ai/ask",
            post(ai::ask).layer(DefaultBodyLimit::max(ai::MAX_ASK_BYTES)),
        )
        // Document AI (ADR 0029 §3): propose text for the alo Doc editor to
        // apply only on approval.
        .route(
            "/ai/compose",
            post(ai::compose).layer(DefaultBodyLimit::max(ai::MAX_SUMMARIZE_BYTES)),
        )
        // "Ask alo" agent (ADR 0034): answer OR propose one action; a separate,
        // approval-gated route is the only one that executes.
        .route(
            "/ai/agent",
            post(agent::agent).layer(DefaultBodyLimit::max(ai::MAX_ASK_BYTES)),
        )
        .route(
            "/ai/agent/execute",
            post(agent::agent_execute).layer(DefaultBodyLimit::max(ai::MAX_ASK_BYTES)),
        )
        // Snooze: hide conversations until a chosen time (a background sweeper wakes them).
        .route("/snooze", post(snooze::snooze))
        // Send later: hold a draft until a chosen time (a background sweeper sends it).
        .route("/send-later", post(schedule::send_later))
        .route("/send-later/cancel", post(schedule::cancel_send))
        // Recent correspondents for compose recipient autocomplete.
        .route(
            "/calendar/events",
            get(calendar::list).post(calendar::create),
        )
        .route(
            "/calendar/events/{id}",
            get(calendar::get_one)
                .put(calendar::update)
                .delete(calendar::delete),
        )
        .route("/calendar/rsvp", post(calendar::rsvp))
        .route("/calendar/cancel", post(calendar::cancel))
        // Record a guest's reply on the organizer's event (attendee PARTSTAT).
        .route("/calendar/apply-reply", post(calendar::apply_reply))
        .route(
            "/calendar/calendars",
            get(calendar::list_calendars).post(calendar::create_calendar),
        )
        .route(
            "/calendar/calendars/{id}",
            put(calendar::rename_calendar).delete(calendar::remove_calendar),
        )
        // Calendar sharing (slice 2): grant/list/revoke access on a calendar.
        .route(
            "/calendar/calendars/{id}/grants",
            get(calendar::list_grants)
                .post(calendar::share_calendar)
                .delete(calendar::unshare_calendar),
        )
        // Groups the caller can share a calendar with (team access).
        .route("/calendar/groups", get(calendar::list_shareable_groups))
        // Free/busy: when are these people (in the tenant) busy?
        .route("/calendar/freebusy", post(calendar::free_busy))
        // Tasks (ADR 0021–0023). Rows out; the client groups into board/list.
        .route(
            "/tasks/projects",
            get(tasks::list_projects).post(tasks::create_project),
        )
        .route("/tasks", get(tasks::list_tasks).post(tasks::create_task))
        // "What's on my plate", the calendar's due-task overlay, and AI proposals
        // (static paths before /tasks/{id}).
        .route("/tasks/today", get(tasks::my_plate))
        .route("/tasks/due", get(tasks::due_tasks))
        .route("/tasks/files", get(tasks::project_files))
        .route("/tasks/dependencies", get(tasks::project_dependencies))
        .route(
            "/tasks/labels",
            get(tasks::list_labels).post(tasks::create_label),
        )
        .route(
            "/tasks/labels/{id}",
            axum::routing::delete(tasks::delete_label),
        )
        .route("/tasks/proposals", get(tasks::list_proposals))
        .route("/tasks/propose", post(tasks::propose_tasks))
        .route(
            "/tasks/{id}",
            get(tasks::get_task)
                .put(tasks::update_task)
                .delete(tasks::delete_task),
        )
        .route("/tasks/{id}/move", post(tasks::move_task))
        .route("/tasks/{id}/accept", post(tasks::accept_task))
        .route("/tasks/{id}/reject", post(tasks::reject_task))
        .route("/tasks/{id}/subtasks", post(tasks::add_subtask))
        .route(
            "/tasks/{id}/subtasks/{sid}",
            put(tasks::set_subtask).delete(tasks::delete_subtask),
        )
        .route("/tasks/{id}/comments", post(tasks::add_comment))
        .route(
            "/tasks/{id}/followers",
            post(tasks::follow_task).delete(tasks::unfollow_task),
        )
        .route(
            "/tasks/{id}/attachments",
            get(tasks::list_attachments).post(tasks::add_attachment),
        )
        .route(
            "/tasks/{id}/attachments/{aid}",
            axum::routing::delete(tasks::delete_attachment),
        )
        .route(
            "/tasks/{id}/attachments/{aid}/download",
            get(tasks::download_attachment),
        )
        .route("/tasks/{id}/labels", post(tasks::add_task_label))
        .route(
            "/tasks/{id}/labels/{lid}",
            axum::routing::delete(tasks::remove_task_label),
        )
        .route("/tasks/{id}/dependencies", post(tasks::add_dependency))
        .route(
            "/tasks/{id}/dependencies/{dep}",
            axum::routing::delete(tasks::remove_dependency),
        )
        // Spaces — the membership spine (ADR 0026). Static paths before /{id}.
        .route(
            "/spaces",
            get(spaces::list_spaces).post(spaces::create_space),
        )
        .route(
            "/spaces/{id}",
            get(spaces::get_space).put(spaces::update_space),
        )
        .route("/spaces/{id}/members", post(spaces::add_member))
        .route(
            "/spaces/{id}/members/{uid}",
            axum::routing::delete(spaces::remove_member),
        )
        .route("/spaces/{id}/modules", post(spaces::set_module))
        // Drive — the file tree (ADR 0027). Static paths before /nodes/{id}.
        .route("/drive/list", get(drive::list))
        .route("/drive/trash", get(drive::trash))
        .route("/drive/folders", post(drive::create_folder))
        .route("/drive/files", post(drive::create_file))
        .route(
            "/drive/nodes/{id}",
            get(drive::get_node).put(drive::rename).delete(drive::purge),
        )
        .route("/drive/nodes/{id}/move", post(drive::move_node))
        .route("/drive/nodes/{id}/copy", post(drive::copy_node))
        .route("/drive/nodes/{id}/trash", post(drive::trash_node))
        .route("/drive/nodes/{id}/restore", post(drive::restore_node))
        .route(
            "/drive/nodes/{id}/versions",
            get(drive::versions).post(drive::add_version),
        )
        .route(
            "/drive/nodes/{id}/versions/{no}/restore",
            post(drive::restore_version),
        )
        .route("/drive/nodes/{id}/download", get(drive::download))
        // Office editing (Collabora over WOPI, ADR 0010): mint a token, then the
        // token-authed WOPI host endpoints Collabora calls to load/save bytes.
        .route("/drive/nodes/{id}/office", get(wopi::office_token))
        .route("/wopi/files/{id}", get(wopi::check_file_info))
        .route(
            "/wopi/files/{id}/contents",
            get(wopi::get_file).post(wopi::put_file),
        )
        // alo Base (ADR 0032) — relational data under a base drive node. Distinct
        // literal prefixes so a node-id param never collides with a literal.
        .route("/drive/base", post(base::create_base))
        .route("/drive/base/{node}", get(base::get_base))
        .route("/drive/base/{node}/tables", post(base::add_table))
        .route("/drive/base-tables/{table}/fields", post(base::add_field))
        .route("/drive/base-tables/{table}/records", post(base::add_record))
        .route("/drive/base-tables/{table}/views", post(base::add_view))
        .route(
            "/drive/base-records/{record}",
            put(base::update_record).delete(base::delete_record),
        )
        // Workspace search (ADR 0029): files + tasks by name/title.
        .route("/search", get(workspace_search::search))
        .route("/contacts", get(contacts::list))
        // Address-book import (a .vcf upload) and export (whole book as .vcf).
        .route("/contacts/import", post(contacts::import))
        .route("/contacts/export", get(contacts::export))
        // Import wizard: pull recent mail from a remote IMAP host (Gmail/
        // Outlook/…) into the user's Inbox.
        .route("/import/imap", post(imap_import_route::import))
        // Mail-client autoconfiguration (unauthenticated, public settings
        // only): a mail app configures itself from just an email address.
        // Mozilla format (Thunderbird / Apple Mail) at the .well-known path
        // and the autoconfig-subdomain path; Microsoft POX (Outlook) at the
        // autodiscover path. Operator DNS wiring is in the deploy README.
        .route(
            "/.well-known/autoconfig/mail/config-v1.1.xml",
            get(autoconfig::mozilla),
        )
        .route("/mail/config-v1.1.xml", get(autoconfig::mozilla))
        // Outlook varies the casing of this path; register both forms since
        // axum routing is case-sensitive.
        // Self-service personal signup (ADR 0018): unauthenticated, rate-
        // limited; provisions an account only after the recovery-email code
        // is verified.
        .route("/signup/domains", get(signup_route::domains))
        .route("/signup/available", post(signup_route::available))
        .route("/signup/begin", post(signup_route::begin))
        .route("/signup/verify", post(signup_route::verify))
        .route("/reset/request", post(reset_route::request))
        .route("/reset/verify", post(reset_route::verify))
        .route(
            "/autodiscover/autodiscover.xml",
            get(autoconfig::outlook).post(autoconfig::outlook),
        )
        .route(
            "/Autodiscover/Autodiscover.xml",
            get(autoconfig::outlook).post(autoconfig::outlook),
        )
        // CardDAV (RFC 6352): native contact sync for phones and desktops.
        // Any WebDAV method routes to the one handler, which dispatches by
        // method + path; well-known bootstraps discovery.
        .route("/.well-known/carddav", any(carddav::well_known))
        .route("/.well-known/caldav", any(carddav::well_known))
        .route("/dav", any(carddav::handle))
        .route("/dav/", any(carddav::handle))
        .route("/dav/{*rest}", any(carddav::handle))
        // alo Transfer: upload a large file (authenticated) for an expiring
        // link, and the PUBLIC download route the recipient's link points at.
        // The upload streams straight to storage, so its body limit is disabled
        // (there is no size cap); the handler never buffers the whole file.
        .route(
            "/share/upload",
            post(share::upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/share/{token}", get(share::download))
        // Server-side mail filters (rules) + one-click Block sender.
        .route("/filters", get(filters::list).put(filters::save))
        .route("/filters/block", post(filters::block))
        // RFC 8058 one-click unsubscribe (performed server-side, SSRF-guarded).
        .route("/jmap/unsubscribe", post(unsubscribe::unsubscribe))
        // Self-service mailbox sharing (ADR 0017): manage who can access YOUR
        // mailbox, no admin needed.
        .route(
            "/jmap/delegates",
            get(delegates::list).post(delegates::grant),
        )
        .route("/jmap/delegates/remove", post(delegates::revoke))
        // Flag follow-up due-date (set/clear a flagged message's due date).
        .route("/jmap/flag-due", post(flagdue::set_flag_due))
        // alo Docs (ADR 0015): tenant/owner-scoped technical-authoring documents.
        .route("/docs", get(docs::list).post(docs::create))
        .route(
            "/docs/{id}",
            get(docs::get)
                .put(docs::save)
                .delete(docs::delete)
                .layer(DefaultBodyLimit::max(docs::MAX_DOC_BYTES)),
        )
        // Admin console (tenant-admin only): AI provider management.
        .route(
            "/admin/ai/providers",
            get(admin::list_providers).post(admin::upsert_provider),
        )
        .route("/admin/ai/providers/default", post(admin::set_default))
        .route(
            "/admin/ai/providers/{id}",
            axum::routing::delete(admin::delete_provider),
        )
        .route("/admin/ai/test", post(admin::test_connection))
        // Admin console: users & mailboxes.
        .route(
            "/admin/users",
            get(admin::list_users).post(admin::create_user),
        )
        .route("/admin/users/password", post(admin::reset_password))
        .route("/admin/users/admin", post(admin::set_user_admin))
        .route("/admin/users/alias", post(admin::add_alias))
        .route("/admin/users/alias/remove", post(admin::remove_alias))
        .route(
            "/admin/users/{id}",
            axum::routing::delete(admin::delete_user),
        )
        .route("/admin/users/{id}/mailboxes", get(admin::user_mailboxes))
        // Admin console: groups & lists.
        .route(
            "/admin/groups",
            get(admin::list_groups).post(admin::create_group),
        )
        .route("/admin/groups/name", post(admin::rename_group))
        .route("/admin/groups/address", post(admin::set_group_address))
        .route("/admin/groups/members", post(admin::add_group_member))
        .route(
            "/admin/groups/members/remove",
            post(admin::remove_group_member),
        )
        .route(
            "/admin/groups/{id}",
            axum::routing::delete(admin::delete_group),
        )
        // Mailbox delegation / shared mailboxes (ADR 0017).
        .route("/admin/delegates", post(admin::grant_delegate))
        .route("/admin/delegates/remove", post(admin::revoke_delegate))
        .route("/admin/delegates/{ownerId}", get(admin::list_delegates))
        // Admin console: this tenant's domains (register + DNS-verify).
        .route(
            "/admin/domains",
            get(admin::list_domains).post(admin::create_domain),
        )
        .route("/admin/domains/verify", post(admin::verify_domain))
        .route("/admin/domains/delete", post(admin::delete_domain))
        .route("/admin/domains/dkim/rotate", post(admin::rotate_dkim))
        // Admin console: security & trust (live deliverability checks).
        .route("/admin/security/checks", get(security::checks))
        // Admin console: audit log (this tenant's administrative actions).
        .route("/admin/audit", get(admin::list_audit))
        // Mail settings: the user's signature + the tenant footer.
        .route("/settings/mail", get(settings::mail_settings))
        .route("/settings/signature", post(settings::set_signature))
        .route("/settings/out-of-office", post(settings::set_out_of_office))
        .route("/admin/org-footer", post(settings::set_org_footer))
        .layer(DefaultBodyLimit::max(upload_limit))
        .with_state(state);
    jmap.merge(identity_routes)
}

/// A convenience [`AppState`] with default limits and a fresh push hub.
pub fn app_state(store: Arc<Store>, identity: Identity, base_url: impl Into<String>) -> AppState {
    AppState {
        store,
        identity,
        push: PushHub::new(),
        limits: Limits::default(),
        base_url: base_url.into(),
        submission_addr: std::env::var("ALO_JMAP_SUBMISSION_ADDR").ok(),
        junk_learner: crate::junk_learn::JunkLearner::from_env(),
        personal_domains: std::env::var("ALO_PERSONAL_DOMAINS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        signup_limiter: alo_identity::ratelimit::RateLimiter::new(),
    }
}

/// `POST /auth/token` — the **non-public** first-party password grant for
/// programmatic clients (e.g. the raw JMAP exit-gate client): username +
/// password (+ optional `otp`) → an opaque access token, issued through
/// `alo-identity` with the same constant-time path and 2FA enforcement
/// as the OAuth flow. Public/browser clients use `/oauth/authorize`
/// instead (ADR 0008).
async fn token(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Problem> {
    let username = body
        .get("username")
        .and_then(Value::as_str)
        .ok_or_else(Problem::not_request)?;
    let password = body
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(Problem::not_request)?;
    let otp = body.get("otp").and_then(Value::as_str);
    match state
        .identity
        .password_login(username, password, otp)
        .await
        .map_err(|_| Problem::server_error())?
    {
        Some((token, principal)) => Ok(Json(
            json!({ "token": token.reveal(), "accountId": principal.user.as_str() }),
        )),
        None => Err(Problem::unauthorized()),
    }
}

/// Binds `addr` and serves the JMAP API (with the OIDC provider) until
/// shutdown. Provisions an ID-token signing key first (idempotent), so the
/// mounted `/oauth/jwks` and token-signing paths work without an
/// out-of-band CLI step — failing fast with a clear message if it cannot.
///
/// # Errors
/// I/O errors binding or serving; a startup error if the signing key cannot
/// be provisioned.
pub async fn serve(addr: SocketAddr, state: AppState) -> std::io::Result<()> {
    state.identity.ensure_signing_key().await.map_err(|error| {
        std::io::Error::other(format!("could not provision OIDC signing key: {error}"))
    })?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "alo-jmap listening");
    axum::serve(listener, app(state)).await
}
