//! The chart of accounts over HTTP (ADR 0035, wave B4.13c), driven through the
//! real router over a real Postgres.
//!
//! Five claims, each one something a chart editor can get silently wrong about
//! a set of books:
//!
//! - **the first read seeds a working chart, in the caller's language**, and
//!   the second read does not seed it again — a tenant handed twenty accounts
//!   twice has two of everything, and a tenant handed none cannot issue an
//!   invoice at all;
//! - **a rename or a renumber leaves the posting rules alone**: the role
//!   survives, and the account that resolves `ar` is still the same row;
//! - **`includeInactive` is spelled the way a client sends it** — the regression
//!   that shipped for exactly as long as it took to curl it;
//! - **a period puts the journal's own figures on the chart**, and no period
//!   states nothing at all;
//! - **another tenant's account id is a `404` and changes nothing**, which is
//!   the isolation proof this file exists for.
//!
//! The gate (admin-or-accountant, on every door here including the reads) is
//! proven by `accountant_role_http.rs`, where the whole finance boundary is
//! walked from both sides.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, harness_on, send};

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

/// The account of a chart with a given code.
fn by_code<'a>(body: &'a Value, code: &str) -> &'a Value {
    body["accounts"]
        .as_array()
        .unwrap_or_else(|| panic!("no accounts in {body}"))
        .iter()
        .find(|account| account["code"] == code)
        .unwrap_or_else(|| panic!("no {code} in {body}"))
}

#[tokio::test]
async fn the_first_read_writes_a_working_chart_in_the_readers_language_and_the_second_does_not() {
    let h = harness("chartseed").await;
    // The chart is the bookkeepers\' door (`accountant_role_http.rs` walks the
    // gate itself); this test is about what is behind it.
    h.ts.set_admin(&h.user, true).await.unwrap();

    let (status, body) = get(&h.app, &h.token, "/finance/accounts?lang=fr").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["seeded"], true, "the read that wrote it says so");
    let count = body["accounts"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    assert!(count >= 20, "a working chart, not a stub: {count}");
    // In the language of whoever opened it — no English hardcoded in the store.
    assert_eq!(by_code(&body, "1000")["name"], "Banque");
    assert_eq!(by_code(&body, "1100")["role"], "ar");
    // Every posting rule resolves, which is what lets an invoice book on day one.
    for role in ["ar", "ap", "bank", "vat_output", "vat_input", "revenue"] {
        assert!(
            body["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|account| account["role"] == role),
            "no account holds {role}"
        );
    }

    // The second read seeds nothing — not even in another language.
    let (status, again) = get(&h.app, &h.token, "/finance/accounts?lang=nl").await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["seeded"], false);
    assert_eq!(again["accounts"].as_array().map(Vec::len), Some(count));
    assert_eq!(
        by_code(&again, "1000")["name"],
        "Banque",
        "a name the tenant now owns is never retranslated"
    );
}

