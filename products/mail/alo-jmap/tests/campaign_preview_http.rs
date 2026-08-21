//! `/campaigns/campaigns/{id}/preview`, `/test` and `/campaigns/merge-fields`
//! over the real router (C3.6, ADR 0044).
//!
//! `alo-store`'s `campaign_preview_tenancy` suite proves the rendering and the
//! refusals. What is asserted here is the **edge**:
//!
//! - **The three things the item names arrive over the wire**: the HTML, the
//!   text part, and the merge fields with the fallback flag that is the whole
//!   reason to report them.
//! - **A seed test writes a draft and sends nothing.** The message lands in the
//!   caller's own Drafts, addressed to the caller, carrying **both** parts —
//!   and the campaign is not modified, so asking twice writes two drafts.
//! - **There is still no send on this surface.** `POST …/send` is a `404`/`405`
//!   rather than a `403`, because a route that exists is a route somebody
//!   eventually points at a list.
//! - **Every route is wrong-tenant tested**, including the one that names a
//!   person: a neighbour's campaign is a `404`, and so is a neighbour's
//!   recipient.
//!
//! Runs against the real Postgres from compose (see `tests/common`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use alo_store::{
    ConsentSource, MessageId, NewCampaignConsent, NewCustomer, NewSuppression, SuppressionReason,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use common::{Harness, harness, send};

fn request(method: &str, uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn get(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("GET", uri, Some(token))).await
}

async fn post(app: &Router, token: &str, uri: &str) -> (StatusCode, Value) {
    send(app, request("POST", uri, Some(token))).await
}

/// A letter that greets by name and writes to the address.
fn a_body() -> Value {
    json!({
        "schema_version": 1,
        "blocks": [
            { "type": "heading", "id": "h1", "level": 1, "text": "Hi {{first_name|there}}" },
            { "type": "paragraph", "id": "p1", "text": "We write to {{email|your address}}." },
        ],
    })
}

/// Writes a campaign through the real route and returns its id.
async fn campaign(h: &Harness) -> String {
    let body = json!({
        "subject": "Spring prices for {{first_name|you}}",
        "topic": "Monthly newsletter",
        "content": a_body(),
    });
    let req = Request::builder()
        .method("POST")
        .uri("/campaigns/campaigns")
        .header("authorization", format!("Bearer {}", h.token))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, answer) = send(&h.app, req).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    answer["campaign"]["id"].as_str().unwrap().to_owned()
}

