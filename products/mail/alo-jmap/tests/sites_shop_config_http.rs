//! The shop-setup proposal route through the real router and Postgres
//! (S3.05b2, ADR 0041).
//!
//! The model is the shared scripted localhost fixture backend; no external AI
//! service is ever called. Pinned here: authentication, the typed
//! unconfigured branch, the parser-enforced flags surviving to the wire
//! verbatim, one repair then a typed refusal, the proposal writing nothing,
//! and the site-editor role being refused at both mounts — this route names
//! prices and VAT, which that role must never see (S2.03a).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::common::model::{scripted_model, use_model};
use crate::common::{Harness, harness, send};

/// The ADR 0041 example the valid fixture was drafted from: every stated
/// amount the envelope may carry (€25, €19,50, €60, €5) is in this sentence.
const DESCRIPTION: &str = "I run pottery workshops in Antwerp and sell two books: \
     Glaze Basics at €25 and Wheel Notes at €19,50. A workshop seat is €60. \
     Shipping is €5 per order.";

fn valid_fixture() -> String {
    include_str!("../../../../platform/alo-ai/tests/fixtures/sites/valid_shop_config.json")
        .to_owned()
}

fn invented_price_fixture() -> String {
    include_str!("../../../../platform/alo-ai/tests/fixtures/sites/near_miss_invented_price.json")
        .to_owned()
}

async fn post(app: &Router, token: Option<&str>, uri: &str, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    send(app, request.body(Body::from(body.to_string())).unwrap()).await
}

#[tokio::test]
async fn proposing_requires_auth_and_unconfigured_is_typed() {
    let h = harness("shop-config-unconfigured").await;
    let body = json!({ "description": DESCRIPTION });

    let (status, problem) = post(&h.app, None, "/sites/shop-config/propose", body.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{problem}");

    let (status, problem) = post(&h.app, Some(&h.token), "/sites/shop-config/propose", body).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    assert_eq!(problem["reason"], "unconfigured");
    assert!(problem["detail"].as_str().unwrap().contains("by hand"));
}

#[tokio::test]
async fn the_request_is_validated_before_any_model_is_called() {
    let h = harness("shop-config-validation").await;

    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        "/sites/shop-config/propose",
        json!({ "description": "   " }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    assert!(problem["detail"].as_str().unwrap().contains("Describe"));

    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        "/sites/shop-config/propose",
        json!({ "description": "a".repeat(8_001) }),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{problem}");

    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        "/sites/shop-config/propose",
        json!({ "description": DESCRIPTION, "apply": true }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
}

#[tokio::test]
async fn the_fixture_proposal_reaches_the_wire_flagged_and_writes_nothing() {
    let h = harness("shop-config-fixture").await;
    let (base_url, seen) = scripted_model(vec![valid_fixture()]).await;
    use_model(&h, &base_url).await;

    let (status, body) = post(
        &h.app,
        Some(&h.token),
        "/sites/shop-config/propose",
        json!({ "description": DESCRIPTION }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The parser-enforced envelope, verbatim on the wire: stated prices carry
    // the description's own amounts, VAT is structurally a guess under the
    // key that says so, shipping is the stated flat rate.
    let proposal = &body["proposal"];
    assert_eq!(proposal["schema_version"], 1);
    let items = proposal["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["name"], "Glaze Basics");
    assert_eq!(items[0]["kind"], "stock");
    assert_eq!(
        items[0]["price"],
        json!({ "state": "stated", "cents": 2500 })
    );
    assert_eq!(items[0]["vat_guess"]["rate_bp"], 600);
    assert!(
        items[0]["vat_guess"]["basis"]
            .as_str()
            .unwrap()
            .contains("printed books")
    );
    assert!(
        items[0].get("vat").is_none(),
        "VAT must only ever appear under the key that calls it a guess"
    );
    assert_eq!(items[2]["kind"], "dated");
    assert_eq!(
        proposal["shipping"],
        json!({ "state": "stated", "cents": 500 })
    );

    // The model was shown the owner's description, once.
    {
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0]["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("pottery workshops in Antwerp")
        );
    }

    // Proposing applies nothing: no Billing product exists until the approval
    // screen creates one through the owned routes.
    assert!(h.acc.billing_products(true).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_invented_price_gets_one_repair_then_a_typed_refusal() {
    let h = harness("shop-config-invented").await;
    // 2400 is stated nowhere in the description; the parser refuses it, the
    // one repair turn returns the same envelope, and the caller gets 422.
    let (base_url, seen) =
        scripted_model(vec![invented_price_fixture(), invented_price_fixture()]).await;
    use_model(&h, &base_url).await;

    let (status, problem) = post(
        &h.app,
        Some(&h.token),
        "/sites/shop-config/propose",
        json!({ "description": DESCRIPTION }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert_eq!(problem["reason"], "invalid_proposal");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("Nothing was changed")
    );
    assert_eq!(seen.lock().unwrap().len(), 2, "exactly one repair turn");
    assert!(h.acc.billing_products(true).await.unwrap().is_empty());
}

/// The prerequisite the item carries (S2.03a): a restricted site-editor
/// collaborator must never see Billing-side facts, and this proposal names
/// prices and VAT. The static path is outside the site-editor allowlist, so
/// the one scoping middleware refuses it — at the bare mount and at the
/// `/api` mount production actually proxies.
#[tokio::test]
async fn a_site_editor_is_refused_at_both_mounts() {
    let h = harness("shop-config-editor").await;
    let (token, editor) = site_editor(&h).await;
    let site = h
        .acc
        .create_site("Shop config", &format!("shopcfg-{}", suffix(&h)))
        .await
        .unwrap();
    h.ts.grant_site_editor(&editor, &site, &h.user)
        .await
        .unwrap();

    for uri in [
        "/sites/shop-config/propose",
        "/api/sites/shop-config/propose",
    ] {
        let (status, problem) = post(
            &h.app,
            Some(&token),
            uri,
            json!({ "description": DESCRIPTION }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {problem}");
    }
}

async fn site_editor(h: &Harness) -> (String, alo_store::UserId) {
    let email = format!("shop-config-collab-{}@example.test", h.tenant);
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

fn suffix(h: &Harness) -> String {
    h.tenant
        .as_str()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .take(20)
        .collect()
}
