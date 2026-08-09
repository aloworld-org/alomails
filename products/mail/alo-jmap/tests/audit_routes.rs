//! Coverage of the business audit trail over the router's own source (B2.13).
//!
//! The item's promise is "every mutating billing/CRM route writes exactly one
//! entry" — and, since B3.04, every mutating `/projects` route too, and since
//! B4.05b every mutating `/finance` route.
//! `audit_http.rs` proves the *writing* on a live service, one route at a time;
//! what cannot be proved that way is **every**. axum's router does not hand back
//! the routes it holds, so this suite reads the source that registers them —
//! `server.rs` — and asserts that each mutating route under an audited module
//! resolves to an audit action.
//!
//! Reading source in a test is unusual enough to say why: the alternative is a
//! hand-maintained list of routes, which is the very thing that goes stale and
//! the very thing this item exists to avoid. The router registration is the
//! only place a route is declared, so it is the only honest input.
//!
//! The second assertion here is the **vocabulary**: the full list of actions
//! the log can contain, spelled out. It is meant to fail when an audited route
//! is added — that failure is the review, and the fix is to read the new line
//! and paste it in (or, if it reads wrong, to fix the derivation).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_jmap::audit_action::{event_for, is_mutating};

/// The router's source. Compiled in, so this suite tracks the file it is about.
const SERVER_SOURCE: &str = include_str!("../src/server.rs");

/// The HTTP method constructors axum routes are built from.
const VERBS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// The text inside every `.route( … )` call in the source, parentheses balanced
/// and string literals respected (a `(` inside a path would otherwise end the
/// call early).
fn route_calls(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut calls = Vec::new();
    let mut cursor = 0;
    while let Some(found) = source[cursor..].find(".route(") {
        let start = cursor + found + ".route(".len();
        let mut depth = 1_usize;
        let mut index = start;
        let mut in_string = false;
        while index < bytes.len() && depth > 0 {
            let c = bytes[index] as char;
            if in_string {
                if c == '\\' {
                    index += 2;
                    continue;
                }
                if c == '"' {
                    in_string = false;
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            index += 1;
        }
        calls.push(&source[start..index.saturating_sub(1)]);
        cursor = index;
    }
    calls
}

/// The route's path — the first string literal in the call.
fn template_of(call: &str) -> Option<&str> {
    let opening = call.find('"')? + 1;
    let rest = &call[opening..];
    let closing = rest.find('"')?;
    Some(&rest[..closing])
}

/// The HTTP methods a route call registers. A verb counts where it opens a
/// call — either on its own (`post(create)`) or chained onto the method router
/// (`get(list).post(create)`) — but not as the tail of a longer name
/// (`forget(`) or a path (`Something::get(`).
fn methods_of(call: &str) -> Vec<String> {
    let mut found = Vec::new();
    for verb in VERBS {
        let pattern = format!("{verb}(");
        let mut cursor = 0;
        while let Some(at) = call[cursor..].find(&pattern) {
            let position = cursor + at;
            let preceding = call[..position].chars().next_back();
            let bare = preceding.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == ':'));
            if bare {
                found.push(verb.to_ascii_uppercase());
                break;
            }
            cursor = position + pattern.len();
        }
    }
    found
}

/// The route prefixes this suite holds to the audit promise — the modules in
/// `audit_action::AUDITED_MODULES`, spelled out here rather than imported so
/// adding a module to that list without deciding what its trail says fails
/// loudly instead of quietly widening the promise.
const AUDITED_PREFIXES: [&str; 4] = ["/billing/", "/crm/", "/finance/", "/projects/"];

/// Every `(method, template)` the router registers under an audited module.
fn business_routes() -> Vec<(String, String)> {
    let mut routes: Vec<(String, String)> = route_calls(SERVER_SOURCE)
        .into_iter()
        .filter_map(|call| template_of(call).map(|template| (template, call)))
        .filter(|(template, _)| {
            AUDITED_PREFIXES
                .iter()
                .any(|prefix| template.starts_with(prefix))
        })
        .flat_map(|(template, call)| {
            methods_of(call)
                .into_iter()
                .map(move |method| (method, template.to_owned()))
        })
        .collect();
    routes.sort();
    routes.dedup();
    routes
}