#[tokio::test]
async fn a_renumbering_leaves_every_posting_rule_where_it_was() {
    let h = harness("chartrename").await;
    // The chart is the bookkeepers\' door (`accountant_role_http.rs` walks the
    // gate itself); this test is about what is behind it.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (_, chart) = get(&h.app, &h.token, "/finance/accounts").await;
    let receivables = by_code(&chart, "1100")["id"].as_str().unwrap().to_owned();

    // The accountant's own numbering, and a name to match. Nothing is said
    // about the role.
    let (status, body) = patch(
        &h.app,
        &h.token,
        &format!("/finance/accounts/{receivables}"),
        json!({ "code": "1400", "name": "Debiteuren" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["account"]["code"], "1400");
    assert_eq!(body["account"]["name"], "Debiteuren");
    assert_eq!(
        body["account"]["role"], "ar",
        "a rename that dropped this would stop invoices booking"
    );

    // And the store still answers the posting rule with the same row.
    assert_eq!(
        h.acc
            .fin_account_for_role(alo_store::AccountRole::Ar)
            .await
            .unwrap()
            .map(|account| account.id.as_str().to_owned()),
        Some(receivables)
    );
}

#[tokio::test]
async fn a_retired_account_is_out_of_the_list_until_it_is_asked_for() {
    let h = harness("chartretire").await;
    // The chart is the bookkeepers\' door (`accountant_role_http.rs` walks the
    // gate itself); this test is about what is behind it.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (status, body) = post(
        &h.app,
        &h.token,
        "/finance/accounts",
        json!({ "code": "6110", "name": "Hosting", "type": "expense" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hosting = body["account"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["account"]["system"], false, "the tenant's own line");

    let (_, body) = patch(
        &h.app,
        &h.token,
        &format!("/finance/accounts/{hosting}"),
        json!({ "active": false }),
    )
    .await;
    assert_eq!(body["account"]["active"], false);
    assert_eq!(body["account"]["code"], "6110", "and nothing else moved");

    let (_, plain) = get(&h.app, &h.token, "/finance/accounts").await;
    assert!(
        !plain["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["code"] == "6110")
    );
    // The parameter is spelled the way a client sends it. Serde's default snake
    // case made this a filter the server ignored, and only the wire found it.
    let (_, asked) = get(&h.app, &h.token, "/finance/accounts?includeInactive=true").await;
    assert!(
        asked["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["code"] == "6110"),
        "a retired account cannot be brought back if it cannot be seen: {asked}"
    );

    // An account nothing has used is the tenant's to remove; a seeded one is not.
    let (status, _) = delete(&h.app, &h.token, &format!("/finance/accounts/{hosting}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, chart) = get(&h.app, &h.token, "/finance/accounts").await;
    let seeded = by_code(&chart, "1000")["id"].as_str().unwrap().to_owned();
    let (status, body) = delete(&h.app, &h.token, &format!("/finance/accounts/{seeded}")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("deactivate it instead"),
        "the refusal says what to do instead: {body}"
    );
}

#[tokio::test]
async fn a_period_puts_the_journals_own_figures_on_the_chart_and_no_period_states_nothing() {
    let h = harness("chartfigures").await;
    // The chart is the bookkeepers\' door (`accountant_role_http.rs` walks the
    // gate itself); this test is about what is behind it.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let (_, chart) = get(&h.app, &h.token, "/finance/accounts").await;
    // No period asked for: nothing is claimed about the books.
    assert_eq!(chart["currency"], Value::Null);
    assert_eq!(by_code(&chart, "1000")["balanceCents"], Value::Null);
    assert_eq!(by_code(&chart, "1000")["postings"], Value::Null);

    let (status, over) = get(
        &h.app,
        &h.token,
        "/finance/accounts?from=2026-01-01&to=2026-12-31",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{over}");
    assert_eq!(over["currency"], "EUR", "the unit the figures are in");
    // An account the period never moved is a zero, not an absence.
    assert_eq!(by_code(&over, "1000")["balanceCents"], 0);
    assert_eq!(by_code(&over, "1000")["postings"], 0);

    // Half a period is refused rather than folded open-ended.
    let (status, body) = get(&h.app, &h.token, "/finance/accounts?from=2026-01-01").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .starts_with("to"),
        "the refusal names the end that is missing: {body}"
    );
}

#[tokio::test]
async fn another_tenants_account_is_not_there_and_is_not_changed() {
    let h = harness("chartmine").await;
    // The chart is the bookkeepers\' door (`accountant_role_http.rs` walks the
    // gate itself); this test is about what is behind it.
    h.ts.set_admin(&h.user, true).await.unwrap();
    let theirs = harness_on(std::sync::Arc::clone(&h.store), "charttheirs").await;
    theirs.ts.set_admin(&theirs.user, true).await.unwrap();

    // Their chart, and one line of their own on it.
    let (_, their_chart) = get(&theirs.app, &theirs.token, "/finance/accounts?lang=nl").await;
    let their_bank = by_code(&their_chart, "1000")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, body) = post(
        &theirs.app,
        &theirs.token,
        "/finance/accounts",
        json!({ "code": "6110", "name": "Hosting", "type": "expense" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let their_custom = body["account"]["id"].as_str().unwrap().to_owned();

    // Ours is an admin of ours and a stranger to theirs, on every door.
    for uri in [&their_bank, &their_custom] {
        let (status, _) = get(&h.app, &h.token, &format!("/finance/accounts/{uri}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "read {uri}");
        let (status, _) = patch(
            &h.app,
            &h.token,
            &format!("/finance/accounts/{uri}"),
            json!({ "name": "Mine now", "role": "" }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "write {uri}");
        let (status, _) = delete(&h.app, &h.token, &format!("/finance/accounts/{uri}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "delete {uri}");
    }

    // And their rows are byte-identical afterwards — a refusal that still wrote
    // would be the worst possible outcome of this test passing.
    let (_, after) = get(&theirs.app, &theirs.token, "/finance/accounts").await;
    assert_eq!(by_code(&after, "1000")["name"], "Bank");
    assert_eq!(by_code(&after, "1000")["role"], "bank");
    assert_eq!(by_code(&after, "6110")["name"], "Hosting");
    assert_eq!(by_code(&after, "6110")["id"], their_custom.as_str());

    // Our own chart never held either of them.
    let (_, ours) = get(&h.app, &h.token, "/finance/accounts?includeInactive=true").await;
    assert!(
        !ours["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["id"] == their_bank.as_str()
                || account["id"] == their_custom.as_str())
    );
}
