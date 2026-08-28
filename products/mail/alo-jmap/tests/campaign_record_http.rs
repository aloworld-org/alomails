//! `/campaigns/campaigns` over the real router (C3.1, ADR 0044) — writing the
//! letter, reading it back, and the three things this surface must never do.
//!
//! `alo-store`'s suite proves the record and its rules. What is asserted here is
//! the **edge**:
//!
//! - **The body a composer sends is the body it gets back.** Block for block,
//!   field for field, through JSON in both directions — because wave C3.2's
//!   golden files compile exactly this JSON into what a customer's customers
//!   receive.
//! - **Nothing here sends.** There is no route that puts a campaign in front of
//!   anybody, and the test says so by asking for one: a `send` is a `404`/`405`,
//!   not a `403`, because a route that exists is a route somebody eventually
//!   points at a list.
//! - **Clearing the preview text and leaving it alone are different requests.**
//!   `null` removes it, an absent field keeps it. Folding those together is how
//!   a preheader somebody deleted arrives in an inbox anyway.
//! - **Every route is wrong-tenant tested from both sides**, with each tenant's
//!   list asserted whole so a leak has to show up as a named extra row.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::{harness, harness_on, send};

fn request(method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("GET", uri, Some(token), None)).await
}

async fn post(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, request("POST", uri, Some(token), Some(body))).await
}

async fn patch(app: &Router, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send(app, request("PATCH", uri, Some(token), Some(body))).await
}

async fn delete(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("DELETE", uri, Some(token), None)).await
}

/// The body the Docs editor writes — one of each block a campaign may carry.
fn a_body() -> Value {
    json!({
        "schema_version": 1,
        "blocks": [
            { "type": "heading", "id": "h1", "level": 1, "text": "Spring prices" },
            { "type": "paragraph", "id": "p1", "text": "Everything below is per litre." },
            { "type": "table", "id": "t1", "rows": [["Product", "Price"], ["Oil", "€12"]] },
            { "type": "code", "id": "c1", "code": "curl https://alo", "language": "bash" },
        ],
    })
}