/// Somebody this tenant may mail: a customer with a name, and consent.
async fn reachable(h: &Harness, name: &str, email: &str) {
    h.acc
        .create_billing_customer(&NewCustomer {
            name: name.to_owned(),
            country: "BE".to_owned(),
            email: Some(email.to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    h.acc
        .record_campaign_consent(&NewCampaignConsent {
            address: email,
            source: ConsentSource::Manual,
            source_ref: None,
            statement: "Ticked the newsletter box at the counter",
            occurred_at: None,
        })
        .await
        .unwrap();
}

/// The `(field, value, fellBack)` rows of a preview, in the order returned.
fn fields(preview: &Value) -> Vec<(String, String, bool)> {
    preview["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("no fields array in {preview}"))
        .iter()
        .map(|used| {
            (
                used["field"].as_str().unwrap_or_default().to_owned(),
                used["value"].as_str().unwrap_or_default().to_owned(),
                used["fellBack"].as_bool().unwrap_or_default(),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_vocabulary_is_read_from_the_server_rather_than_hard_coded_by_a_composer() {
    let h = harness("cprevfields").await;
    let (status, answer) = get(&h.app, &h.token, "/campaigns/merge-fields").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        answer["fields"],
        json!(["first_name", "name", "email", "country"]),
        "in the order a composer offers them"
    );
    // Names only: the words describing a field are user-facing strings and live
    // in the i18n catalogues, not in a Rust literal that arrives in English.
    for field in answer["fields"].as_array().unwrap() {
        assert!(field.is_string(), "{field} is not a bare name");
    }
    // Authenticated like the rest of the surface.
    let (status, _) = send(&h.app, request("GET", "/campaigns/merge-fields", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_preview_answers_the_html_the_text_and_the_fields_with_their_fallback_flag() {
    let h = harness("cprevread").await;
    reachable(&h, "Jean Dupont", "jean@cprev.test").await;
    let id = campaign(&h).await;

    let (status, answer) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/preview?unsubscribeText=Uitschrijven"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let preview = &answer["preview"];

    assert_eq!(preview["subject"], "Spring prices for Jean");
    let html = preview["html"].as_str().unwrap();
    let text = preview["text"].as_str().unwrap();
    assert!(
        html.contains("Hi Jean") && html.contains("<table"),
        "{html}"
    );
    assert!(text.contains("Hi Jean") && !text.contains('<'), "{text}");
    assert_eq!(
        preview["against"],
        json!({
            "kind": "recipient",
            "address": "jean@cprev.test",
            "name": "Jean Dupont",
            "country": "BE",
        }),
        "with no `as`, the preview is against the first person the tenant may mail"
    );
    assert_eq!(
        fields(preview),
        vec![
            ("first_name".to_owned(), "Jean".to_owned(), false),
            ("email".to_owned(), "jean@cprev.test".to_owned(), false),
        ]
    );

    // The copy every recipient with nothing recorded gets — asked for by name,
    // and reported as such rather than as somebody's.
    let (status, answer) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/preview?unsubscribeText=Uitschrijven&as=fallbacks"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let preview = &answer["preview"];
    assert_eq!(preview["subject"], "Spring prices for you");
    assert_eq!(
        preview["against"],
        json!({ "kind": "fallbacks", "reason": "asked" })
    );
    assert!(fields(preview).iter().all(|(_, _, fell_back)| *fell_back));
    assert!(
        !preview["html"]
            .as_str()
            .unwrap()
            .contains("jean@cprev.test")
    );

    // A parameter that is neither an address nor the literal is the caller's
    // error, named — not a silent widening to "everybody".
    let (status, _) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/preview?unsubscribeText=Uitschrijven&as=everyone"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_seed_test_writes_a_draft_to_the_caller_and_sends_nothing() {
    let h = harness("cprevtest").await;
    reachable(&h, "Jean Dupont", "jean@cprev.test").await;
    let id = campaign(&h).await;

    let (status, answer) = post(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/test?unsubscribeText=Uitschrijven"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(
        answer["draft"]["to"].as_str(),
        Some(h.email.as_str()),
        "a test copy goes to the colleague who asked and nowhere else"
    );
    assert_eq!(answer["draft"]["subject"], "Spring prices for Jean");
    assert_eq!(answer["against"]["kind"], "recipient");

    // It is a draft in the caller's own Drafts, marked `$draft`, carrying both
    // parts — so it is the `multipart/alternative` a recipient gets rather than
    // a screenshot of one half.
    let draft_id = MessageId::new(answer["draft"]["id"].as_str().unwrap().to_owned());
    let drafts = h
        .acc
        .mailbox_by_role("drafts")
        .await
        .unwrap()
        .expect("Drafts");
    assert_eq!(h.acc.mailbox(&drafts).await.unwrap().total_messages, 1);
    assert!(
        h.acc
            .keywords(&draft_id)
            .await
            .unwrap()
            .contains(&"$draft".to_owned()),
        "a message without $draft is not submittable and is not a draft"
    );
    // Nothing was sent: there is no Sent folder, because nothing put one there.
    assert!(h.acc.mailbox_by_role("sent").await.unwrap().is_none());

    let raw = String::from_utf8_lossy(&h.acc.message_bytes(&draft_id).await.unwrap()).into_owned();
    assert!(raw.contains("multipart/alternative"), "{raw}");
    assert!(raw.contains("text/plain") && raw.contains("text/html"));
    assert!(
        raw.contains(&h.email),
        "from the caller and to the caller: {raw}"
    );
    assert!(
        raw.contains("Spring prices for Jean"),
        "the subject is the letter's own — a test whose subject differs from \
         the real one does not test the subject"
    );

    // Asking twice writes two drafts and changes no campaign — the behaviour of
    // somebody who closed the compose window without sending.
    let (status, _) = post(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/test?unsubscribeText=Uitschrijven"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h.acc.mailbox(&drafts).await.unwrap().total_messages, 2);

    // And there is still no route that puts a campaign in front of anybody.
    let (status, _) = post(&h.app, &h.token, &format!("/campaigns/campaigns/{id}/send")).await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
        "a send route appeared on the campaigns surface: {status}"
    );
}

#[tokio::test]
async fn neither_a_neighbours_letter_nor_a_neighbours_recipient_is_reachable() {
    let ours = harness("cprevours").await;
    let theirs = common::harness_on(ours.store.clone(), "cprevtheirs").await;
    reachable(&ours, "Ann Ours", "ann@cprev.test").await;
    reachable(&theirs, "Bea Theirs", "bea@cprev.test").await;
    let ours_id = campaign(&ours).await;

    // Their token, our campaign: a `404` from an id that is a perfectly good
    // campaign one tenant over.
    for uri in [
        format!("/campaigns/campaigns/{ours_id}/preview?unsubscribeText=Uitschrijven"),
        format!(
            "/campaigns/campaigns/{ours_id}/preview?unsubscribeText=Uitschrijven&as=ann@cprev.test"
        ),
    ] {
        let (status, _) = get(&theirs.app, &theirs.token, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    let (status, _) = post(
        &theirs.app,
        &theirs.token,
        &format!("/campaigns/campaigns/{ours_id}/test?unsubscribeText=Uitschrijven"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Our token, our campaign, their recipient: also a `404`. The address is
    // real, and it is not ours to render a letter to.
    let (status, _) = get(
        &ours.app,
        &ours.token,
        &format!(
            "/campaigns/campaigns/{ours_id}/preview?unsubscribeText=Uitschrijven&as=bea@cprev.test"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Nor is somebody of ours who has left. Suppression is absolute (ADR 0044
    // §2), including for the rehearsal.
    ours.ts
        .suppress_campaign_address(&NewSuppression {
            address: "ann@cprev.test",
            reason: SuppressionReason::Unsubscribe,
            source_ref: None,
            occurred_at: None,
        })
        .await
        .unwrap();
    let (status, _) = get(
        &ours.app,
        &ours.token,
        &format!(
            "/campaigns/campaigns/{ours_id}/preview?unsubscribeText=Uitschrijven&as=ann@cprev.test"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // With nobody left to mail, a preview is still possible and says why it is
    // showing the writer's own words.
    let (status, answer) = get(
        &ours.app,
        &ours.token,
        &format!("/campaigns/campaigns/{ours_id}/preview?unsubscribeText=Uitschrijven"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(
        answer["preview"]["against"],
        json!({ "kind": "fallbacks", "reason": "nobody_to_mail_yet" })
    );
}

#[tokio::test]
async fn a_preview_without_the_footers_words_is_refused_rather_than_guessed() {
    // C2.5 made the unsubscribe footer part of every letter, and its words are
    // the reader's language. The server holds no translations, so a preview
    // that guessed them in English would be a preview of a letter nobody
    // receives — in the one place a recipient looks when they want the mail to
    // stop. Refused by name, with the parameter in the sentence.
    let h = harness("cprevwords").await;
    let id = campaign(&h).await;

    let (status, body) = get(
        &h.app,
        &h.token,
        &format!("/campaigns/campaigns/{id}/preview"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|d| d.contains("unsubscribeText")),
        "the refusal names the parameter: {body}"
    );
}