#[test]
fn the_router_source_parses_into_the_routes_it_registers() {
    let routes = business_routes();
    assert!(
        routes.len() > 40,
        "the parser found only {} audited routes — it is broken, not the router",
        routes.len()
    );
    assert!(
        routes.contains(&("POST".to_owned(), "/billing/invoices".to_owned())),
        "the parser missed a route everyone knows exists"
    );
    assert!(
        routes.contains(&(
            "DELETE".to_owned(),
            "/billing/invoices/{id}/payments/{payment_id}".to_owned()
        )),
        "the parser missed a two-parameter route"
    );
}

#[test]
fn every_mutating_business_route_resolves_to_an_audit_action() {
    // The one documented exception, restated here rather than imported: if the
    // list of dry runs grows, this test should be the thing that makes someone
    // say so out loud.
    const DRY_RUNS: [&str; 2] = ["/crm/imports/leads/preview", "/finance/receipts"];

    for (method, template) in business_routes() {
        if !is_mutating(&method) {
            continue;
        }
        let event = event_for(&method, &template, &template);
        if DRY_RUNS.contains(&template.as_str()) {
            assert!(
                event.is_none(),
                "{method} {template} is a dry run and must not be audited"
            );
            continue;
        }
        let event =
            event.unwrap_or_else(|| panic!("{method} {template} resolves to no audit action"));
        assert!(
            event.action.starts_with(&event.entity_type),
            "{method} {template}: action {} is not about {}",
            event.action,
            event.entity_type
        );
        assert!(
            event.action.split('.').all(|part| !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')),
            "{method} {template}: action {} is not a dotted lowercase verb",
            event.action
        );
        assert!(
            event.action.split('.').count() >= 3,
            "{method} {template}: action {} says no verb",
            event.action
        );
    }
}

#[test]
fn the_audit_vocabulary_is_what_it_is_expected_to_be() {
    let lines: Vec<String> = business_routes()
        .into_iter()
        .filter(|(method, _)| is_mutating(method))
        .filter_map(|(method, template)| {
            event_for(&method, &template, &template)
                .map(|event| format!("{method} {template} -> {}", event.action))
        })
        .collect();
    assert_eq!(
        lines.join("\n"),
        EXPECTED_VOCABULARY.trim(),
        "the audit vocabulary changed. Read the diff: if the new action reads \
         right, paste it in; if it does not, fix audit_action.rs"
    );
}

/// Every action the business audit log can contain today, in route order.
const EXPECTED_VOCABULARY: &str = r#"
DELETE /billing/bills/{id} -> billing.bill.delete
DELETE /billing/invoices/{id} -> billing.invoice.delete
DELETE /billing/invoices/{id}/payments/{payment_id} -> billing.invoice.payment.delete
DELETE /billing/quotes/{id} -> billing.quote.delete
DELETE /billing/schedules/{id} -> billing.schedule.delete
DELETE /crm/activities/{id} -> crm.activity.delete
DELETE /crm/deals/{id} -> crm.deal.delete
DELETE /crm/deals/{id}/threads/{threadId} -> crm.deal.thread.delete
DELETE /crm/stages/{id} -> crm.stage.delete
DELETE /finance/expenses/{id} -> finance.expense.delete
DELETE /projects/clients/{id} -> projects.client.delete
DELETE /projects/milestones/{id} -> projects.milestone.delete
DELETE /projects/tasks/{task_id}/milestone -> projects.task.milestone.delete
DELETE /projects/templates/{id} -> projects.template.delete
DELETE /projects/time/{id} -> projects.time.delete
PATCH /billing/customers/{id} -> billing.customer.update
PATCH /billing/invoices/{id} -> billing.invoice.update
PATCH /billing/products/{id} -> billing.product.update
PATCH /billing/quotes/{id} -> billing.quote.update
PATCH /billing/schedules/{id} -> billing.schedule.update
PATCH /billing/settings -> billing.setting.update
PATCH /crm/deals/{id} -> crm.deal.update
PATCH /crm/pipelines/{id} -> crm.pipeline.update
PATCH /crm/stages/{id} -> crm.stage.update
PATCH /finance/expenses/{id} -> finance.expense.update
PATCH /projects/milestones/{id} -> projects.milestone.update
PATCH /projects/time/{id} -> projects.time.update
POST /billing/bills/import -> billing.bill.import
POST /billing/bills/sepa.xml -> billing.bill.sepa_xml
POST /billing/bills/{id}/approve -> billing.bill.approve
POST /billing/bills/{id}/reject -> billing.bill.reject
POST /billing/customers -> billing.customer.create
POST /billing/customers/{id}/archive -> billing.customer.archive
POST /billing/fx/rates/import -> billing.fx.rates.import
POST /billing/invoices -> billing.invoice.create
POST /billing/invoices/{id}/credit-note -> billing.invoice.credit_note
POST /billing/invoices/{id}/issue -> billing.invoice.issue
POST /billing/invoices/{id}/payments -> billing.invoice.payment.create
POST /billing/invoices/{id}/reminder -> billing.invoice.reminder
POST /billing/invoices/{id}/send -> billing.invoice.send
POST /billing/invoices/{id}/void -> billing.invoice.void
POST /billing/products -> billing.product.create
POST /billing/products/{id}/archive -> billing.product.archive
POST /billing/quotes -> billing.quote.create
POST /billing/quotes/{id}/accept -> billing.quote.accept
POST /billing/quotes/{id}/decline -> billing.quote.decline
POST /billing/quotes/{id}/expire -> billing.quote.expire
POST /billing/quotes/{id}/send -> billing.quote.send
POST /billing/schedules -> billing.schedule.create
POST /billing/schedules/run -> billing.schedule.run
POST /billing/schedules/{id}/pause -> billing.schedule.pause
POST /billing/schedules/{id}/resume -> billing.schedule.resume
POST /crm/deals -> crm.deal.create
POST /crm/deals/{id}/activities -> crm.deal.activity.create
POST /crm/deals/{id}/invoice -> crm.deal.invoice
POST /crm/deals/{id}/next-steps -> crm.deal.next_step.create
POST /crm/deals/{id}/quote -> crm.deal.quote
POST /crm/deals/{id}/stage -> crm.deal.stage
POST /crm/deals/{id}/threads -> crm.deal.thread.create
POST /crm/imports/leads -> crm.import.lead.create
POST /crm/pipelines -> crm.pipeline.create
POST /crm/pipelines/{id}/archive -> crm.pipeline.archive
POST /crm/pipelines/{id}/stages -> crm.pipeline.stage.create
POST /crm/stages/{id}/archive -> crm.stage.archive
POST /crm/stages/{id}/move -> crm.stage.move
POST /finance/expenses -> finance.expense.create
POST /finance/expenses/{id}/approve -> finance.expense.approve
POST /finance/expenses/{id}/reimburse -> finance.expense.reimburse
POST /finance/expenses/{id}/reject -> finance.expense.reject
POST /finance/expenses/{id}/submit -> finance.expense.submit
POST /finance/expenses/{id}/withdraw -> finance.expense.withdraw
POST /projects/approvals/{id}/approve -> projects.approval.approve
POST /projects/approvals/{id}/reject -> projects.approval.reject
POST /projects/approvals/{id}/reopen -> projects.approval.reopen
POST /projects/invoices -> projects.invoice.create
POST /projects/milestones -> projects.milestone.create
POST /projects/milestones/{id}/done -> projects.milestone.done
POST /projects/templates -> projects.template.create
POST /projects/templates/{id}/instantiate -> projects.template.instantiate
POST /projects/time -> projects.time.create
POST /projects/time/{id}/accept -> projects.time.accept
POST /projects/time/{id}/reject -> projects.time.reject
POST /projects/timer/start -> projects.timer.start
POST /projects/timer/stop -> projects.timer.stop
POST /projects/weeks/{monday}/submit -> projects.week.submit
POST /projects/weeks/{monday}/withdraw -> projects.week.withdraw
PUT /billing/fx/rates -> billing.fx.rates.update
PUT /projects/clients/{id} -> projects.client.update
PUT /projects/tasks/{task_id}/milestone -> projects.task.milestone.update
"#;
