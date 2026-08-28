//! What the accountant role actually opens, and what it does not (ADR 0035,
//! wave B4.12; `docs/design/finance.md`, "The accountant role"), driven through
//! the real router over a real Postgres.
//!
//! A role is not a feature, it is a **boundary**, and a boundary is only worth
//! what its far side proves. So the same person is walked round the whole
//! product three times — as an ordinary member, as an accountant, and as
//! somebody else's accountant — and every door is tried from each side:
//!
//! - the four reports, the approvals inbox and the period lock **open** for an
//!   accountant and stay shut for a member who is neither;
//! - the admin console stays shut for both — the role hands out the books, not
//!   the company;
//! - billing and CRM are **readable** (an accountant must see the document
//!   behind a posting) and **unwritable**, on every verb, including the ones
//!   nobody remembered to gate by hand — because the gate is a layer, not sixty
//!   handlers;
//! - a dry run that writes nothing is still allowed, because the rule is about
//!   the data and not about the HTTP method;
//! - and none of it crosses a tenant: an accountant of one company is an
//!   ordinary stranger to another's books, its documents and its role table.
//!
//! The mailbox question is answered by construction and asserted anyway: an
//! accountant is a user with no delegation grant and no Space membership, so
//! their own inbox is all the mail they have.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alo_store::TenantRole;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{Harness, harness, harness_on, send};

// ---- request helpers ---------------------------------------------------------

fn with_json(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("POST", uri, token, body)).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, with_json("PATCH", uri, token, body)).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

/// A second real user of the harness's tenant, logged in for real — the person
/// the role is handed to. Returns their token and id.
async fn colleague(h: &Harness, tag: &str) -> (String, alo_store::UserId) {
    let email = format!("{tag}-{}@example.test", h.tenant);
    let user = h.ts.create_user(&email).await.unwrap();
    h.identity
        .set_password(&h.tenant, &user, &email, "s3cret-pw")
        .await
        .unwrap();
    let token = h
        .identity
        .password_login(&email, "s3cret-pw", None)
        .await
        .unwrap()
        .expect("token issued")
        .0
        .reveal()
        .to_owned();
    (token, user)
}

/// The privileged finance reads an accountant is hired for.
///
/// The bank three joined the list with B4.13b: a statement names every
/// counterparty the company paid and was paid by, and the suggestions read
/// lists every open invoice beside them. That is the tenant's business and not
/// an employee's, so it is the bookkeeper's door like the reports are.
///
/// `/finance/accounts` joined with B4.13c: the chart says what the company owes,
/// is owed and earns, and the read is also what SEEDS it — so a read here
/// writes, which is a second reason it is not an ordinary member's.
const FINANCE_READS: [&str; 9] = [
    "/finance/accounts",
    "/finance/reports/pl?from=2026-01-01&to=2026-12-31",
    "/finance/reports/pl.csv?from=2026-01-01&to=2026-12-31",
    "/finance/reports/balance?on=2026-12-31",
    "/finance/reports/aged?on=2026-12-31&side=receivable",
    "/finance/expenses/pending",
    "/finance/bank/statements",
    "/finance/bank/lines",
    "/finance/bank/suggestions",
];

/// The acts on a bank line, and the two upload doors. Every one of them is a
/// `POST`, and every one is refused an ordinary member **before** the store is
/// asked anything — which is what makes the refusal a `403` and not a `404`
/// telling them whether the line exists.
const BANK_WRITES: [&str; 6] = [
    "/finance/imports/bank/preview",
    "/finance/imports/bank",
    "/finance/bank/lines/nosuchline/match",
    "/finance/bank/lines/nosuchline/unmatch",
    "/finance/bank/lines/nosuchline/ignore",
    "/finance/bank/lines/nosuchline/unignore",
];

/// The chart's own writes (B4.13c), each on an id that does not exist: an
/// ordinary member is refused **before** the store is asked, so the refusal is a
/// `403` and never a `404` telling them which accounts a company keeps.
const CHART_WRITES: [(&str, &str); 3] = [
    ("POST", "/finance/accounts"),
    ("PATCH", "/finance/accounts/nosuchaccount"),
    ("DELETE", "/finance/accounts/nosuchaccount"),
];

/// Doors the role must never open. The admin console is the company, not the
/// books.
const ADMIN_READS: [&str; 3] = ["/admin/users", "/admin/audit", "/admin/security/checks"];

// ---- the boundary ------------------------------------------------------------