/// The subjects a list answer names, in the order it returned them.
fn subjects(body: &Value) -> Vec<String> {
    body["campaigns"]
        .as_array()
        .unwrap_or_else(|| panic!("no campaigns array in {body}"))
        .iter()
        .map(|c| c["subject"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The id of a created campaign.
fn id_of(body: &Value) -> String {
    body["campaign"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no campaign id in {body}"))
        .to_owned()
}

#[tokio::test]
async fn every_route_needs_a_token() {
    let h = harness("crec-auth").await;
    for (method, uri) in [
        ("GET", "/campaigns/campaigns"),
        ("POST", "/campaigns/campaigns"),
        ("GET", "/campaigns/campaigns/anything"),
        ("PATCH", "/campaigns/campaigns/anything"),
        ("DELETE", "/campaigns/campaigns/anything"),
    ] {
        let body = matches!(method, "POST" | "PATCH").then(|| json!({}));
        let (status, _) = send(&h.app, request(method, uri, None, body)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} answered without a token"
        );
    }
}

#[tokio::test]
async fn a_letter_written_through_the_api_reads_back_exactly() {
    let h = harness("crec-write").await;

    let (status, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({
            "subject": "  Spring prices  ",
            "preheader": "Ten per cent off until Friday",
            "topic": "Monthly Newsletter",
            "content": a_body(),
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["campaign"]["subject"], "Spring prices");
    assert_eq!(
        created["campaign"]["preheader"],
        "Ten per cent off until Friday"
    );
    assert_eq!(created["campaign"]["topic"], "Monthly Newsletter");
    assert_eq!(
        created["campaign"]["content"],
        a_body(),
        "the body a composer sent must be the body it gets back — C3.2 compiles this JSON"
    );
    assert_eq!(created["campaign"]["createdBy"], h.account_id);

    let id = id_of(&created);
    let (status, read) = get(&h.app, &h.token, &format!("/campaigns/campaigns/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["campaign"], created["campaign"]);

    // The list carries how far along it is, and not the body itself.
    let (status, list) = get(&h.app, &h.token, "/campaigns/campaigns").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(subjects(&list), vec!["Spring prices".to_owned()]);
    assert_eq!(list["campaigns"][0]["blocks"], 4);
    assert!(
        list["campaigns"][0]["content"].is_null(),
        "a list carries no bodies: half a body is a thing a client can save back"
    );
}

#[tokio::test]
async fn a_campaign_can_be_named_before_it_is_written() {
    let h = harness("crec-empty").await;
    let (status, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({ "subject": "Not written yet", "topic": "Product news" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        created["campaign"]["content"],
        json!({ "schema_version": 1, "blocks": [] }),
        "the composer opens on an empty body, and that is a real state"
    );
    assert!(created["campaign"]["preheader"].is_null());
}

#[tokio::test]
async fn clearing_the_preview_text_and_leaving_it_alone_are_different_requests() {
    let h = harness("crec-preheader").await;
    let (_, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({
            "subject": "Spring prices",
            "preheader": "Ten per cent off until Friday",
            "topic": "Monthly Newsletter",
        }),
    )
    .await;
    let id = id_of(&created);
    let uri = format!("/campaigns/campaigns/{id}");

    // Stating something else leaves the preheader where it was.
    let (status, edited) = patch(
        &h.app,
        &h.token,
        &uri,
        json!({ "subject": "Spring, again" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["campaign"]["subject"], "Spring, again");
    assert_eq!(
        edited["campaign"]["preheader"], "Ten per cent off until Friday",
        "an absent field is not an instruction"
    );

    // Null removes it — the obvious way for a client to clear a field.
    let (status, cleared) = patch(&h.app, &h.token, &uri, json!({ "preheader": null })).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(
        cleared["campaign"]["preheader"].is_null(),
        "a preheader somebody deleted must not arrive in an inbox"
    );
    // And so does the empty string a form sends when the box is emptied.
    let (_, set) = patch(&h.app, &h.token, &uri, json!({ "preheader": "Back again" })).await;
    assert_eq!(set["campaign"]["preheader"], "Back again");
    let (_, emptied) = patch(&h.app, &h.token, &uri, json!({ "preheader": "   " })).await;
    assert!(emptied["campaign"]["preheader"].is_null());
}

#[tokio::test]
async fn a_stated_body_replaces_the_stored_one_whole() {
    let h = harness("crec-patch-body").await;
    let (_, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({ "subject": "Spring prices", "topic": "Monthly Newsletter", "content": a_body() }),
    )
    .await;
    let id = id_of(&created);
    let uri = format!("/campaigns/campaigns/{id}");

    // A patch that does not mention the body leaves every block of it.
    let (_, renamed) = patch(&h.app, &h.token, &uri, json!({ "subject": "Renamed" })).await;
    assert_eq!(renamed["campaign"]["content"], a_body());

    // A patch that does mention it replaces it whole — never merged block by
    // block, which is how a mail loses its last paragraph.
    let shorter = json!({
        "schema_version": 1,
        "blocks": [{ "type": "paragraph", "id": "p9", "text": "Just this." }],
    });
    let (status, rewritten) = patch(&h.app, &h.token, &uri, json!({ "content": shorter })).await;
    assert_eq!(status, StatusCode::OK, "{rewritten}");
    assert_eq!(rewritten["campaign"]["content"], shorter);
    assert_eq!(
        rewritten["campaign"]["subject"], "Renamed",
        "replacing the body says nothing about the subject"
    );
}

#[tokio::test]
async fn a_body_a_mail_client_cannot_draw_is_refused_with_the_reason() {
    let h = harness("crec-refuse").await;

    // Missing the two fields that decide what arrives and what a recipient can
    // stop.
    for (body, expected) in [
        (json!({ "topic": "Monthly Newsletter" }), "subject"),
        (json!({ "subject": "Spring prices" }), "topic"),
    ] {
        let (status, answer) = post(&h.app, &h.token, "/campaigns/campaigns", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
        assert!(
            answer["detail"]
                .as_str()
                .unwrap_or_default()
                .contains(expected),
            "the error must name the field: {answer}"
        );
    }

    // A formula: a Docs block somebody legitimately wrote, refused by name
    // rather than dropped or sent as LaTeX.
    let (status, answer) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({
            "subject": "Spring prices",
            "topic": "Monthly Newsletter",
            "content": {
                "schema_version": 1,
                "blocks": [{ "type": "equation", "id": "e1", "latex": "x^2", "numbered": true }],
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    let detail = answer["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("formula") && detail.contains("picture"),
        "the writer is told why, while they are still writing: {answer}"
    );

    // A body from a newer build is refused by version rather than half-read.
    let (status, answer) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({
            "subject": "Spring prices",
            "topic": "Monthly Newsletter",
            "content": { "schema_version": 99, "blocks": [] },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert!(
        answer["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("schema_version"),
        "{answer}"
    );

    // Nothing above was written.
    let (_, list) = get(&h.app, &h.token, "/campaigns/campaigns").await;
    assert!(subjects(&list).is_empty(), "{list}");
}

#[tokio::test]
async fn nothing_on_this_surface_sends() {
    let h = harness("crec-nosend").await;
    let (_, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({ "subject": "Spring prices", "topic": "Monthly Newsletter" }),
    )
    .await;
    let id = id_of(&created);

    // Absent rather than guarded: a route that exists is a route somebody
    // eventually points at a list. ADR 0044 §1 blocks sending on a second
    // egress IP, which is a purchase and not a screen.
    for uri in [
        format!("/campaigns/campaigns/{id}/send"),
        format!("/campaigns/campaigns/{id}/test-send"),
        "/campaigns/sends".to_owned(),
    ] {
        let (status, _) = post(&h.app, &h.token, &uri, json!({})).await;
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
            "{uri} answered {status} — this wave has no path that mails anybody"
        );
    }
}

#[tokio::test]
async fn a_neighbours_campaign_is_not_reachable_from_either_side() {
    let ours = harness("crec-tenancy-a").await;
    let theirs = harness_on(std::sync::Arc::clone(&ours.store), "crec-tenancy-b").await;

    let (_, ours_created) = post(
        &ours.app,
        &ours.token,
        "/campaigns/campaigns",
        json!({ "subject": "Our spring mail", "topic": "Monthly Newsletter", "content": a_body() }),
    )
    .await;
    let ours_id = id_of(&ours_created);
    let (_, theirs_created) = post(
        &theirs.app,
        &theirs.token,
        "/campaigns/campaigns",
        json!({ "subject": "Their spring mail", "topic": "Product news" }),
    )
    .await;
    let theirs_id = id_of(&theirs_created);

    // Every route that takes an id, driven with the neighbour's id.
    let uri = format!("/campaigns/campaigns/{ours_id}");
    let (status, _) = get(&theirs.app, &theirs.token, &uri).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a neighbour's letter is not readable"
    );
    let (status, _) = patch(
        &theirs.app,
        &theirs.token,
        &uri,
        json!({ "subject": "Ours now" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "nor rewritable");
    let (status, _) = delete(&theirs.app, &theirs.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "nor deletable");
    // And the other way round, so a leak cannot be one-directional.
    let (status, _) = get(
        &ours.app,
        &ours.token,
        &format!("/campaigns/campaigns/{theirs_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Both lists, whole.
    let (_, ours_list) = get(&ours.app, &ours.token, "/campaigns/campaigns").await;
    assert_eq!(subjects(&ours_list), vec!["Our spring mail".to_owned()]);
    let (_, theirs_list) = get(&theirs.app, &theirs.token, "/campaigns/campaigns").await;
    assert_eq!(subjects(&theirs_list), vec!["Their spring mail".to_owned()]);

    // And ours is untouched by everything the neighbour tried.
    let (_, ours_read) = get(&ours.app, &ours.token, &uri).await;
    assert_eq!(ours_read["campaign"], ours_created["campaign"]);
}

#[tokio::test]
async fn deleting_a_letter_is_a_delete_and_a_second_one_is_a_404() {
    let h = harness("crec-delete").await;
    let (_, created) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({ "subject": "Spring prices", "topic": "Monthly Newsletter" }),
    )
    .await;
    let uri = format!("/campaigns/campaigns/{}", id_of(&created));

    let (status, answer) = delete(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["deleted"], true);
    let (status, _) = get(&h.app, &h.token, &uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(&h.app, &h.token, &uri).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a caller that thought it held a campaign is told it does not"
    );
}

#[tokio::test]
async fn a_malformed_request_is_the_callers_error_rather_than_a_500() {
    let h = harness("crec-malformed").await;
    let broken = Request::builder()
        .method("POST")
        .uri("/campaigns/campaigns")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let (status, _) = send(&h.app, broken).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A block that is not a block this build knows names itself.
    let (status, answer) = post(
        &h.app,
        &h.token,
        "/campaigns/campaigns",
        json!({
            "subject": "Spring prices",
            "topic": "Monthly Newsletter",
            "content": { "schema_version": 1, "blocks": [{ "type": "carousel", "id": "x1" }] },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        answer["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("carousel"),
        "{answer}"
    );
}
