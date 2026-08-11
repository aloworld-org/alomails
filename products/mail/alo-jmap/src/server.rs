//! The axum router (JMAP methods + the mounted OIDC provider), the
//! non-public first-party `/auth/token` password grant, and `serve`.

use std::net::SocketAddr;
use std::sync::Arc;

use alo_identity::Identity;
use alo_store::Store;
use axum::extract::{DefaultBodyLimit, Extension, State};
use axum::routing::{any, delete, get, patch, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::error::Problem;
use crate::push::PushHub;
use crate::state::{AppState, Limits};
use crate::{
    admin, agent, ai, api, audit, audit_record, autoconfig, base, billing_bills, billing_customers,
    billing_fx, billing_invoices, billing_payments, billing_products, billing_quotes,
    billing_reminder, billing_reports, billing_schedules, billing_send, billing_sepa,
    billing_settings, blob, calendar, carddav, chat, chat_agent_routes, contacts, crm_activities,
    crm_deals, crm_handoff, crm_imports, crm_next_steps, crm_pipelines, crm_reports, crm_stages,
    crm_threads, delegates, docs, drive, filters, finance_approvals, finance_bank,
    finance_bank_match, finance_chart, finance_expenses, finance_mileage, finance_periods,
    finance_receipts, finance_report_aged, finance_report_balance, finance_report_pl,
    finance_report_vat, flagdue, hr_checklists, hr_documents, hr_employees, hr_holidays,
    hr_leave_balances, hr_leave_policies, hr_leave_requests, hr_org, hr_recruitment,
    imap_import_route, insights, insights_ask, insights_eval, insights_gallery, inventory_counts,
    inventory_locations, inventory_moves, inventory_po, inventory_po_print, inventory_po_receipts,
    inventory_po_send, inventory_reorder, inventory_scan, inventory_so, inventory_so_deliveries,
    inventory_so_invoice, inventory_stock, inventory_supplier_prices, inventory_suppliers,
    meet_routes, projects_clients, projects_invoices, projects_plan, projects_reports,
    projects_templates, projects_time, projects_weeks, push, reset_route, schedule, scoped_roles,
    security, session, settings, share, signup_route, sites, snooze, spaces, tasks, unsubscribe,
    wopi, workspace_search,
};

/// Builds the JMAP router over the given state. The OpenID Connect /
/// OAuth 2.0 provider (`alo-identity`) is mounted alongside so a Phase-1
/// deployment serves JMAP and the IdP from one HTTP service.
pub fn app(state: AppState) -> Router {
    app_with_site_domain_dns(state, Arc::new(sites::SystemSiteDomainTxtLookup))
}

/// Builds the router with an injectable Sites TXT lookup. Production callers
/// use [`app`]; integration tests replace only this external DNS boundary.
pub fn app_with_site_domain_dns(
    state: AppState,
    site_domain_dns: Arc<dyn sites::SiteDomainTxtLookup>,
) -> Router {
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
        // alo Sites (ADR 0036) — the authenticated edit surface; public
        // serving is the separate alo-sites binary. `/sites` is a NEW
        // top-level prefix: the production Caddyfile needs it added at the
        // next deploy (docs/design/sites.md). Static paths before /{id}.
        // alo Chat (ADR 0038): rooms, membership, messages, read state. Also a
        // new top-level prefix — the production Caddyfile needs /chat/* added
        // at the next deploy. Static paths before /{id}, as above.
        .route(
            "/chat/channels",
            get(chat::list_channels).post(chat::create_channel),
        )
        .route("/chat/channels/joinable", get(chat::list_joinable))
        .route(
            "/chat/channels/{id}",
            get(chat::get_channel).patch(chat::patch_channel),
        )
        .route("/chat/channels/{id}/archive", post(chat::archive_channel))
        .route("/chat/channels/{id}/join", post(chat::join_channel))
        .route("/chat/channels/{id}/members", post(chat::add_member))
        .route(
            "/chat/channels/{id}/members/{user}",
            delete(chat::remove_member),
        )
        .route(
            "/chat/channels/{id}/messages",
            get(chat::list_messages).post(chat::post_message),
        )
        .route("/chat/channels/{id}/threads/{seq}", get(chat::list_thread))
        .route("/chat/channels/{id}/read", post(chat::mark_read))
        // Static before `{id}`: `/chat/reactions` must not be read as a
        // message called "reactions".
        .route("/chat/reactions", get(chat::list_reactions))
        .route("/chat/search", get(chat::search))
        .route("/chat/people", get(chat::find_people))
        // alo Meet: the record is ours, the media is the engine's.
        .route("/meet", get(meet_routes::mine).post(meet_routes::start))
        .route("/meet/{id}", get(meet_routes::get))
        .route("/meet/{id}/join", post(meet_routes::join))
        .route("/meet/{id}/end", post(meet_routes::end))
        .route("/meet/{id}/participants", get(meet_routes::participants))
        .route("/meet/channels/{id}", get(meet_routes::in_channel))
        .route("/meet/events/{id}", get(meet_routes::for_event))
        .route(
            "/chat/channels/{id}/turns",
            get(chat_agent_routes::list_turns),
        )
        .route(
            "/chat/channels/{id}/turns/{turn}/stop",
            post(chat_agent_routes::stop_turn),
        )
        .route(
            "/chat/agents",
            get(chat_agent_routes::list_agents).post(chat_agent_routes::create_agent),
        )
        .route(
            "/chat/channels/{id}/agents",
            get(chat_agent_routes::list_channel_agents).post(chat_agent_routes::add_channel_agent),
        )
        .route(
            "/chat/channels/{id}/agents/{agent}",
            delete(chat_agent_routes::remove_channel_agent),
        )
        .route(
            "/chat/proposals/{id}",
            post(chat_agent_routes::decide_proposal),
        )
        .route(
            "/chat/messages/{id}",
            patch(chat::edit_message).delete(chat::delete_message),
        )
        .route("/chat/messages/{id}/reactions", post(chat::toggle_reaction))
        .route("/sites", get(sites::list_sites).post(sites::create_site))
        .route(
            "/sites/generate",
            post(sites::generate_site).layer(DefaultBodyLimit::max(sites::MAX_SITE_GENERATE_BYTES)),
        )
        .route("/sites/subdomain-check", get(sites::check_subdomain))
        .route("/sites/theme-presets", get(sites::list_theme_presets))
        .route("/sites/config", get(sites::sites_config))
        .route(
            "/sites/{id}",
            get(sites::get_site)
                .put(sites::update_site)
                .delete(sites::delete_site),
        )
        .route("/sites/{id}/theme", put(sites::set_theme))
        .route("/sites/{id}/publish", post(sites::publish_site))
        .route("/sites/{id}/unpublish", post(sites::unpublish_site))
        .route("/sites/{id}/analytics", get(sites::get_analytics))
        .route(
            "/sites/{id}/collections",
            get(sites::list_collections).post(sites::create_collection),
        )
        .route(
            "/sites/{id}/collections/{collection}",
            put(sites::update_collection).delete(sites::delete_collection),
        )
        .route(
            "/sites/{id}/collections/{collection}/preview",
            get(sites::preview_collection),
        )
        .route(
            "/sites/{id}/domains",
            get(sites::list_domains).post(sites::create_domain),
        )
        .route("/sites/{id}/domains/{domain}", delete(sites::delete_domain))
        .route(
            "/sites/{id}/domains/{domain}/verify",
            post(sites::verify_domain),
        )
        .route("/sites/{id}/submissions", get(sites::list_submissions))
        .route(
            "/sites/{id}/submissions.csv",
            get(sites::export_submissions),
        )
        .route(
            "/sites/{id}/forms/{form}/submissions/{submission}",
            put(sites::set_submission_handled),
        )
        .route(
            "/sites/{id}/pages",
            get(sites::list_pages).post(sites::create_page),
        )
        .route(
            "/sites/{id}/translation-readiness",
            get(sites::translation_readiness),
        )
        .route(
            "/sites/{id}/translation-proposals",
            post(sites::propose_site_translation).put(sites::apply_site_translation),
        )
        .route(
            "/sites/{id}/posts",
            get(sites::list_posts).post(sites::create_post),
        )
        .route(
            "/sites/{id}/posts/{post}",
            get(sites::get_post)
                .put(sites::update_post)
                .delete(sites::delete_post),
        )
        .route(
            "/sites/{id}/posts/{post}/publish",
            post(sites::publish_post),
        )
        .route(
            "/sites/{id}/posts/{post}/unpublish",
            post(sites::unpublish_post),
        )
        .route("/sites/{id}/pages/order", put(sites::reorder_pages))
        .route(
            "/sites/{id}/pages/{pid}/locales/{locale}",
            get(sites::get_localized_page).put(sites::put_localized_page),
        )
        .route(
            "/sites/{id}/pages/{pid}/locales/{locale}/preview",
            get(sites::preview_localized_page),
        )
        .route(
            "/sites/{id}/pages/{pid}",
            get(sites::get_page)
                .put(sites::update_page)
                .delete(sites::delete_page),
        )
        .route("/sites/{id}/pages/{pid}/home", post(sites::set_home_page))
        .route("/sites/{id}/pages/{pid}/preview", get(sites::preview_page))
        .route(
            "/sites/{id}/pages/{pid}/ai-edits",
            post(sites::propose_page_edit)
                .put(sites::apply_page_edit)
                .layer(DefaultBodyLimit::max(sites::MAX_SITE_EDIT_BYTES)),
        )
        .route(
            "/sites/{id}/pages/{pid}/sections",
            put(sites::set_sections).post(sites::add_section),
        )
        .route(
            "/sites/{id}/pages/{pid}/sections/{index}",
            put(sites::update_section).delete(sites::remove_section),
        )
        .route(
            "/sites/{id}/pages/{pid}/sections/{index}/move",
            post(sites::move_section),
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
        // alo Billing (ADR 0035, wave B1) — the customer list and the price
        // list. `/billing` is a NEW top-level prefix: the production Caddyfile
        // needs it added at the next deploy (docs/design/billing.md § Routes).
        // Archiving is its own POST, never a field on the PATCH, so an
        // ordinary edit cannot drop a record out of the pickers.
        .route(
            "/billing/customers",
            get(billing_customers::list_customers).post(billing_customers::create_customer),
        )
        .route(
            "/billing/customers/{id}",
            get(billing_customers::get_customer).patch(billing_customers::update_customer),
        )
        .route(
            "/billing/customers/{id}/archive",
            post(billing_customers::archive_customer),
        )
        .route(
            "/billing/products",
            get(billing_products::list_products).post(billing_products::create_product),
        )
        .route(
            "/billing/products/{id}",
            get(billing_products::get_product).patch(billing_products::update_product),
        )
        .route(
            "/billing/products/{id}/archive",
            post(billing_products::archive_product),
        )
        // alo Inventory (ADR 0035, wave B5.03) — the suppliers a tenant buys
        // from, and the price list THEY quote us. `/inventory` is a NEW
        // top-level prefix: the production Caddyfile needs it added at the next
        // deploy (docs/design/inventory.md § Routes), exactly as `/billing`
        // did. Archiving is its own POST, never a field on the PATCH; the
        // offer under a supplier is an idempotent PUT on the pair, so a form
        // saves in one call and a retry cannot produce two quotes.
        .route(
            "/inventory/suppliers",
            get(inventory_suppliers::list_suppliers).post(inventory_suppliers::create_supplier),
        )
        .route(
            "/inventory/suppliers/{id}",
            get(inventory_suppliers::get_supplier).patch(inventory_suppliers::update_supplier),
        )
        .route(
            "/inventory/suppliers/{id}/archive",
            post(inventory_suppliers::archive_supplier),
        )
        .route(
            "/inventory/suppliers/{id}/products",
            get(inventory_supplier_prices::list_supplier_products),
        )
        .route(
            "/inventory/suppliers/{id}/products/{product_id}",
            // Spelled `put(…)` rather than `axum::routing::put(…)` so
            // `tests/audit_routes.rs` can see the verb: it reads this file's
            // source, and a qualified path hides a mutating route from the
            // promise that every one of them is audited (B5.04b).
            put(inventory_supplier_prices::set_supplier_product)
                .delete(inventory_supplier_prices::remove_supplier_product),
        )
        // Locations, on-hand and the move ledger (B5.04b). The list SEEDS a
        // tenant's starting places on first read, in the caller's language, so
        // receiving the first purchase order books itself instead of failing
        // with "there is nowhere to put it".
        //
        // `POST /inventory/moves` is the one door that writes a movement by
        // hand — a transfer, or an adjustment with a reason code — and it is
        // the route that brings `inventory` into
        // `audit_action::AUDITED_MODULES`: "who adjusted this stock down by
        // forty, and when" is what the audit trail exists for. There is
        // deliberately no PATCH and no DELETE on a movement: a mistake is
        // corrected by a movement in the other direction.
        .route(
            "/inventory/locations",
            get(inventory_locations::list_locations).post(inventory_locations::create_location),
        )
        .route(
            "/inventory/locations/{id}",
            get(inventory_locations::get_location)
                .patch(inventory_locations::update_location)
                .delete(inventory_locations::delete_location),
        )
        .route(
            "/inventory/locations/{id}/archive",
            post(inventory_locations::archive_location),
        )
        .route("/inventory/stock", get(inventory_stock::list_stock))
        // `GET /inventory/scan?code=` (B5.09c) is the read behind pointing a
        // machine at a box: a wedge scanner and a phone camera both end here,
        // because they are two ways of typing the same thirteen digits. A code
        // that is not a GTIN is a `422` carrying the reason, so a misread scan
        // is never reported as a product nobody stocks.
        .route("/inventory/scan", get(inventory_scan::scan))
        .route(
            "/inventory/moves",
            get(inventory_moves::list_moves).post(inventory_moves::create_move),
        )
        // Purchase orders (B5.05a): the order we place with a supplier. A
        // draft is editable and deletable; everything past it is frozen, and
        // cancelling is its own POST because it is a decision with a date, not
        // a field on a form. `POST …/{id}/send` (B5.05a2) is the one act that
        // draws the number, stamps the day, freezes the order AND writes the
        // covering mail draft with the printed order attached; `/print` and
        // `/pdf` are that same document without the letter.
        .route(
            "/inventory/purchase-orders",
            get(inventory_po::list_purchase_orders).post(inventory_po::create_purchase_order),
        )
        .route(
            "/inventory/purchase-orders/{id}",
            get(inventory_po::get_purchase_order)
                .patch(inventory_po::update_purchase_order)
                .delete(inventory_po::delete_purchase_order),
        )
        .route(
            "/inventory/purchase-orders/{id}/cancel",
            post(inventory_po::cancel_purchase_order),
        )
        .route(
            "/inventory/purchase-orders/{id}/send",
            post(inventory_po_send::send_purchase_order),
        )
        // Receiving (B5.05b): one act with three consequences — the movements
        // into stock, the order's new state, and the draft bill for what
        // arrived. A sub-resource, so the audit trail files it against the
        // order it happened to.
        .route(
            "/inventory/purchase-orders/{id}/receipts",
            get(inventory_po_receipts::list_receipts).post(inventory_po_receipts::create_receipt),
        )
        .route(
            "/inventory/purchase-orders/{id}/print",
            get(inventory_po_print::print_purchase_order),
        )
        .route(
            "/inventory/purchase-orders/{id}/pdf",
            get(inventory_po_print::pdf_purchase_order),
        )
        // Sales orders (B5.06a) — the purchase order mirrored: what a customer
        // asked US for. Confirming draws the number and freezes the document;
        // it moves no stock, because an order is a promise and goods move when
        // they are picked.
        .route(
            "/inventory/sales-orders",
            get(inventory_so::list_sales_orders).post(inventory_so::create_sales_order),
        )
        .route(
            "/inventory/sales-orders/{id}",
            get(inventory_so::get_sales_order)
                .patch(inventory_so::update_sales_order)
                .delete(inventory_so::delete_sales_order),
        )
        .route(
            "/inventory/sales-orders/{id}/confirm",
            post(inventory_so::confirm_sales_order),
        )
        .route(
            "/inventory/sales-orders/{id}/cancel",
            post(inventory_so::cancel_sales_order),
        )
        // Delivering (B5.06a): the movements out of stock and the order's new
        // state, in one act, with the delivery note that describes them. A
        // sub-resource, so the audit trail files it against the order it
        // happened to.
        .route(
            "/inventory/sales-orders/{id}/deliveries",
            get(inventory_so_deliveries::list_deliveries)
                .post(inventory_so_deliveries::create_delivery),
        )
        // Invoicing (B5.06b): the bridge into billing. It bills what has been
        // DELIVERED and not yet invoiced — never what was ordered — so a
        // partial consignment invoices correctly and a second one raises a
        // second draft for the new quantity only.
        .route(
            "/inventory/sales-orders/{id}/invoice",
            post(inventory_so_invoice::create_invoice),
        )
        .route(
            "/inventory/sales-orders/{id}/invoices",
            get(inventory_so_invoice::list_invoices),
        )
        // Reorder rules and the shortage report (B5.07). The rules are the only
        // thing a tenant types; the report is derived — on-hand from the
        // ledger, on-order from the placed purchase orders, committed from the
        // confirmed sales orders — so it has no writable surface and never
        // will. The `.csv` twin names its format in the URL, as every report
        // before it does (B1.20).
        .route(
            "/inventory/reorder-rules",
            get(inventory_reorder::list_rules).post(inventory_reorder::create_rule),
        )
        .route(
            "/inventory/reorder-rules/{id}",
            get(inventory_reorder::get_rule)
                .patch(inventory_reorder::update_rule)
                .delete(inventory_reorder::delete_rule),
        )
        .route(
            "/inventory/shortages",
            get(inventory_reorder::list_shortages),
        )
        .route(
            "/inventory/shortages.csv",
            get(inventory_reorder::shortages_csv),
        )
        // Stocktakes (B5.08a, B5.08b). A count is a worksheet over one
        // location: it snapshots what the ledger says is there, a person works
        // down the sheet, and applying it writes ordinary adjustment movements
        // — recomputed against on-hand at that moment, skipping any row whose
        // shelf moved underneath the counter. A row is PUT rather than POSTed
        // because its identity is the pair (count, product) — a wedge scanner
        // that fires twice on one barcode must record one row, not two.
        .route(
            "/inventory/counts",
            get(inventory_counts::list_counts).post(inventory_counts::open_count),
        )
        .route(
            "/inventory/counts/{id}",
            get(inventory_counts::get_count).patch(inventory_counts::update_count),
        )
        .route(
            "/inventory/counts/{id}/lines/{product_id}",
            put(inventory_counts::set_count_line),
        )
        .route(
            "/inventory/counts/{id}/apply",
            post(inventory_counts::apply_count),
        )
        .route(
            "/inventory/counts/{id}/cancel",
            post(inventory_counts::cancel_count),
        )
        // alo HR (B6.02b), the module's first routes. `/hr` is a NEW top-level
        // prefix: the production Caddyfile needs it added at the next deploy,
        // and it joins `API_PATHS` in `web/vite.config.ts` in the same commit.
        //
        // Two doors, and which one answers is decided by calling a different
        // store function rather than by filtering a response: the directory and
        // the chart are every member's read and carry public fields only; the
        // record, the terms and the papers are HR's (or a tenant admin's), and
        // are the only place a national identifier, an IBAN or a pay figure is
        // returned about somebody else (`docs/design/hr.md` § Three doors).
        //
        // The archive is its own POST for the reason /billing/customers set:
        // an ordinary edit must never drop somebody out of the directory
        // because a stale form carried a flag. Documents are a sub-resource of
        // the person, so the audit trail files them against the record they are
        // on (`hr.employee.document.create`, audit_action::event_for).
        .route("/hr/me", get(hr_org::me))
        .route("/hr/org", get(hr_org::org_chart))
        .route(
            "/hr/employees",
            get(hr_employees::list_employees).post(hr_employees::create_employee),
        )
        .route(
            "/hr/employees/{id}",
            get(hr_employees::get_employee).patch(hr_employees::update_employee),
        )
        .route(
            "/hr/employees/{id}/archive",
            post(hr_employees::archive_employee),
        )
        .route(
            "/hr/employees/{id}/documents",
            get(hr_documents::list_documents).post(hr_documents::file_document),
        )
        .route(
            "/hr/employees/{id}/documents/{document_id}",
            delete(hr_documents::detach_document),
        )
        // Leave (B6.03b). The policies a tenant runs are readable by every
        // member — somebody asking for time off has to choose what kind, and a
        // picker they may not read is a form they cannot fill in — and writable
        // only through the HR door. There is no DELETE: a balance is only
        // explicable beside the policy that produced it, so a policy is
        // archived (`docs/design/hr.md` § Leave).
        //
        // The four verbs on a request are POSTs rather than a status field on
        // the PATCH, for the reason /billing/invoices/{id}/issue established:
        // a decision must never happen because an editor sent a stale form.
        // Who may take each of them is `hr_leave_door` — mine, my team's, or
        // HR's — and nobody approves their own leave unless they are the
        // tenant's admin.
        //
        // `/hr/absences` is the module's one read every member gets about
        // other people: a name and a day, never the reason. The Agenda draws
        // it as a layer rather than as events, because a calendar event has an
        // owner who could delete an approval, and a title somebody could type a
        // diagnosis into (`docs/design/hr.md` § The absence layer).
        .route(
            "/hr/leave-policies",
            get(hr_leave_policies::list_policies).post(hr_leave_policies::create_policy),
        )
        .route(
            "/hr/leave-policies/{id}",
            get(hr_leave_policies::get_policy).patch(hr_leave_policies::update_policy),
        )
        .route(
            "/hr/leave-policies/{id}/archive",
            post(hr_leave_policies::archive_policy),
        )
        .route(
            "/hr/leave-requests",
            get(hr_leave_requests::list_requests).post(hr_leave_requests::create_request),
        )
        .route(
            "/hr/leave-requests/{id}",
            get(hr_leave_requests::get_request).patch(hr_leave_requests::update_request),
        )
        .route(
            "/hr/leave-requests/{id}/withdraw",
            post(hr_leave_requests::withdraw_request),
        )
        .route(
            "/hr/leave-requests/{id}/approve",
            post(hr_leave_requests::approve_request),
        )
        .route(
            "/hr/leave-requests/{id}/reject",
            post(hr_leave_requests::reject_request),
        )
        .route(
            "/hr/leave-requests/{id}/cancel",
            post(hr_leave_requests::cancel_request),
        )
        .route("/hr/leave-balances", get(hr_leave_balances::list_balances))
        .route("/hr/absences", get(hr_leave_balances::list_absences))
        // Public holidays (B6.04). The days themselves are a seed table in the
        // repo, not rows: what is per-tenant is only which calendar a company
        // observes, and that choice is one value replaced whole — hence a PUT
        // on the collection rather than a POST that appends. Reading either
        // route is every member's, because a holiday inside somebody's leave
        // costs them nothing and they are entitled to know which days those
        // are (`docs/design/hr.md` § Public holidays).
        .route("/hr/holidays", get(hr_holidays::list_holidays))
        .route(
            "/hr/holiday-calendars",
            get(hr_holidays::get_calendars).put(hr_holidays::put_calendars),
        )
        // Onboarding and offboarding checklists (B6.05). The templates are
        // HR's both ways — unlike a leave policy, a shape nobody may run is of
        // no use to read — and DELETE is a real delete here, because running a
        // template *copies* it onto a task board: nothing anybody is working
        // through depends on the row (`docs/design/hr.md` § Onboarding and
        // offboarding checklists).
        //
        // Running one is a POST on the person, so the audit trail files it
        // against their record (`hr.employee.checklist.create`); reading the
        // runs back is the leave door — HR, their manager, or the newcomer
        // looking at their own first week.
        .route(
            "/hr/checklist-templates",
            get(hr_checklists::list_templates).post(hr_checklists::create_template),
        )
        .route(
            "/hr/checklist-templates/{id}",
            get(hr_checklists::get_template)
                .patch(hr_checklists::update_template)
                .delete(hr_checklists::delete_template),
        )
        .route(
            "/hr/employees/{id}/checklists",
            get(hr_checklists::list_checklists).post(hr_checklists::run_checklist),
        )
        // Hiring (B6.06a): the openings, and the people who applied for them.
        // Every route is HR's — there is no employee-facing or manager-facing
        // view of a candidate at all.
        //
        // `publish` and `close` are POSTs of their own rather than a status
        // field on the PATCH, the shape `issue` and `archive` already have: a
        // transition is a decision, and it must not happen because an editor
        // sent back a stale form. A closed opening is terminal — a role being
        // hired for again is next year's opening.
        //
        // `move` is the ONLY way a stage changes, deliberately (the PATCH will
        // not write one), so every decision about a person is one audited act
        // with a human's id on it. Nothing in this service reads a CV: no
        // screening, ranking or scoring exists to be routed to
        // (`docs/design/hr.md` § The EU AI Act posture).
        //
        // DELETE on an applicant is a real erasure — the row, its notes and the
        // CV — because an unsuccessful applicant has no statutory retention
        // behind them, and it is a person's act rather than a scheduled job.
        .route(
            "/hr/openings",
            get(hr_recruitment::list_openings).post(hr_recruitment::create_opening),
        )
        .route(
            "/hr/openings/{id}",
            get(hr_recruitment::get_opening).patch(hr_recruitment::update_opening),
        )
        .route(
            "/hr/openings/{id}/publish",
            post(hr_recruitment::publish_opening),
        )
        .route(
            "/hr/openings/{id}/close",
            post(hr_recruitment::close_opening),
        )
        .route(
            "/hr/openings/{id}/applicants",
            get(hr_recruitment::list_applicants).post(hr_recruitment::record_applicant),
        )
        .route(
            "/hr/applicants/{id}",
            get(hr_recruitment::get_applicant)
                .patch(hr_recruitment::update_applicant)
                .delete(hr_recruitment::delete_applicant),
        )
        .route(
            "/hr/applicants/{id}/move",
            post(hr_recruitment::move_applicant),
        )
        .route("/hr/applicants/{id}/notes", post(hr_recruitment::add_note))
        // Invoices (B1.10). The lifecycle transitions are their own POSTs, not
        // fields on the PATCH: issuing assigns a legal number and freezes the
        // document, so it can never happen because an editor sent a stale
        // form. DELETE is draft-only — an issued document is voided, keeping
        // its number so the series stays gapless.
        .route(
            "/billing/invoices",
            get(billing_invoices::list_invoices).post(billing_invoices::create_invoice),
        )
        .route(
            "/billing/invoices/{id}",
            get(billing_invoices::get_invoice)
                .patch(billing_invoices::update_invoice)
                .delete(billing_invoices::delete_invoice),
        )
        .route(
            "/billing/invoices/{id}/issue",
            post(billing_invoices::issue_invoice),
        )
        .route(
            "/billing/invoices/{id}/void",
            post(billing_invoices::void_invoice),
        )
        .route(
            "/billing/invoices/{id}/credit-note",
            post(billing_invoices::create_credit_note),
        )
        // The printable document (B1.16): self-contained HTML, and the source
        // the PDF (B1.17) and the mail attachment (B1.18) are made from.
        .route(
            "/billing/invoices/{id}/print",
            get(billing_invoices::print_invoice),
        )
        // The same document as a file (B1.17): laid out a second way from the
        // same model, never converted from the page above.
        .route(
            "/billing/invoices/{id}/pdf",
            get(billing_invoices::pdf_invoice),
        )
        // The e-invoice on its own (B1.22): the same EN 16931 document the
        // hybrid PDF carries inside it, for a customer whose system takes a
        // file rather than extracting one.
        .route(
            "/billing/invoices/{id}/facturx.xml",
            get(billing_invoices::facturx_invoice),
        )
        // The same e-invoice in the other syntax law recognises (B1.23): UBL
        // 2.1 in the German CIUS, which is what a public authority must be
        // invoiced with. It refuses more often than the route above, because
        // XRechnung requires terms EN 16931 leaves optional.
        .route(
            "/billing/invoices/{id}/xrechnung.xml",
            get(billing_invoices::xrechnung_invoice),
        )
        // Drafts a covering email to the customer with the PDF attached and
        // leaves it in Drafts (B1.18). It never sends, and never changes the
        // invoice — unlike the quote route of the same name, which is a
        // lifecycle transition.
        .route(
            "/billing/invoices/{id}/send",
            post(billing_send::send_invoice),
        )
        // Chasing one late invoice (B1.26): the same drafts-only rule as
        // `/send`, for the letter that asks for money instead of presenting the
        // document. It reads how late and how much is left off the stored
        // invoice, so the request cannot state either.
        .route(
            "/billing/invoices/{id}/reminder",
            post(billing_reminder::remind_invoice),
        )
        // Payments (B1.19) — money received, under the document it settles:
        // a payment does not exist on its own, and addressing it through its
        // invoice is what makes an id from another document a plain 404. The
        // invoice's `paid` status is a projection of this ledger, recomputed
        // by the store inside the transaction that changes it.
        .route(
            "/billing/invoices/{id}/payments",
            get(billing_payments::list_payments).post(billing_payments::create_payment),
        )
        .route(
            "/billing/invoices/{id}/payments/{payment_id}",
            delete(billing_payments::delete_payment),
        )
        // Bills (B1.24) — the other direction: a supplier's Factur-X or
        // XRechnung file read into a record waiting for approval. The upload
        // carries the XML as its own body (a file is a file, not a JSON
        // string), and a decision is its own POST because it is final.
        .route("/billing/bills/import", post(billing_bills::import_bill))
        .route("/billing/bills", get(billing_bills::list_bills))
        .route(
            "/billing/bills/{id}",
            get(billing_bills::get_bill).delete(billing_bills::delete_bill),
        )
        .route(
            "/billing/bills/{id}/approve",
            post(billing_bills::approve_bill),
        )
        .route(
            "/billing/bills/{id}/reject",
            post(billing_bills::reject_bill),
        )
        // Paying them (B2.12): the approved bills of a run become one SEPA
        // credit-transfer file the tenant uploads to their bank. A POST that
        // answers with a file, because giving the instruction and recording
        // that it was given are one act (crate::billing_sepa). Registered
        // before `{id}` would ever be consulted — a static segment wins — and
        // under the existing `/billing` prefix, so the Caddyfile needs nothing.
        .route(
            "/billing/bills/sepa.xml",
            post(billing_sepa::export_payment_file),
        )
        // Quotes (B1.12) — the offer that precedes an invoice, with the same
        // shape: draft CRUD, a strict status filter, and every transition its
        // own POST. Accepting is the one that answers two documents: it closes
        // the offer and raises the draft invoice for it in one transaction.
        .route(
            "/billing/quotes",
            get(billing_quotes::list_quotes).post(billing_quotes::create_quote),
        )
        .route(
            "/billing/quotes/{id}",
            get(billing_quotes::get_quote)
                .patch(billing_quotes::update_quote)
                .delete(billing_quotes::delete_quote),
        )
        .route(
            "/billing/quotes/{id}/send",
            post(billing_quotes::send_quote),
        )
        .route(
            "/billing/quotes/{id}/accept",
            post(billing_quotes::accept_quote),
        )
        .route(
            "/billing/quotes/{id}/decline",
            post(billing_quotes::decline_quote),
        )
        .route(
            "/billing/quotes/{id}/expire",
            post(billing_quotes::expire_quote),
        )
        .route(
            "/billing/quotes/{id}/print",
            get(billing_quotes::print_quote),
        )
        // Recurring invoices (B2.11): the standing arrangements that raise the
        // same invoice again every month, and the run that raises the drafts
        // they have come due for. `/run` is a plural act on the collection —
        // "raise everything due" — so it sits beside the collection rather than
        // on any one arrangement; a schedule is never run off its own rhythm.
        // Under the existing `/billing` prefix, so the Caddyfile needs nothing
        // new.
        .route(
            "/billing/schedules/run",
            post(billing_schedules::run_schedules),
        )
        .route(
            "/billing/schedules",
            get(billing_schedules::list_schedules).post(billing_schedules::create_schedule),
        )
        .route(
            "/billing/schedules/{id}",
            get(billing_schedules::get_schedule)
                .patch(billing_schedules::update_schedule)
                .delete(billing_schedules::delete_schedule),
        )
        .route(
            "/billing/schedules/{id}/pause",
            post(billing_schedules::pause_schedule),
        )
        .route(
            "/billing/schedules/{id}/resume",
            post(billing_schedules::resume_schedule),
        )
        // The issuer identity every printed document carries (B1.16): one row
        // per tenant, so the resource has no id and no list.
        .route(
            "/billing/settings",
            get(billing_settings::get_settings).patch(billing_settings::update_settings),
        )
        // The VAT summary of a period (B1.20) — one read, two
        // representations: JSON for the screen and CSV for the accountant,
        // named by their own paths exactly as /print and /pdf are.
        // The exchange rates a tenant's foreign-currency documents are converted
        // at (B1.21): what it has, one rate by hand, and a published
        // reference-rate file. Under the existing `/billing` prefix, so the
        // production Caddyfile needs nothing new.
        .route(
            "/billing/fx/rates",
            get(billing_fx::list_rates).put(billing_fx::put_rate),
        )
        .route("/billing/fx/rates/import", post(billing_fx::import_rates))
        .route("/billing/reports/vat", get(billing_reports::vat_report))
        .route(
            "/billing/reports/vat.csv",
            get(billing_reports::vat_report_csv),
        )
        // alo CRM (ADR 0035, wave B2) — the boards deals move across, their
        // columns, and the deals themselves (B2.04). `/crm` is a NEW top-level
        // prefix: the production Caddyfile needs it added at the next deploy,
        // the same standing human action `/billing` carries
        // (docs/design/crm.md § Routes).
        //
        // Listing the boards is also what SEEDS a tenant's first one, in the
        // language `?lang=` asks for — a new tenant opens CRM onto a working
        // funnel rather than a setup form.
        .route(
            "/crm/pipelines",
            get(crm_pipelines::list_pipelines).post(crm_pipelines::create_pipeline),
        )
        .route(
            "/crm/pipelines/{id}",
            get(crm_pipelines::get_pipeline).patch(crm_pipelines::update_pipeline),
        )
        .route(
            "/crm/pipelines/{id}/archive",
            post(crm_pipelines::archive_pipeline),
        )
        // A column is created under its board and addressed on its own after
        // that. Reordering is its own POST, never a field on the PATCH: a board
        // drag must not be able to rename a column, and saving an edit form must
        // not be able to reorder the board. DELETE is for a column created by
        // mistake — one no deal and no history row has ever named.
        .route(
            "/crm/pipelines/{id}/stages",
            get(crm_stages::list_stages).post(crm_stages::create_stage),
        )
        .route(
            "/crm/stages/{id}",
            get(crm_stages::get_stage)
                .patch(crm_stages::update_stage)
                .delete(crm_stages::delete_stage),
        )
        .route("/crm/stages/{id}/move", post(crm_stages::move_stage))
        .route("/crm/stages/{id}/archive", post(crm_stages::archive_stage))
        // Deals. The move is its own POST for the same reason issuing an
        // invoice is: it writes a history row and can close the deal, so it must
        // never happen because an editor submitted a stale form.
        .route(
            "/crm/deals",
            get(crm_deals::list_deals).post(crm_deals::create_deal),
        )
        .route(
            "/crm/deals/{id}",
            get(crm_deals::get_deal)
                .patch(crm_deals::update_deal)
                .delete(crm_deals::delete_deal),
        )
        .route("/crm/deals/{id}/stage", post(crm_deals::move_deal))
        .route("/crm/deals/{id}/history", get(crm_deals::deal_history))
        // The won-deal handoff to billing (B2.08): a DRAFT quote or invoice for
        // the deal's customer — created from the lead and linked back to the
        // card when there was not one yet. A draft, always: nothing is issued,
        // nothing is sent, and no number is consumed from the gapless sequence.
        .route("/crm/deals/{id}/quote", post(crm_handoff::deal_quote))
        .route("/crm/deals/{id}/invoice", post(crm_handoff::deal_invoice))
        // The conversations a deal belongs to (B2.05) — the module's reason to
        // exist, and the one boundary inside the tenant that CRM has to defend:
        // a deal is tenant-wide, a mailbox is not. Suggestions PROPOSE over the
        // caller's own recent mail and write nothing; only the POST links, and
        // only a conversation the caller can already see.
        .route(
            "/crm/deals/{id}/threads",
            get(crm_threads::list_threads).post(crm_threads::link_thread),
        )
        .route(
            "/crm/deals/{id}/threads/{threadId}",
            delete(crm_threads::unlink_thread),
        )
        .route(
            "/crm/deals/{id}/thread-suggestions",
            get(crm_threads::suggest_threads),
        )
        // What was said and done on a deal (B2.06). Written once, never
        // edited — a correction is another note — and deleted only by the
        // colleague who wrote it, which is the one `403` in CRM: the entry is
        // readable tenant-wide, so hiding its existence would be theatre.
        .route(
            "/crm/deals/{id}/activities",
            get(crm_activities::list_activities).post(crm_activities::add_activity),
        )
        .route(
            "/crm/activities/{id}",
            delete(crm_activities::delete_activity),
        )
        // And what happens next (B2.06), which is deliberately NOT a CRM
        // record: a next step is a real task in the tasks module, carried by
        // ADR 0021's source link and answered in the tasks module's own JSON.
        // Two to-do lists in one workspace is how a CRM becomes the system
        // nobody updates.
        .route(
            "/crm/deals/{id}/next-steps",
            get(crm_next_steps::list_next_steps).post(crm_next_steps::add_next_step),
        )
        // Value by stage and win/loss for a board (B2.08). Two paths for one
        // read, as the VAT summary settled: a URL that names its representation
        // is the one a browser saves and a script quotes.
        .route("/crm/reports/pipeline", get(crm_reports::pipeline_report))
        .route(
            "/crm/reports/pipeline.csv",
            get(crm_reports::pipeline_report_csv),
        )
        // A lead list from a spreadsheet (B2.09): the preview writes nothing,
        // the commit is all-or-nothing. The file is the body — so both carry
        // the import's own body limit rather than the JMAP request limit.
        .route(
            "/crm/imports/leads/preview",
            post(crm_imports::preview_leads).layer(DefaultBodyLimit::max(
                alo_store::crm_lead_import::MAX_IMPORT_BYTES,
            )),
        )
        .route(
            "/crm/imports/leads",
            post(crm_imports::import_leads).layer(DefaultBodyLimit::max(
                alo_store::crm_lead_import::MAX_IMPORT_BYTES,
            )),
        )
        // alo Insights (ADR 0037, wave BI1.04) — the boards a tenant reads its
        // numbers from, and the two routes that answer a question with figures.
        // `/insights` is a NEW top-level prefix: the production Caddyfile needs
        // it added at the next deploy, the standing human action `/billing`,
        // `/crm` and `/audit` already carry (docs/design/insights.md § Routes).
        //
        // A board and its tiles come back in one read, and each tile's numbers
        // are fetched on its own — so a grid draws immediately and fills in as
        // the answers arrive, rather than waiting on the slowest chart.
        .route(
            "/insights/dashboards",
            get(insights::list_dashboards).post(insights::create_dashboard),
        )
        .route(
            "/insights/dashboards/{id}",
            get(insights::get_dashboard)
                .patch(insights::update_dashboard)
                .delete(insights::delete_dashboard),
        )
        .route(
            "/insights/dashboards/{id}/tiles",
            post(insights::create_tile),
        )
        // A tile is created under its board and addressed on its own after
        // that — `/billing/invoices/{id}/payments`' shape, for its reason. The
        // move is its own POST, the mirror of `/crm/stages/{id}/move`: a grid
        // drag must not be able to retitle a chart, and saving an edit form
        // must not be able to rearrange the board.
        .route(
            "/insights/tiles/{id}",
            patch(insights::update_tile).delete(insights::delete_tile),
        )
        .route("/insights/tiles/{id}/move", post(insights::move_tile))
        .route("/insights/tiles/{id}/data", get(insights_eval::tile_data))
        // The builder's live preview: a spec in, a series out, nothing stored.
        // That separation is what keeps the ask (BI1.07) propose-then-approve —
        // a model can have a chart drawn, and only a person can pin one.
        .route("/insights/eval", post(insights_eval::eval))
        // The prebuilt questions a tenant pins from (BI1.06). The specs it
        // hands back are pinned through the ordinary tile route, so the gallery
        // is a set of good defaults rather than a privileged path into the
        // store — the same write gate validates them either way.
        .route("/insights/gallery", get(insights_gallery::list_gallery))
        // Ask-to-chart (BI1.07): a question in, a PROPOSED chart and its
        // preview figures out. It stores nothing — pinning the proposal is the
        // ordinary tile route, so a model can only ever offer a chart and a
        // person is what puts one on a board (ADR 0034).
        .route("/insights/ask", post(insights_ask::ask))
        // alo Projects (ADR 0035, wave B3.04) — a person's own hours: the
        // running clock, the manual entry, and the week they read back.
        // `/projects` is a NEW top-level prefix: the production Caddyfile needs
        // it added at the next deploy, the standing human action `/billing`,
        // `/crm`, `/audit` and `/insights` already carry
        // (docs/design/projects.md § Routes).
        //
        // Nothing on this surface names a user. A person's hours are personal
        // data, the account door binds `user_id` on every statement, and the
        // cross-user reads — the approvals inbox, another person's week — are
        // the admin door's and arrive at B3.05.
        //
        // The engagement list and the client facts that make a board client
        // work (B3.07) — the module's front door, and what the timer's hours
        // become worth something against. A project is a `task_projects` row
        // plus, when it is client work, a `project_clients` row beside it; this
        // surface zips them, so an internal project reads with `client: null`
        // rather than not at all.
        //
        // The facts are addressed as `/projects/clients/{id}` rather than the
        // design note's `/projects/{id}/client`, because the audit derivation
        // reads the matched template mechanically and needs the collection in
        // the second segment — the record it files against is still the
        // project (`projects_clients`'s module note has the trade in full).
        .route("/projects", get(projects_clients::list_projects))
        .route(
            "/projects/clients/{id}",
            put(projects_clients::set_project_client)
                .delete(projects_clients::clear_project_client),
        )
        // `timer` and `time` are distinct literal segments, registered before
        // `/projects/time/{id}` so a record id can never shadow one.
        .route("/projects/timer", get(projects_time::get_timer))
        .route("/projects/timer/start", post(projects_time::start_timer))
        // Stopping is what writes the hour, so it is a POST of its own rather
        // than a DELETE of the timer: a delete promises the thing is gone, and
        // this one leaves an entry behind. Starting while one runs is a 409
        // carrying the running timer, never an implicit stop — the UI's one
        // button makes two calls, and both are audited.
        .route("/projects/timer/stop", post(projects_time::stop_timer))
        .route(
            "/projects/time",
            get(projects_time::list_time).post(projects_time::create_time),
        )
        // The agent's drafted entries and the two answers a human gives them
        // (B3.10a): `proposals` is a literal segment beside `{id}`, the same
        // shape `timer` has, and an entry id can never be one.
        .route(
            "/projects/time/proposals",
            get(projects_time::list_proposals),
        )
        .route(
            "/projects/time/{id}",
            get(projects_time::get_time)
                .patch(projects_time::update_time)
                .delete(projects_time::delete_time),
        )
        // Accepting is a write that prices the hour, so it is audited as
        // `projects.time.accept`; rejecting discards a suggestion that was in no
        // total, and is audited as `projects.time.reject`.
        .route(
            "/projects/time/{id}/accept",
            post(projects_time::accept_time),
        )
        .route(
            "/projects/time/{id}/reject",
            post(projects_time::reject_time),
        )
        // The week (B3.05) — two doors onto the same row, and the shape of each
        // URL is the reason it is a different door.
        //
        // The PERSONAL door names a week by its Monday, because a week nobody
        // has submitted has no row and therefore no id; the ADMIN door names it
        // by id, because spelling a colleague's week as (person, date) would put
        // an employee's identity in every access log on the way here. Both are
        // audited: `projects.week.*` for what a person did with their own week,
        // `projects.approval.*` for what an approver decided about somebody's.
        .route("/projects/weeks", get(projects_weeks::list_weeks))
        .route(
            "/projects/weeks/{monday}/submit",
            post(projects_weeks::submit_week),
        )
        .route(
            "/projects/weeks/{monday}/withdraw",
            post(projects_weeks::withdraw_week),
        )
        // Admin only, every one of them (`Account::require_admin`, checked in
        // the handler as `/admin/*` does).
        .route("/projects/approvals", get(projects_weeks::list_approvals))
        .route(
            "/projects/approvals/{id}/approve",
            post(projects_weeks::approve_week),
        )
        .route(
            "/projects/approvals/{id}/reject",
            post(projects_weeks::reject_week),
        )
        .route(
            "/projects/approvals/{id}/reopen",
            post(projects_weeks::reopen_week),
        )
        // The billable handoff (B3.06) — approved hours become a DRAFT invoice
        // in billing, and never anything more: it issues nothing and sends
        // nothing, the one-way, one-shot rule the won-deal handoff holds.
        //
        // The unbilled view is a tenant-wide AGGREGATE on the account door — an
        // invoice carries the team's hours, not the caller's — and it answers
        // with projects, minutes and money, never with who worked when. The
        // handoff is audited as `projects.invoice.create` against the document
        // it raised, so which hours went onto an invoice, and who sent them
        // there, is answerable from the record.
        .route("/projects/unbilled", get(projects_invoices::list_unbilled))
        .route(
            "/projects/invoices",
            post(projects_invoices::create_invoice),
        )
        // The profitability report (B3.08) — hours × rates against a budget,
        // per engagement per currency, with the CSV beside it. Two paths rather
        // than one route with a `?format=`, the shape `/billing/reports/vat` and
        // `/crm/reports/pipeline` already have: a URL that names its
        // representation is the one a browser saves under a sensible name.
        //
        // A project aggregate, like the unbilled view: it answers with
        // engagements, minutes and money and never with who worked when.
        .route(
            "/projects/reports/profitability",
            get(projects_reports::profitability_report),
        )
        .route(
            "/projects/reports/profitability.csv",
            get(projects_reports::profitability_report_csv),
        )
        // The plan (B3.09) — the milestones a timeline is drawn from, and
        // where each task sits among them. The list route answers with both in
        // one read: a timeline that fetched them separately would draw a bar
        // before it knew what was under it.
        //
        // Addressed as `/projects/milestones/{id}` rather than the design
        // note's `/projects/{id}/milestones`, for the audit derivation's
        // reason the client facts give above; the project is stated as
        // `projectId`. Reaching a milestone is its own POST rather than a
        // field on the PATCH, so the trail says `projects.milestone.done`
        // instead of filing a closed deliverable as an edit.
        .route(
            "/projects/milestones",
            get(projects_plan::list_plan).post(projects_plan::create_milestone),
        )
        .route(
            "/projects/milestones/{id}",
            get(projects_plan::get_milestone)
                .patch(projects_plan::update_milestone)
                .delete(projects_plan::delete_milestone),
        )
        .route(
            "/projects/milestones/{id}/done",
            post(projects_plan::set_milestone_done),
        )
        // A task's place in the plan, filed against the task whose place it is
        // (`projects.task.milestone.*`): one milestone per task, so putting it
        // somewhere is a PUT and moving it is the same call.
        .route(
            "/projects/tasks/{task_id}/milestone",
            put(projects_plan::place_task).delete(projects_plan::unplace_task),
        )
        // Templates (B3.09) — the boards a tenant has marked reusable, and the
        // copy that starts a new engagement from one. A template IS a project,
        // so it is addressed by the board's own id: there is no second record
        // to go stale when the board is edited.
        .route(
            "/projects/templates",
            get(projects_templates::list_templates).post(projects_templates::mark_template),
        )
        .route(
            "/projects/templates/{id}",
            delete(projects_templates::unmark_template),
        )
        .route(
            "/projects/templates/{id}/instantiate",
            post(projects_templates::instantiate_template),
        )
        // One engagement, registered last so the file reads in the order
        // matchit resolves: every literal segment above (`time`, `timer`,
        // `weeks`, `approvals`, `clients`, `unbilled`, `invoices`, `reports`,
        // `milestones`, `tasks`, `templates`)
        // wins over this capture, and an id — a base64url'd 16-byte token —
        // can never spell one of them anyway.
        .route("/projects/{id}", get(projects_clients::get_project))
        // Finance — expense claims (B4.05) and the flow that decides them.
        //
        // `/finance` is a NEW top-level prefix: the production Caddyfile needs
        // it added at the next deploy, and `web/vite.config.ts` carries it in
        // `API_PATHS` from this commit.
        //
        // Two doors onto one row, exactly as `/projects/weeks` and
        // `/projects/approvals` are. The CLAIMANT's routes are their own and
        // carry no `userId` anywhere; the APPROVER's are cross-user, gated by
        // `Account::require_admin` in the handler, and live in their own module
        // so the module's one privileged read cannot hide among ordinary ones.
        //
        // The queue is `/finance/expenses/pending` rather than a second
        // collection: the decisions are on the claim itself (the design note's
        // routes table), so the queue is a view of the same collection. `pending`
        // is a static segment and an id — a base64url'd 16-byte token — can
        // never spell it, the shape `/tasks/labels` beside `/tasks/{id}` has had
        // since ADR 0021.
        .route(
            "/finance/expenses",
            get(finance_expenses::list_expenses).post(finance_expenses::create_expense),
        )
        .route(
            "/finance/expenses/pending",
            get(finance_approvals::list_pending_expenses),
        )
        // The payer's queue beside the approver's, and a static segment for the
        // same reason `pending` is: what the company has approved and still owes
        // an employee for. Not `pending?status=approved` — an approved claim a
        // company card paid is approved and is NOT reimbursable, so the two
        // lists differ by more than a word (B4.13a).
        .route(
            "/finance/expenses/reimbursable",
            get(finance_approvals::list_reimbursable_expenses),
        )
        .route(
            "/finance/expenses/{id}",
            get(finance_expenses::get_expense)
                .patch(finance_expenses::update_expense)
                .delete(finance_expenses::delete_expense),
        )
        .route(
            "/finance/expenses/{id}/submit",
            post(finance_expenses::submit_expense),
        )
        .route(
            "/finance/expenses/{id}/withdraw",
            post(finance_expenses::withdraw_expense),
        )
        // Answering the agent's suggested category (B4.14a). The claimant's own
        // verbs on their own claim, so they sit with the rest of the personal
        // surface rather than with the tool that produced the suggestion.
        .route(
            "/finance/expenses/{id}/category/accept",
            post(finance_expenses::accept_expense_category),
        )
        .route(
            "/finance/expenses/{id}/category/decline",
            post(finance_expenses::decline_expense_category),
        )
        // Admin only, all three (`Account::require_admin`, checked in the
        // handler as `/admin/*` does).
        .route(
            "/finance/expenses/{id}/approve",
            post(finance_approvals::approve_expense),
        )
        .route(
            "/finance/expenses/{id}/reject",
            post(finance_approvals::reject_expense),
        )
        .route(
            "/finance/expenses/{id}/reimburse",
            post(finance_approvals::reimburse_expense),
        )
        // Reading an uploaded receipt (B4.06b): a POST that writes NOTHING —
        // the file is already in the caller's Drive, and the answer is candidate
        // fields for a human to confirm in the create form above. It joins
        // `audit_action::READ_ONLY_POSTS` beside `/crm/imports/leads/preview`
        // for that reason: an audit line saying somebody created something they
        // only looked at would be a false line in the log.
        .route("/finance/receipts", post(finance_receipts::read_receipt))
        // Mileage (B4.07): a journey is not an amount somebody types, it is a
        // distance at a rate the company published — so the rate table is a
        // route of its own, and the journey route takes no money at all.
        //
        // `rates` before `{id}` for the reason `pending` is registered before
        // `/finance/expenses/{id}`: matchit prefers the static segment, and an
        // id — a base64url'd 16-byte token — can never spell one.
        //
        // The two doors differ here in an unusual way and it is deliberate:
        // EVERYBODY reads the rate table (a traveller must know what a kilometre
        // is worth), only an ADMIN replaces it (`Account::require_admin`, in the
        // handler) — a rate table anybody could raise is a self-service pay
        // rise. The journeys below are the caller's own and carry no `userId`.
        .route(
            "/finance/mileage/rates",
            get(finance_mileage::list_mileage_rates).put(finance_mileage::replace_mileage_rates),
        )
        .route(
            "/finance/mileage",
            get(finance_mileage::list_mileage).post(finance_mileage::create_mileage),
        )
        .route(
            "/finance/mileage/{id}",
            delete(finance_mileage::delete_mileage),
        )
        // The bank (B4.08). One upload door for three formats — CAMT.053,
        // MT940 and a mapped CSV — because a person has a file, not a format;
        // the store sniffs which parser it wants unless the caller says.
        //
        // The preview writes nothing and joins `audit_action::READ_ONLY_POSTS`
        // beside `/crm/imports/leads/preview`: the store's reading is a pure
        // function, and an audit line saying somebody imported something they
        // only looked at would be a false line in the log.
        //
        // Both carry the statement file's own body limit rather than the JMAP
        // request limit, as `/crm/imports/leads` does, because the file IS the
        // body.
        .route(
            "/finance/imports/bank/preview",
            post(finance_bank::preview_bank_import)
                .layer(DefaultBodyLimit::max(alo_store::MAX_BANK_FILE_BYTES)),
        )
        .route(
            "/finance/imports/bank",
            post(finance_bank::import_bank_file)
                .layer(DefaultBodyLimit::max(alo_store::MAX_BANK_FILE_BYTES)),
        )
        // What was imported, and where each line stands. A statement is the
        // company's and not one employee's, so neither read is narrowed by user
        // — which is the point of importing a month once.
        .route(
            "/finance/bank/statements",
            get(finance_bank::list_bank_statements),
        )
        .route("/finance/bank/lines", get(finance_bank::list_bank_lines))
        // Reconciliation (B4.09c). The suggestions read is static and comes
        // before the `{id}` routes; matchit prefers the static segment anyway,
        // and keeping them in this order is what makes that obvious to a reader.
        .route(
            "/finance/bank/suggestions",
            get(finance_bank_match::list_bank_suggestions),
        )
        // Four named acts on one line, never a settable status: each has its own
        // consequences (a payment and two entries; a reversal; a sentence) and
        // the audit log records them by name.
        .route(
            "/finance/bank/lines/{id}/match",
            post(finance_bank_match::match_bank_line),
        )
        .route(
            "/finance/bank/lines/{id}/unmatch",
            post(finance_bank_match::unmatch_bank_line),
        )
        .route(
            "/finance/bank/lines/{id}/ignore",
            post(finance_bank_match::ignore_bank_line),
        )
        .route(
            "/finance/bank/lines/{id}/unignore",
            post(finance_bank_match::unignore_bank_line),
        )
        // Fiscal periods and the soft close (B4.10). Close and reopen are named
        // acts rather than a settable status: one shuts the books, the other
        // admits a reported period is being changed, and the audit trail records
        // them apart.
        .route(
            "/finance/periods",
            get(finance_periods::list_periods).post(finance_periods::create_period),
        )
        .route(
            "/finance/periods/{id}/close",
            post(finance_periods::close_period),
        )
        .route(
            "/finance/periods/{id}/reopen",
            post(finance_periods::reopen_period),
        )
        // The chart of accounts (B4.13c) — the list of places money can be, and
        // the doors a tenant edits it through. Admin or accountant on every one
        // of them, the reads included: the chart says what the company owes, is
        // owed and earns, and the list is also what SEEDS it on first use, so a
        // read here writes.
        //
        // `/finance/accounts` before `{id}` for the reason `pending` is
        // registered before `/finance/expenses/{id}`: matchit prefers the
        // static segment, and an id — a base64url'd 16-byte token — can never
        // spell one.
        //
        // Retiring an account is a field of the `PATCH` rather than a named act
        // (unlike the period close beside it): it is reversible, it decides
        // nothing, and the design note's own routes table says `deactivate` is
        // what an edit does.
        .route(
            "/finance/accounts",
            get(finance_chart::list_accounts).post(finance_chart::create_account),
        )
        .route(
            "/finance/accounts/{id}",
            get(finance_chart::get_account)
                .patch(finance_chart::update_account)
                .delete(finance_chart::delete_account),
        )
        // The reports (B4.11) — folds over the journal, each with a `.csv`
        // twin serving the same store read as a file. Admin only: a P&L is the
        // whole tenant's result, and B4.12's accountant role widens that gate
        // additively.
        .route("/finance/reports/pl", get(finance_report_pl::pl_report))
        .route(
            "/finance/reports/pl.csv",
            get(finance_report_pl::pl_report_csv),
        )
        .route(
            "/finance/reports/balance",
            get(finance_report_balance::balance_report),
        )
        .route(
            "/finance/reports/balance.csv",
            get(finance_report_balance::balance_report_csv),
        )
        .route(
            "/finance/reports/aged",
            get(finance_report_aged::aged_report),
        )
        .route(
            "/finance/reports/aged.csv",
            get(finance_report_aged::aged_report_csv),
        )
        .route(
            "/finance/reports/vat",
            get(finance_report_vat::vat_return_report),
        )
        .route(
            "/finance/reports/vat.csv",
            get(finance_report_vat::vat_return_report_csv),
        )
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
        // Tenant-wide scoped roles (B4.12) — today only `accountant`. Its own
        // route rather than a field beside `isAdmin`, because the admin flag is
        // the console and a role is a scope; a body that could set both would
        // make "make them an accountant" and "make them an admin" one call.
        .route("/admin/users/roles", post(admin::set_user_role))
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
        // One business record's history (B2.13) — the same log, read from the
        // record instead of the console, so it is open to any member of the
        // tenant rather than admins. `/audit` is a NEW top-level prefix: the
        // production Caddyfile needs it added at the next deploy, and the vite
        // dev proxy already has it.
        .route("/audit", get(audit::list_entity_audit))
        // Mail settings: the user's signature + the tenant footer.
        .route("/settings/mail", get(settings::mail_settings))
        .route("/settings/signature", post(settings::set_signature))
        .route("/settings/out-of-office", post(settings::set_out_of_office))
        .route("/admin/org-footer", post(settings::set_org_footer))
        .layer(DefaultBodyLimit::max(upload_limit))
        // The business audit trail (B2.13). Applied to the routes rather than
        // around the router, so it runs *after* routing and can read the
        // matched template — `/billing/invoices/{id}/issue` is what says which
        // segment is a record id. It short circuits for every method and prefix
        // it does not audit, which is nearly all traffic.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            audit_record::audit_business_mutations,
        ))
        // The scoped-role gate (B4.12), outside the audit layer so it runs
        // first: an accountant may read every billing and CRM record — they
        // must see the document behind a posting — and may change none of
        // them. One layer rather than a gate in sixty handlers, for the reason
        // the audit trail is one layer. It also short circuits for everything
        // that is not a mutating call into those two modules.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            scoped_roles::enforce_scoped_roles,
        ))
        .layer(Extension(site_domain_dns))
        .with_state(state);
    jmap.merge(identity_routes)
}

/// A convenience [`AppState`] with default limits and a fresh push hub.
pub fn app_state(store: Arc<Store>, identity: Identity, base_url: impl Into<String>) -> AppState {
    AppState {
        turns: crate::chat_turns::Turns::default(),
        media: crate::state::MediaEngine::from_env(),
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