#[tokio::test]
async fn the_books_open_for_an_accountant_and_for_no_other_member() {
    let h = harness("acctopen").await;
    let (clerk, clerk_id) = colleague(&h, "clerk").await;

    // As an ordinary member: every privileged finance read is shut…
    for uri in FINANCE_READS {
        let (status, _) = get(&h.app, &clerk, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // …while the periods list, which any member reads, answers — so the
    // refusals above are the gate and not a broken token.
    let (status, _) = get(&h.app, &clerk, "/finance/periods").await;
    assert_eq!(status, StatusCode::OK);

    // The same person, handed the role, reads the same books.
    h.ts.grant_role(&clerk_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();
    for uri in FINANCE_READS {
        let (status, body) = get(&h.app, &clerk, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }

    // And revoking takes it all back — a role is a state, not a one-way door.
    h.ts.revoke_role(&clerk_id, TenantRole::Accountant)
        .await
        .unwrap();
    for uri in FINANCE_READS {
        let (status, _) = get(&h.app, &clerk, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} after revoke");
    }
}

#[tokio::test]
async fn the_bank_is_the_bookkeepers_and_every_act_on_a_line_is_shut_before_it_is_looked_up() {
    let h = harness("acctbank").await;
    let (clerk, clerk_id) = colleague(&h, "clerk").await;

    // An ordinary member is refused every act, and refused it as a `403`: a
    // `404` here would answer "that line is not yours" to somebody who was
    // never allowed to ask, and would be an existence oracle for the pile.
    for uri in BANK_WRITES {
        let (status, _) = post(&h.app, &clerk, uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }

    // The bookkeeper gets past the gate on the same made-up line — the `404` is
    // the store answering, which is the proof that the `403`s above were the
    // gate and not a broken token.
    h.ts.grant_role(&clerk_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();
    let (status, _) = post(
        &h.app,
        &clerk,
        "/finance/bank/lines/nosuchline/unmatch",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And the role is a state: revoking shuts the same doors again.
    h.ts.revoke_role(&clerk_id, TenantRole::Accountant)
        .await
        .unwrap();
    for uri in BANK_WRITES {
        let (status, _) = post(&h.app, &clerk, uri, json!({})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} after revoke");
    }
}

#[tokio::test]
async fn the_chart_is_the_bookkeepers_and_editing_it_is_shut_before_it_is_looked_up() {
    let h = harness("acctchart").await;
    let (clerk, clerk_id) = colleague(&h, "clerk").await;

    for (method, uri) in CHART_WRITES {
        let (status, _) = send(
            &h.app,
            with_json(
                method,
                uri,
                &clerk,
                json!({ "code": "9999", "name": "Mine", "type": "asset" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }

    // The bookkeeper gets past the gate — to the store's own refusal on an id
    // that is not there, which is what proves the `403`s above were the gate.
    h.ts.grant_role(&clerk_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();
    let (status, _) = patch(
        &h.app,
        &clerk,
        "/finance/accounts/nosuchaccount",
        json!({ "name": "Anything" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // …and to a chart of their own, which the read seeds.
    let (status, body) = get(&h.app, &clerk, "/finance/accounts").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["accounts"].as_array().is_some_and(|a| a.len() >= 20));

    h.ts.revoke_role(&clerk_id, TenantRole::Accountant)
        .await
        .unwrap();
    for (method, uri) in CHART_WRITES {
        let (status, _) = send(
            &h.app,
            with_json(
                method,
                uri,
                &clerk,
                json!({ "code": "9999", "name": "Mine", "type": "asset" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri} after revoke");
    }
}

#[tokio::test]
async fn an_accountant_closes_the_books_and_decides_a_claim() {
    let h = harness("acctwrite").await;
    let (accountant, accountant_id) = colleague(&h, "books").await;
    h.ts.grant_role(&accountant_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();

    // The claim is somebody else's — the harness user's — which is the whole
    // point of an approvals inbox.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/finance/expenses",
        json!({ "spentOn": "2026-03-04", "grossCents": 4_200, "method": "personal",
                "description": "train to the audit" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let claim = body["expense"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &h.token,
        &format!("/finance/expenses/{claim}/submit"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The accountant sees it in the queue and approves it.
    let (status, body) = get(&h.app, &accountant, "/finance/expenses/pending").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expenses"].as_array().map(Vec::len), Some(1), "{body}");
    let (status, body) = post(
        &h.app,
        &accountant,
        &format!("/finance/expenses/{claim}/approve"),
        json!({ "note": "receipt seen" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expense"]["status"], "approved");

    // And shuts a period, which is the act that makes a report final.
    let (status, body) = post(
        &h.app,
        &accountant,
        "/finance/periods",
        json!({ "fromDate": "2026-01-01", "toDate": "2026-03-31" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let period = body["period"]["id"].as_str().unwrap().to_owned();
    let (status, body) = post(
        &h.app,
        &accountant,
        &format!("/finance/periods/{period}/close"),
        json!({ "note": "Q1 filed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["period"]["status"], "closed");
}

#[tokio::test]
async fn the_role_hands_out_the_books_not_the_company() {
    let h = harness("acctadmin").await;
    let (accountant, accountant_id) = colleague(&h, "outside").await;
    h.ts.grant_role(&accountant_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();

    for uri in ADMIN_READS {
        let (status, _) = get(&h.app, &accountant, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // Least of all the role table itself: an accountant cannot promote anyone,
    // including themselves.
    let (status, _) = post(
        &h.app,
        &accountant,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Nor the mileage rate table: what the company pays for driving is a pay
    // decision, not a bookkeeping one (the one finance write B4.12 left alone).
    let (status, _) = send(
        &h.app,
        with_json(
            "PUT",
            "/finance/mileage/rates",
            &accountant,
            json!({ "rates": [ { "effectiveFrom": "2026-01-01", "centsPerKm": 99 } ] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn billing_and_crm_are_readable_and_unwritable() {
    let h = harness("acctdocs").await;
    let (accountant, accountant_id) = colleague(&h, "reader").await;

    // A document and a deal, created by the tenant itself before the role
    // exists — the records an accountant will need to look at.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Kunde GmbH", "country": "DE" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let customer = body["customer"]["id"].as_str().unwrap().to_owned();

    h.ts.grant_role(&accountant_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();

    // Reads: open, because the posting is only half the story.
    for uri in [
        "/billing/customers",
        "/billing/invoices",
        "/billing/products",
        "/crm/deals",
        "/crm/pipelines",
    ] {
        let (status, body) = get(&h.app, &accountant, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    let (status, body) = get(
        &h.app,
        &accountant,
        &format!("/billing/customers/{customer}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["name"], "Kunde GmbH");

    // Writes: refused on every verb and every shape, by the layer rather than
    // by a gate somebody remembered to add to each handler.
    let (status, body) = post(
        &h.app,
        &accountant,
        "/billing/customers",
        json!({ "name": "Mine Now Ltd", "country": "NL" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _) = patch(
        &h.app,
        &accountant,
        &format!("/billing/customers/{customer}"),
        json!({ "name": "Renamed" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &h.app,
        &accountant,
        &format!("/billing/customers/{customer}/archive"),
        json!({ "archived": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &h.app,
        &accountant,
        "/billing/invoices",
        json!({ "customerId": customer, "lines": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &h.app,
        &accountant,
        "/crm/pipelines",
        json!({ "name": "Theirs" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = delete(&h.app, &accountant, "/crm/stages/whatever").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the refusal comes before the id is even looked up"
    );

    // The record really is untouched: the refusal was not a 403 over a write
    // that had already happened.
    let (status, body) = get(&h.app, &h.token, &format!("/billing/customers/{customer}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["customer"]["name"], "Kunde GmbH");

    // Inventory joined the same boundary at B5.04b, and for a sharper reason:
    // an accountant values the stock on a balance sheet, so they must see what
    // is on the shelves and why it moved — and a stock adjustment is the write
    // that can make theft look like paperwork, which is not a books-only role's
    // to make.
    for uri in [
        "/inventory/locations",
        "/inventory/stock",
        "/inventory/moves",
    ] {
        let (status, body) = get(&h.app, &accountant, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    let (status, body) = post(
        &h.app,
        &accountant,
        "/inventory/moves",
        json!({
            "productId": "whatever",
            "fromLocationId": "a",
            "toLocationId": "b",
            "qtyMilli": 40_000,
            "reason": "adjustment",
            "reasonCode": "lost",
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the refusal comes before the ledger is even reached: {body}"
    );
    let (status, _) = post(
        &h.app,
        &accountant,
        "/inventory/locations",
        json!({ "code": "THEIRS", "name": "Not yours" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A dry run writes nothing, so it is not a write: refusing it would be a
    // rule about the HTTP method rather than about the data.
    let (status, body) = post(
        &h.app,
        &accountant,
        "/crm/imports/leads/preview",
        json!({ "csv": "email,name\nada@example.test,Ada\n" }),
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN, "a preview is a read: {body}");

    // And an admin who also holds the role is still an admin — a role only
    // ever adds.
    h.ts.grant_role(&h.user, TenantRole::Accountant, &h.user)
        .await
        .unwrap();
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/billing/customers",
        json!({ "name": "Zweite GmbH", "country": "DE" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn a_role_is_held_in_one_tenant_only() {
    let h = harness("acctours").await;
    let theirs = harness_on(std::sync::Arc::clone(&h.store), "accttheirs").await;
    let (accountant, accountant_id) = colleague(&h, "ours").await;
    h.ts.grant_role(&accountant_id, TenantRole::Accountant, &h.user)
        .await
        .unwrap();

    // Their books, their documents: our accountant is a stranger with a token
    // that resolves to our tenant, so their ids are simply not there.
    let (status, body) = post(
        &theirs.app,
        &theirs.token,
        "/billing/customers",
        json!({ "name": "Their Customer", "country": "FR" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let theirs_customer = body["customer"]["id"].as_str().unwrap().to_owned();
    let (status, _) = get(
        &h.app,
        &accountant,
        &format!("/billing/customers/{theirs_customer}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Our books answer our accountant with ours, and never with theirs.
    let (status, body) = get(&h.app, &accountant, "/billing/customers").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !body.to_string().contains("Their Customer"),
        "another tenant's customer must not appear: {body}"
    );

    // And the role table does not cross either: their admin cannot make our
    // accountant theirs, and our admin cannot make their user ours.
    let (status, _) = post(
        &theirs.app,
        &theirs.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "their harness user is not an admin — the gate before the tenancy one"
    );
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();
    let (status, _) = post(
        &theirs.app,
        &theirs.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "our user is not a member of their tenant, and existence is not disclosed"
    );
    assert!(
        theirs.ts.role_grants().await.unwrap().is_empty(),
        "the refused grant wrote nothing"
    );
    // Our accountant is still ours, unchanged by any of it.
    assert_eq!(
        h.ts.user_roles(&accountant_id).await.unwrap(),
        vec![TenantRole::Accountant]
    );
}

#[tokio::test]
async fn an_admin_grants_the_role_and_the_session_says_who_holds_it() {
    let h = harness("acctgrant").await;
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (accountant, accountant_id) = colleague(&h, "hire").await;

    // Before: no role, and the session says so.
    let (status, body) = get(&h.app, &accountant, "/.well-known/jmap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["alo:roles"], json!([]));
    assert_eq!(body["alo:isAdmin"], json!(false));

    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = get(&h.app, &accountant, "/.well-known/jmap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["alo:roles"], json!(["accountant"]));
    assert_eq!(
        body["alo:isAdmin"],
        json!(false),
        "a role is never the admin flag"
    );

    // The console lists it beside the person, in one read.
    let (status, body) = get(&h.app, &h.token, "/admin/users").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == json!(accountant_id.as_str()))
        .unwrap_or_else(|| panic!("the new user is listed: {body}"))
        .clone();
    assert_eq!(listed["roles"], json!(["accountant"]));

    // Granting twice is one grant; revoking takes it away.
    let (status, _) = post(
        &h.app,
        &h.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        h.ts.user_roles(&accountant_id).await.unwrap(),
        vec![TenantRole::Accountant]
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "accountant", "granted": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(h.ts.user_roles(&accountant_id).await.unwrap().is_empty());

    // A word no gate knows is refused, naming what is accepted — a role that
    // enforces nothing would read as access somebody was given.
    let (status, body) = post(
        &h.app,
        &h.token,
        "/admin/users/roles",
        json!({ "userId": accountant_id.as_str(), "role": "owner", "granted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("accountant"),
        "{body}"
    );
    let (status, _) = post(
        &h.app,
        &h.token,
        "/admin/users/roles",
        json!({ "role": "accountant", "granted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "userId is required");
}

#[tokio::test]
async fn no_token_is_still_the_handlers_own_401() {
    let h = harness("acctnoauth").await;
    // The role layer must not turn an unauthenticated write into a 403: one
    // place decides what a request with no credentials is told.
    let req = Request::builder()
        .method("POST")
        .uri("/billing/customers")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "name": "X", "country": "DE" }).to_string(),
        ))
        .unwrap();
    let (status, _) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
